//! Factory's Action field through the real binary (O-I #1002): `factory
//! actions --json` lists it in oi.owner-action-field/v1, and `factory action
//! run <id> -` commissions work and reads it back through Factory's own
//! routes, refusing an input the route does not take before anything runs.

use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

#[path = "support/executable.rs"]
mod executable;
#[path = "support/praxis_receipt_fixture.rs"]
mod praxis_receipt_fixture;

fn factory(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the factory binary runs");
    if let Some(stdin) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn json_of(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "factory refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("factory answers JSON")
}

#[test]
fn the_field_lists_factorys_actions() {
    let field = json_of(&factory(&["actions", "--json"], None));
    assert_eq!(field["schema"], "oi.owner-action-field/v1");
    assert_eq!(field["owner"], "factory");
    let ids = field["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    for id in [
        "factory.development.commission",
        "factory.development.commission-read",
        "factory.attempt.launch",
        "factory.attempt.action",
        "factory.attempt.read",
    ] {
        assert!(ids.contains(&id), "{id} is in the field: {ids:?}");
    }
}

#[test]
fn a_commission_runs_through_the_field_and_reads_back() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("factory-development.json");
    let state = state.to_str().unwrap();
    let key = "action-field-commission";
    let request = json!({
        "contract": "factory.commission-request/v1", "requestRef": format!("commission-request:{key}"),
        "projectKey": key, "purpose": "Commission through the Action field",
        "frontier": "Declared source; no execution has been requested",
        "runDestination": format!("{key}/first"), "writeOwner": "factory",
        "commissionedAt": "2026-11-13T00:00:00Z",
        "participantRequirements": [{"ref": "agent/action-field", "description": "The field's test principal",
            "sourceOwner": "test-contract", "sourceRef": "source:action-field", "sourceRevision": "action-field-r1"}],
        "rootAct": {"actRef": format!("act:{key}"), "agentRef": "agent/action-field",
            "purpose": "Commission through the Action field", "scopeRefs": ["issue:O-I-1002"],
            "standing": "commissioned-not-executed"}
    });
    let input = json!({"state": state, "request": request}).to_string();
    let receipt = json_of(&factory(
        &["action", "run", "factory.development.commission", "-"],
        Some(&input),
    ));
    assert_eq!(
        receipt["contract"], "factory.commission-receipt/v1",
        "{receipt}"
    );
    assert!(receipt["commission"]["runRef"].is_string(), "{receipt}");

    let read = json_of(&factory(
        &[
            "action",
            "run",
            "factory.development.commission-read",
            &json!({"state": state, "request_ref": format!("commission-request:{key}")})
                .to_string(),
        ],
        None,
    ));
    assert_eq!(read["contract"], "factory.commission-reading/v1", "{read}");
    assert_eq!(
        read["commission"]["runRef"], receipt["commission"]["runRef"],
        "{read}"
    );

    // The same request again is Factory's own replay, not a second Run.
    let replay = json_of(&factory(
        &["action", "run", "factory.development.commission", "-"],
        Some(&input),
    ));
    assert_eq!(
        replay["commission"]["runRef"], receipt["commission"]["runRef"],
        "{replay}"
    );
}

#[test]
fn an_input_the_route_does_not_take_is_refused_before_anything_runs() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("never-written.json");
    let refused = factory(
        &["action", "run", "factory.development.commission", "-"],
        Some(&json!({"state": state, "request": {}, "colour": "red"}).to_string()),
    );
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("takes no input `colour`"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!state.exists(), "nothing ran");
    let unknown = factory(&["action", "run", "factory.teleport", "{}"], None);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("not in Factory's Action field"));
}

/// `factory action run <id> -` with the input on stdin and the host's token as given (none, or one Factory must not read).
fn run_session_act(id: &str, input: &Value, token: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(["action", "run", id, "-"])
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(token) = token {
        command.env("CENTRAL_NATIVE_TOKEN", token);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// REPLACED (review F8): a session's acts read the host's token and resolved the grant in Factory; the host resolves it now.
#[test]
fn a_sessions_acts_take_the_grant_the_host_resolved_and_never_read_its_token() {
    let field = json_of(&factory(&["actions", "--json"], None));
    let ids = field["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    for id in [
        "factory.session.list",
        "factory.attempt.retry",
        "factory.attempt.choose",
    ] {
        assert!(ids.contains(&id), "{id} is in the field: {ids:?}");
    }
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join(".factory/development-state.json");
    let grant = "central:native-action-authority#human:owner";
    for (id, input) in [
        (
            "factory.task.run",
            json!({"state": state, "run_ref": "run:x", "session": "agent-session/x"}),
        ),
        (
            "factory.task.stop",
            json!({"state": state, "run_ref": "run:x", "attempt_ref": "attempt:x"}),
        ),
        (
            "factory.attempt.retry",
            json!({"state": state, "run_ref": "run:x", "unit_ref": "unit:x"}),
        ),
        (
            "factory.attempt.choose",
            json!({"state": state, "run_ref": "run:x", "attempt_ref": "attempt:x"}),
        ),
    ] {
        // The person's press presents the grant the host resolved: without it the field refuses before anything runs.
        let refused = run_session_act(id, &input, Some("a-token-factory-never-reads"));
        let words = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{id} ran with no grant");
        assert!(
            words.contains("needs its input `authority`"),
            "{id}: {words}"
        );
        // With it, and no token at all, the act goes past the grant: what stops it here is the Run it names.
        let mut granted = input.clone();
        granted["authority"] = json!(grant);
        let refused = run_session_act(id, &granted, None);
        let words = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{id} ran on no Run");
        assert!(
            !words.contains("grant") && !words.contains("CENTRAL_NATIVE_TOKEN"),
            "{id} stopped at the grant: {words}"
        );
        // A grant is named in its contract's form, or refused in those words.
        granted["authority"] = json!("human:owner");
        let refused = run_session_act(id, &granted, None);
        let words = String::from_utf8_lossy(&refused.stderr);
        assert!(
            words.contains("central:native-action-authority#<principal_ref>"),
            "{id}: {words}"
        );
    }
    assert!(!directory.path().join(".factory").exists(), "nothing moved");
}

#[test]
fn the_sessions_factory_keeps_are_listed_open_and_ended_in_the_order_they_opened() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join(".factory/development-state.json");
    // Nothing kept yet: no sessions.
    let none = json_of(&factory(
        &["action", "run", "factory.session.list", "-"],
        Some(&json!({"state": state}).to_string()),
    ));
    assert_eq!(none["contract"], "factory.session-list/v1");
    assert_eq!(none["sessions"], json!([]));
    let sessions = directory.path().join(".factory/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let kept = |name: &str, document: Value| {
        std::fs::write(sessions.join(name), document.to_string()).unwrap();
    };
    kept(
        "b.session.json",
        json!({"contract": "factory.session/v1", "state": "ended", "session": "agent-session/b",
            "opened_at": "2026-11-14T02:00:00Z", "ended_at": "2026-11-14T02:30:00Z", "tasks": []}),
    );
    kept(
        "a.session.json",
        json!({"contract": "factory.session/v1", "state": "open", "session": "agent-session/a",
            "opened_at": "2026-11-14T01:00:00Z", "tasks": [{"run_ref": "run:1", "task": "Write the notes"}]}),
    );
    // A task's log beside them, and a document of another contract, are no sessions.
    kept("attempt-1.log.jsonl", json!({"kind": "sent"}));
    kept("c.session.json", json!({"contract": "something/v1"}));
    let listed = json_of(&factory(
        &["action", "run", "factory.session.list", "-"],
        Some(&json!({"state": state}).to_string()),
    ));
    let order: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["session"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["agent-session/a", "agent-session/b"]);
    assert_eq!(listed["sessions"][0]["tasks"][0]["run_ref"], "run:1");
    assert_eq!(listed["sessions"][1]["state"], "ended");
}

/// AIKit's side of a session's task, served from the answers beside it (O-I #1006): the agent's binding, its status and
/// practice, the selection receipt for the session, the World's NOW service and its preparation. A stand-in for the owner's
/// protocol, so Factory's own routes run against it; never an installed AIKit. It serves no session view, so a task's words
/// never go.
#[cfg(unix)]
const AIKIT: &str = r#"#!/bin/sh
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "$1" = "-C" ]; then shift 2; fi
printf '%s\n' "$*" >> "$dir/calls.log"
answer() { cat "$dir/$1.json"; exit 0; }
case "$1 $2" in
    "session-space encounter-agency-read") answer binding ;;
    "session-space encounter")
        case "$4" in
            *'"action":"status"'*) answer status ;;
        esac ;;
    "compose model") answer selection ;;
    "--json client") answer clients ;;
    "--json set") answer set ;;
    "--json now-context")
        case "$3" in
            service) answer service ;;
            inspect) answer inspect ;;
            prepare) printf '%s\n' '{"ok":true,"data":{"prepared":true}}'; exit 0 ;;
        esac ;;
esac
printf '%s\n' '{"ok":false,"error":{"message":"this stand-in does not serve it"}}'
exit 2
"#;

/// Central's side: the work policy's revision and the task's NOW.
#[cfg(unix)]
const CTRL: &str = r#"#!/bin/sh
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf '%s\n' "$*" >> "$dir/calls.log"
case "$*" in
    *" action run central.work.policy "*) printf '%s\n' '{"ok":true,"data":{"revision":"work-policy/1"}}' ;;
    *" action run central.now.allocate "*) printf '%s\n' '{"ok":true,"data":{"now_ref":"now:session-task"}}' ;;
    *) printf '%s\n' '{"ok":false,"error":{"message":"this stand-in does not serve it"}}'; exit 2 ;;
esac
"#;

const GRANT: &str = "central:native-action-authority#human:owner";
const PRACTICE: &str = "skill/notes-writing";

/// A World with one project, `notes`, and a session AIKit answers for with `selection`, its receipt for the session.
#[cfg(unix)]
struct Seat {
    _directory: tempfile::TempDir,
    home: PathBuf,
    world: PathBuf,
    owners: PathBuf,
    state: PathBuf,
    session: String,
}

#[cfg(unix)]
impl Seat {
    fn new(selection: &Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let world = directory.path().join("world");
        let home = directory.path().join("home");
        let owners = directory.path().join("owners");
        let project = world.join("Work/notes");
        for folder in [
            world.join("Control"),
            project.join("ProjectCentral"),
            home.clone(),
            owners.clone(),
        ] {
            std::fs::create_dir_all(folder).unwrap();
        }
        std::fs::write(
            project.join("ProjectCentral/project.json"),
            json!({"schema": "central.project/v1", "project_id": "central-project:Notes"})
                .to_string(),
        )
        .unwrap();
        executable::write_executable(owners.join("aikit"), AIKIT);
        executable::write_executable(owners.join("ctrl"), CTRL);
        let basis = &selection["ranking_explanation"]["basis"];
        let composition = &basis["harness_composition"];
        let source = &basis["composition_target_basis"]["agency_source"];
        let harness = &basis["composition_target_basis"]["resident_body_basis"]["harness_profile"];
        let session = composition["session"].as_str().unwrap().to_owned();
        let seat = Seat {
            _directory: directory,
            home,
            state: project.join(".factory/development-state.json"),
            world,
            owners,
            session,
        };
        seat.serve(
            "binding",
            json!({"agent_ref": composition["agent"], "agency_ref": composition["agency"],
            "revision": "binding-revision/1", "allowed_senders": ["human:owner"],
            "agency_source": {"source_ref": source["source_ref"], "revision": source["revision"]}}),
        );
        seat.serve(
            "status",
            json!({"agent_session": seat.session, "provider": {"id": harness}}),
        );
        seat.serve(
            "clients",
            json!({"clients": [{"client": harness, "harness": harness, "skill_sets": ["notes"]}]}),
        );
        seat.serve("set", json!({"projected": [PRACTICE]}));
        seat.serve(
            "selection",
            json!({"schema": "aikit.model-realisation/v2", "model_ref": selection["model_ref"],
            "selected": true, "executed": false, "factory_selection": selection}),
        );
        let election = seat.owners.join("election.json");
        std::fs::write(
            &election,
            json!({"url": "redis://127.0.0.1:6390/0"}).to_string(),
        )
        .unwrap();
        seat.serve(
            "service",
            json!({"state": "running", "election_file": election}),
        );
        seat
    }

    /// One answer of AIKit's, in its envelope.
    fn serve(&self, name: &str, data: Value) {
        std::fs::write(
            self.owners.join(format!("{name}.json")),
            json!({"ok": true, "data": data}).to_string(),
        )
        .unwrap();
    }

    /// `factory action run <id> -`, with the stand-ins as the owners, a home of its own and no token.
    fn act(&self, id: &str, input: &Value) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
            .args(["action", "run", id, "-"])
            .env("HOME", &self.home)
            .env("OI_AIKIT_BIN", self.owners.join("aikit"))
            .env("OI_CENTRAL_CTRL_BIN", self.owners.join("ctrl"))
            .env_remove("CENTRAL_NATIVE_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// A task written on the session, and the praxis AIKit resolves for its unit in the task's NOW: its Run and unit.
    fn written(&self, task: &str) -> (String, String) {
        let written = json_of(&self.act(
            "factory.task.write",
            &json!({"world": self.world, "project": "notes", "session": self.session, "instruction": task}),
        ));
        let run_ref = written["run_ref"].as_str().unwrap().to_owned();
        let unit_ref = written["unit_ref"].as_str().unwrap().to_owned();
        let practice = std::collections::BTreeSet::from([PRACTICE.to_owned()]);
        self.serve("inspect", json!({"schema": "aikit.now-context-inspection/v1", "currentVersion": 0,
            "prepared": {"praxis": praxis_receipt_fixture::resolved_receipt(&unit_ref, &practice)}}));
        (run_ref, unit_ref)
    }

    fn reading(&self, run_ref: &str) -> epilogos_factory::attempt_runtime::FactoryAttemptReading {
        epilogos_factory::attempt_runtime::FileAttemptStore::open_run(
            &self.state,
            run_ref.parse().unwrap(),
        )
        .unwrap()
        .reading()
        .unwrap()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.owners.join("calls.log")).unwrap_or_default()
    }
}

/// The words of a task's send that never went: the stand-ins serve no session view.
#[cfg(unix)]
const UNSENT: &str = "the session's view could not be read before the task's words went";

#[cfg(unix)]
fn failed_unsent(
    reading: &epilogos_factory::attempt_runtime::FactoryAttemptReading,
    record: &epilogos_factory::attempt_runtime::FactoryAttemptRecord,
) -> bool {
    record
        .failure_evidence_refs
        .iter()
        .any(|evidence| evidence.starts_with("factory-session-unsent:"))
        && reading.legs[&record.workflow_unit_ref].status
            == epilogos_factory::orchestration::LegStatus::Failed
}

/// O-I #1006 (re-check 2c): factory.task.run launches the task through Factory's own launch, on the receipt AIKit answers for
/// the session it opened (`aikit compose model selection`): the attempt is bound to the receipt's model, provider, route,
/// harness, session and Agency, under the launch's budget, with the praxis AIKit prepared in the task's NOW. A session whose
/// provider is disclosed (codex) and one whose provider is not (gemini) launch alike. The stand-ins serve no session view, so
/// the words never go, and the attempt is failed, never sent, not left Active.
#[cfg(unix)]
#[test]
fn a_task_runs_through_factorys_launch_on_the_receipt_aikit_answers_for_its_session() {
    for fixture in [
        include_str!("fixtures/aikit-seated-codex-selection.json"),
        include_str!("fixtures/aikit-seated-gemini-selection.json"),
    ] {
        let selection: Value = serde_json::from_str(fixture).unwrap();
        let receipt = &selection["ranking_explanation"];
        let basis = &receipt["basis"];
        let seat = Seat::new(&selection);
        let (run_ref, unit_ref) = seat.written("Write the notes");
        let ran = seat.act(
            "factory.task.run",
            &json!({"state": seat.state, "run_ref": run_ref, "session": seat.session, "authority": GRANT}),
        );
        let words = String::from_utf8_lossy(&ran.stderr);
        assert!(
            !ran.status.success() && words.contains(UNSENT),
            "{} {words}",
            receipt["harness_ref"]
        );
        let calls = seat.calls();
        assert!(
            calls.contains(&format!(
                "compose model selection --session {}",
                seat.session
            )),
            "{calls}"
        );
        assert!(calls.contains("action run central.now.allocate"), "{calls}");

        let reading = seat.reading(&run_ref);
        let [attempt] = reading.attempts.as_slice() else {
            panic!("one attempt: {:?}", reading.attempts)
        };
        assert_eq!(attempt.workflow_unit_ref.to_string(), unit_ref);
        let disposition = &attempt.disposition;
        // AIKit's receipt, as AIKit answered it, and the body it binds.
        assert_eq!(json!(disposition.selection.selection), selection);
        let body = &disposition.body;
        assert_eq!(json!(body.model_ref), selection["model_ref"]);
        assert_eq!(json!(body.provider_ref), selection["provider_ref"]);
        assert_eq!(json!(body.route_ref), receipt["route_ref"]);
        assert_eq!(json!(body.harness_ref), receipt["harness_ref"]);
        assert_eq!(
            json!(body.harness_composition_ref),
            receipt["harness_composition_ref"]
        );
        assert_eq!(body.agent_session_ref, seat.session);
        assert_eq!(json!(body.session_space_ref), basis["native"]["space"]);
        let participant = &disposition.participant;
        assert_eq!(
            json!(participant.agent_ref),
            basis["harness_composition"]["agent"]
        );
        assert_eq!(
            json!(participant.agency_ref),
            basis["harness_composition"]["agency"]
        );
        assert_eq!(
            json!(participant.world_binding_ref),
            basis["composition_target_basis"]["world_binding_ref"]
        );
        // The praxis AIKit prepared in the task's NOW, and the launch's budget.
        assert_eq!(
            disposition.praxis_receipt.as_ref().unwrap()["units"][0]["workflow_unit_ref"],
            json!(unit_ref)
        );
        assert_eq!(
            disposition.now_context_ref.as_deref(),
            Some("now:session-task")
        );
        assert_eq!(disposition.budget.maximum_attempts, Some(3));
        assert_eq!(
            disposition.budget.retry_grant_ref.as_deref(),
            Some(format!("retry-grant:{}", attempt.task_ref).as_str())
        );
        assert!(
            failed_unsent(&reading, attempt),
            "{:?}",
            attempt.failure_evidence_refs
        );
    }
}

/// O-I #1006 (re-check 2, F322-2): a task runs on the session it was written on, where its Return is addressed and where
/// Retry runs it again. factory.task.run refuses, in words, a session that is not the task's own, before AIKit is asked
/// anything, and launches nothing: a task written on one session and run on another would leave Retry recording a Retry
/// that sends nothing.
#[cfg(unix)]
#[test]
fn a_task_runs_only_on_the_session_it_was_written_on() {
    let selection: Value =
        serde_json::from_str(include_str!("fixtures/aikit-seated-codex-selection.json")).unwrap();
    let seat = Seat::new(&selection);
    let (run_ref, _) = seat.written("Write the notes");
    let other = "agent-session/another-track";
    let ran = seat.act(
        "factory.task.run",
        &json!({"state": seat.state, "run_ref": run_ref, "session": other, "authority": GRANT}),
    );
    let words = String::from_utf8_lossy(&ran.stderr);
    assert!(
        !ran.status.success() && words.contains("was not written on") && words.contains(other),
        "{} {words}",
        String::from_utf8_lossy(&ran.stdout)
    );
    let calls = seat.calls();
    assert!(!calls.contains("compose model selection"), "{calls}");
    assert!(
        seat.reading(&run_ref).attempts.is_empty(),
        "{:?}",
        seat.reading(&run_ref).attempts
    );
}

/// O-I #1006 (re-check 2b): a task run on a session AIKit realised under a model policy (`aikit compose --session S --realise
/// --model M`) launches on AIKit's explicit-pin receipt, not its seated-session one, and is still the session's task: Retry
/// runs it again on the session, its words sent there. It never records a Retry that sends nothing and leaves its attempt
/// Active. The stand-ins serve no session view, so the retried words never go either, and the new attempt is failed.
#[cfg(unix)]
#[test]
fn a_retry_runs_a_sessions_task_again_on_its_session_whatever_receipt_it_launched_on() {
    let selection: Value =
        serde_json::from_str(include_str!("fixtures/aikit-explicit-pi-selection.json")).unwrap();
    assert_eq!(
        selection["ranking_explanation"]["selection_kind"],
        "explicit-pin"
    );
    let seat = Seat::new(&selection);
    let (run_ref, unit_ref) = seat.written("Write the notes");
    let ran = seat.act(
        "factory.task.run",
        &json!({"state": seat.state, "run_ref": run_ref, "session": seat.session, "authority": GRANT}),
    );
    assert!(!ran.status.success() && String::from_utf8_lossy(&ran.stderr).contains(UNSENT));
    let retried = seat.act(
        "factory.attempt.retry",
        &json!({"state": seat.state, "run_ref": run_ref, "unit_ref": unit_ref, "authority": GRANT}),
    );
    let words = String::from_utf8_lossy(&retried.stderr);
    assert!(
        !retried.status.success() && words.contains(UNSENT),
        "the retry did not go to the session: {} {words}",
        String::from_utf8_lossy(&retried.stdout)
    );
    let reading = seat.reading(&run_ref);
    let [first, again] = reading.attempts.as_slice() else {
        panic!("the task and its retry: {:?}", reading.attempts)
    };
    assert_eq!(again.task_ref, first.task_ref);
    assert_eq!(again.disposition.body.agent_session_ref, seat.session);
    assert!(
        failed_unsent(&reading, again),
        "{:?}",
        again.failure_evidence_refs
    );
    // The session was read for each send: the task's and its retry's.
    let views = seat
        .calls()
        .lines()
        .filter(|call| {
            call.starts_with("session-space encounter --request-json") && call.contains("\"view\"")
        })
        .count();
    assert_eq!(views, 2, "{}", seat.calls());
}
