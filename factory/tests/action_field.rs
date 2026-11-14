//! Factory's Action field through the real binary (O-I #1002): `factory
//! actions --json` lists it in oi.owner-action-field/v1, and `factory action
//! run <id> -` commissions work and reads it back through Factory's own
//! routes, refusing an input the route does not take before anything runs.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};

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

#[test]
fn a_sessions_retry_and_choice_are_in_the_field_and_ask_for_the_persons_grant_before_anything_moves(
) {
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
    // A retry and a choice are the person's presses: with no token handed, Factory asks for the grant and reads nothing.
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join(".factory/development-state.json");
    for (id, input) in [
        (
            "factory.attempt.retry",
            json!({"state": state, "run_ref": "run:x", "unit_ref": "unit:x"}),
        ),
        (
            "factory.attempt.choose",
            json!({"state": state, "run_ref": "run:x", "attempt_ref": "attempt:x"}),
        ),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
            .args(["action", "run", id, "-"])
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
        let refused = child.wait_with_output().unwrap();
        assert!(!refused.status.success(), "{id} ran with no grant");
        let words = String::from_utf8_lossy(&refused.stderr);
        assert!(
            words.contains("presents the person's grant"),
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
