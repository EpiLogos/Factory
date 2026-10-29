//! OpenRig's planning dial and wave SDLC (O-I #741), through Factory's own
//! command surface over a real developmental state document and a real git
//! integration repository: `factory development wave act <state> -` and
//! `factory development wave read <state> --run R`.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
const PLANNER: &str = "central:position:project:O-I:planner";
const CRITIC: &str = "central:position:project:O-I:critic";
const BUILDER_A: &str = "central:position:project:O-I:builder-a";
const BUILDER_B: &str = "central:position:project:O-I:builder-b";
const REVIEWER_1: &str = "central:position:project:O-I:reviewer-1";
const REVIEWER_2: &str = "central:position:project:O-I:reviewer-2";

fn occupancy_stub() -> PathBuf {
    static STUB: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    STUB.get_or_init(|| {
        let path = std::env::temp_dir().join("factory-test-wave-occupancy-ok");
        // A child shell writes the script, so this test never holds a write
        // handle a parallel fork could inherit (ETXTBSY, Factory#292).
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("t=\"$1.$$\"; cat > \"$t\" && chmod 755 \"$t\" && mv \"$t\" \"$1\"")
            .arg("sh")
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"#!/bin/sh\nprintf '%s\\n' '{\"ok\":true}'\n")
            .unwrap();
        assert!(child.wait().unwrap().success());
        path
    })
    .clone()
}

/// `factory` as the body occupying `position`, or as no body at all.
fn factory(args: &[&str], position: Option<&str>, body: Option<&Value>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(args)
        .env_remove("OI_POSITION_REF")
        .env_remove("OI_OCCUPANT_GENERATION")
        .env("ACTUATION_BIN", occupancy_stub())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(position) = position {
        command
            .env("OI_POSITION_REF", position)
            .env("OI_OCCUPANT_GENERATION", "generation:test");
    }
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(body) = body {
        stdin.write_all(body.to_string().as_bytes()).unwrap();
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}

fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A lawful refusal: exit 2, a three-part document on stdout naming `code`.
fn refused(output: Output, code: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let refusal: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refusal["schema"], "factory.refusal/v1");
    assert_eq!(refusal["code"], code, "{refusal:#}");
    for part in ["fact", "consequence", "action"] {
        assert!(
            !refusal[part].as_str().unwrap_or_default().is_empty(),
            "{part} is empty in {refusal:#}"
        );
    }
    refusal
}

struct Bench {
    _dir: tempfile::TempDir,
    state: String,
    repo: PathBuf,
    /// key -> workflow unit ref, from Factory's own unit listing.
    units: std::collections::BTreeMap<String, String>,
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A commit on `branch` (cut from `from`) that writes each file.
fn commit(repo: &Path, from: &str, branch: &str, files: &[&str]) -> String {
    git(repo, &["checkout", "-q", "-B", branch, from]);
    for file in files {
        let path = repo.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(handle, "{branch}").unwrap();
    }
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", branch]);
    let head = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["checkout", "-q", "main"]);
    head
}

/// The integrator's merge: `main` takes `revision`.
fn merge(repo: &Path, revision: &str) {
    git(repo, &["checkout", "-q", "main"]);
    git(repo, &["merge", "-q", "--no-ff", "--no-edit", revision]);
}

impl Bench {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.json").display().to_string();
        ok(factory(
            &["conformance", "developmental-state", &state, "--json"],
            None,
            None,
        ));
        let listing = ok(factory(
            &["development", "workflow-units", &state, RUN, "--json"],
            None,
            None,
        ));
        let units = listing["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| {
                (
                    unit["key"].as_str().unwrap().to_owned(),
                    unit["workflowUnitRef"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        for file in ["app/a.rs", "kernel/b.rs", "docs/c.md"] {
            let path = repo.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "base\n").unwrap();
        }
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "base"]);
        Self {
            _dir: dir,
            state,
            repo,
            units,
        }
    }

    fn unit(&self, key: &str) -> &str {
        &self.units[key]
    }

    fn act(&self, by: Option<&str>, request: Value) -> Output {
        let mut body = request;
        body["schema"] = json!("factory.wave-request/v1");
        factory(
            &["development", "wave", "act", &self.state, "-", "--json"],
            by,
            Some(&body),
        )
    }

    fn read(&self) -> Value {
        ok(factory(
            &[
                "development",
                "wave",
                "read",
                &self.state,
                "--run",
                RUN,
                "--json",
            ],
            None,
            None,
        ))
    }

    fn dial(&self, by: &str, key: &str, rung: &str, territory: &[&str]) -> Output {
        self.act(
            Some(by),
            json!({"act": "dial", "run_ref": RUN, "workflow_unit_ref": self.unit(key),
                   "rung": rung, "reason": format!("{key} is planned at {rung}"),
                   "territory": territory}),
        )
    }

    fn round(&self, by: &str, key: &str, round: &str, extra: Value) -> Output {
        let mut request = json!({"act": "round", "run_ref": RUN, "workflow_unit_ref": self.unit(key),
                                 "round": round, "evidence_ref": format!("evidence:{key}:{round}")});
        if let Value::Object(fields) = extra {
            for (name, value) in fields {
                request[name] = value;
            }
        }
        self.act(Some(by), request)
    }

    fn lock(&self, by: &str, key: &str) -> Output {
        self.act(
            Some(by),
            json!({"act": "lock", "run_ref": RUN, "workflow_unit_ref": self.unit(key)}),
        )
    }

    fn candidate(&self, by: &str, key: &str, revision: &str) -> Output {
        self.act(
            Some(by),
            json!({"act": "candidate", "run_ref": RUN, "workflow_unit_ref": self.unit(key),
                   "revision": revision}),
        )
    }

    fn review(&self, by: &str, wave: &str, leg: &str, findings: Value) -> Output {
        let verdict = if findings.as_array().is_some_and(Vec::is_empty) {
            "clear"
        } else {
            "findings"
        };
        self.act(
            Some(by),
            json!({"act": "review", "run_ref": RUN, "wave": wave, "leg": leg,
                   "verdict": verdict, "findings": findings}),
        )
    }

    fn wave<'a>(reading: &'a Value, id: &str) -> &'a Value {
        reading["waves"]
            .as_array()
            .unwrap()
            .iter()
            .find(|wave| wave["wave"] == id)
            .unwrap_or_else(|| panic!("no wave {id} in {reading:#}"))
    }

    fn unit_reading<'a>(&self, reading: &'a Value, key: &str) -> &'a Value {
        let reference = self.unit(key);
        reading["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|unit| unit["workflow_unit_ref"] == reference)
            .unwrap_or_else(|| panic!("no unit {key} in {reading:#}"))
    }
}

/// The person, from the desk: a desktop-human caller and the grant the host
/// resolved for it, carried in the request (as a prompt pad's write is).
fn as_person(mut request: Value) -> Value {
    request["caller"] = json!({"callerRef": "oi:desk-plan", "projectionKind": "desktop-human", "lineage": ["oi:desk-plan"]});
    request["authority"] = json!({"authorityRef": "central:native-action-authority#owner",
        "nativeOwner": "factory", "capabilityRef": "capability/factory/plan-waves",
        "capabilityGranted": true, "actionAuthorised": true});
    request
}

#[test]
fn the_planning_dial_holds_a_plan_open_until_its_rung_has_been_earned() {
    let bench = Bench::new();
    // Nothing set reads as nothing set: Factory supplies no default rung.
    let before = bench.read();
    assert_eq!(before["schema"], "factory.wave-reading/v1");
    assert!(before["units"].as_array().unwrap().is_empty());
    let untouched = std::fs::read(&bench.state).unwrap();
    assert!(!String::from_utf8_lossy(&untouched).contains("wavePlans"));

    // A rung needs its reason, and a rung Factory does not own is refused.
    let mut no_reason = json!({"act": "dial", "run_ref": RUN,
        "workflow_unit_ref": bench.unit("inspect-source"), "rung": "P2", "reason": " "});
    refused(
        bench.act(Some(PLANNER), no_reason.clone()),
        "factory.plan.reason_required",
    );
    no_reason["rung"] = json!("P9");
    no_reason["reason"] = json!("why");
    refused(bench.act(Some(PLANNER), no_reason), "factory.wave.request");
    // A unit that is not one of Factory's is refused by name.
    refused(
        bench.act(
            Some(PLANNER),
            json!({"act": "dial", "run_ref": RUN,
                "workflow_unit_ref": "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FZZ",
                "rung": "P1", "reason": "x", "territory": ["app/"]}),
        ),
        "factory.wave.unit_not_found",
    );
    // An act with no actor is no operator.
    refused(
        bench.act(
            None,
            json!({"act": "lock", "run_ref": RUN, "workflow_unit_ref": bench.unit("inspect-source")}),
        ),
        "factory.wave.actor_required",
    );
    assert_eq!(untouched, std::fs::read(&bench.state).unwrap());

    // P2: a research round must return before the plan locks.
    let receipt = ok(bench.dial(PLANNER, "inspect-source", "P2", &["app/"]));
    assert_eq!(receipt["schema"], "factory.wave-receipt/v1");
    assert_eq!(receipt["result"], "applied");
    let refusal = refused(
        bench.lock(PLANNER, "inspect-source"),
        "factory.plan.rounds_missing",
    );
    assert!(
        refusal["fact"].as_str().unwrap().contains("research"),
        "{refusal:#}"
    );
    // P2 calls for no adversarial pass.
    refused(
        bench.round(CRITIC, "inspect-source", "adversarial", json!({})),
        "factory.plan.round_not_called",
    );
    ok(bench.round(PLANNER, "inspect-source", "research", json!({})));
    let reading = bench.read();
    let unit = bench.unit_reading(&reading, "inspect-source");
    assert_eq!(unit["dial"]["rung"], "P2");
    assert_eq!(unit["dial"]["needs"], json!([]));
    assert_eq!(unit["ladder"]["locked"]["value"], "no");
    ok(bench.lock(PLANNER, "inspect-source"));
    // A locked plan is frozen: no new rung, no further round.
    refused(
        bench.dial(PLANNER, "inspect-source", "P1", &["app/"]),
        "factory.plan.locked",
    );
    refused(
        bench.round(PLANNER, "inspect-source", "research", json!({})),
        "factory.plan.locked",
    );

    // P3: the adversarial pass is a non-author's.
    ok(bench.dial(PLANNER, "implement-compiler", "P3", &["kernel/"]));
    ok(bench.round(PLANNER, "implement-compiler", "research", json!({})));
    refused(
        bench.round(PLANNER, "implement-compiler", "adversarial", json!({})),
        "factory.plan.self_review",
    );
    ok(bench.round(
        CRITIC,
        "implement-compiler",
        "adversarial",
        json!({"amendments": ["a failing test pins the empty-input refusal"]}),
    ));
    // The person locks from the desk, through the grant the host resolved.
    let lock = as_person(json!({"act": "lock", "run_ref": RUN,
        "workflow_unit_ref": bench.unit("implement-compiler"), "schema": "factory.wave-request/v1"}));
    ok(factory(
        &["development", "wave", "act", &bench.state, "-", "--json"],
        None,
        Some(&lock),
    ));
    // A caller that is not a desktop-human holding the grant is refused.
    let mut forged = lock.clone();
    forged["authority"]["capabilityGranted"] = json!(false);
    refused(
        factory(
            &["development", "wave", "act", &bench.state, "-", "--json"],
            None,
            Some(&forged),
        ),
        "factory.wave.not_authorised",
    );

    // P4: a blind design that differs stops for a design session.
    ok(bench.dial(PLANNER, "review-adversarially", "P4", &["docs/"]));
    ok(bench.round(PLANNER, "review-adversarially", "research", json!({})));
    ok(bench.round(CRITIC, "review-adversarially", "adversarial", json!({})));
    refused(
        bench.round(PLANNER, "review-adversarially", "design-session", json!({})),
        "factory.plan.round_not_called",
    );
    refused(
        bench.round(CRITIC, "review-adversarially", "blind-design", json!({})),
        "factory.plan.verdict_required",
    );
    ok(bench.round(
        CRITIC,
        "review-adversarially",
        "blind-design",
        json!({"verdict": "different"}),
    ));
    let refusal = refused(
        bench.lock(PLANNER, "review-adversarially"),
        "factory.plan.rounds_missing",
    );
    assert!(
        refusal["fact"].as_str().unwrap().contains("design-session"),
        "{refusal:#}"
    );
    let reading = bench.read();
    assert_eq!(
        bench.unit_reading(&reading, "review-adversarially")["dial"]["needs"],
        json!(["design-session"])
    );
    ok(bench.round(PLANNER, "review-adversarially", "design-session", json!({})));
    ok(bench.lock(PLANNER, "review-adversarially"));

    // P1 asks a declared territory before it locks; P0 does not.
    ok(bench.dial(PLANNER, "integrate-return", "P1", &[]));
    refused(
        bench.lock(PLANNER, "integrate-return"),
        "factory.plan.territory_missing",
    );

    let reading = bench.read();
    let unit = bench.unit_reading(&reading, "implement-compiler");
    assert_eq!(unit["dial"]["rung"], "P3");
    assert_eq!(unit["dial"]["lock"]["by"]["kind"], "person");
    assert_eq!(unit["ladder"]["locked"]["value"], "yes");
    assert_eq!(
        unit["dial"]["rounds"][1]["amendments"],
        json!(["a failing test pins the empty-input refusal"])
    );
}

#[test]
fn a_wave_builds_in_disjoint_territories_and_seals_only_on_two_clear_legs_at_one_revision() {
    let bench = Bench::new();
    let repo = bench.repo.display().to_string();
    for (key, rung, territory) in [
        ("inspect-source", "P1", "app/"),
        ("implement-compiler", "P1", "kernel/"),
        ("review-adversarially", "P0", "docs/"),
        ("integrate-return", "P1", "app/a.rs"),
    ] {
        ok(bench.dial(PLANNER, key, rung, &[territory]));
    }
    ok(bench.lock(PLANNER, "inspect-source"));
    ok(bench.lock(PLANNER, "implement-compiler"));

    // Overlapping territories in one wave are refused unless serialized.
    let overlap = json!({"act": "compose", "run_ref": RUN, "wave": "w0",
        "units": [bench.unit("inspect-source"), bench.unit("integrate-return")],
        "integration": {"repo": repo, "onto": "main"}});
    refused(
        bench.act(Some(PLANNER), overlap),
        "factory.wave.territory_overlap",
    );
    // The first composition names where the work integrates.
    refused(
        bench.act(
            Some(PLANNER),
            json!({"act": "compose", "run_ref": RUN, "wave": "w1",
                "units": [bench.unit("inspect-source")]}),
        ),
        "factory.wave.integration_required",
    );
    ok(bench.act(
        Some(PLANNER),
        json!({"act": "compose", "run_ref": RUN, "wave": "w1",
            "units": [bench.unit("inspect-source"), bench.unit("implement-compiler")],
            "review_model": "independent", "integration": {"repo": repo, "onto": "main"}}),
    ));
    ok(bench.act(
        Some(PLANNER),
        json!({"act": "compose", "run_ref": RUN, "wave": "w2",
            "units": [bench.unit("review-adversarially")]}),
    ));
    // A unit stands in one wave.
    refused(
        bench.act(
            Some(PLANNER),
            json!({"act": "compose", "run_ref": RUN, "wave": "w3",
                "units": [bench.unit("inspect-source")]}),
        ),
        "factory.wave.unit_in_two_waves",
    );

    let reading = bench.read();
    let w1 = Bench::wave(&reading, "w1");
    assert_eq!(w1["state"], "building");
    assert_eq!(Bench::wave(&reading, "w2")["state"], "waiting");
    assert!(
        Bench::wave(&reading, "w2")["gate"]
            .as_str()
            .unwrap()
            .contains("w1"),
        "{reading:#}"
    );

    // A candidate stays inside its unit's territory.
    let stray = commit(&bench.repo, "main", "stray", &["app/a.rs", "kernel/b.rs"]);
    refused(
        bench.candidate(BUILDER_A, "inspect-source", &stray),
        "factory.wave.outside_territory",
    );
    let a1 = commit(&bench.repo, "main", "a1", &["app/a.rs"]);
    let b1 = commit(&bench.repo, "main", "b1", &["kernel/b.rs"]);
    ok(bench.candidate(BUILDER_A, "inspect-source", &a1));
    ok(bench.candidate(BUILDER_B, "implement-compiler", &b1));
    // The next wave does not open until this one seals.
    let c1 = commit(&bench.repo, "main", "c1", &["docs/c.md"]);
    refused(
        bench.candidate(BUILDER_A, "review-adversarially", &c1),
        "factory.wave.earlier_wave_open",
    );
    // Review fires when the wave's build completes: every candidate merged.
    refused(
        bench.review(REVIEWER_1, "w1", "structure", json!([])),
        "factory.wave.build_incomplete",
    );
    merge(&bench.repo, &a1);
    let reading = bench.read();
    let w1 = Bench::wave(&reading, "w1");
    assert_eq!(w1["state"], "building");
    // b1 was cut before a1 merged: the merge is a rebase trigger.
    assert!(w1["next"].as_str().unwrap().contains("rebase"), "{w1:#}");
    // A person reads the unit by its key in the workflow, not its ref.
    assert!(
        w1["next"].as_str().unwrap().contains("implement-compiler"),
        "{w1:#}"
    );
    assert_eq!(
        bench.unit_reading(&reading, "implement-compiler")["key"],
        "implement-compiler"
    );
    merge(&bench.repo, &b1);
    let tip = git(&bench.repo, &["rev-parse", "main"]);
    let reading = bench.read();
    assert_eq!(Bench::wave(&reading, "w1")["state"], "in-review");
    assert_eq!(
        bench.unit_reading(&reading, "inspect-source")["ladder"]["merged"]["value"],
        "yes"
    );

    // Nothing self-reviews; one reviewer does not hold both legs.
    refused(
        bench.review(BUILDER_A, "w1", "structure", json!([])),
        "factory.wave.writer_reviews",
    );
    ok(bench.review(REVIEWER_1, "w1", "structure", json!([])));
    refused(
        bench.review(REVIEWER_1, "w1", "claims", json!([])),
        "factory.wave.one_reviewer_two_legs",
    );
    // A finding names its cause.
    refused(
        bench.review(
            REVIEWER_2,
            "w1",
            "claims",
            json!([{"cause": "mystery", "text": "x"}]),
        ),
        "factory.wave.request",
    );
    ok(bench.review(
        REVIEWER_2,
        "w1",
        "claims",
        json!([{"cause": "judgment-gap", "text": "the compiler swallows an error"},
               {"cause": "context-gap", "text": "the spec never named the lockfile", "drift": true}]),
    ));
    let reading = bench.read();
    let w1 = Bench::wave(&reading, "w1");
    assert_eq!(w1["state"], "fix-round");
    assert_eq!(w1["findings"]["judgment_gap"], 1);
    assert_eq!(w1["findings"]["context_gap"], 1);
    assert_eq!(w1["findings"]["drift"], 1);
    assert_eq!(w1["legs"][0]["revision"], tip);

    // A forward fix merges; both legs re-earn at the round's final revision.
    let fix = commit(&bench.repo, "main", "fix", &["kernel/b.rs"]);
    ok(bench.candidate(BUILDER_B, "implement-compiler", &fix));
    merge(&bench.repo, &fix);
    ok(bench.review(REVIEWER_2, "w1", "claims", json!([])));
    let reading = bench.read();
    // The structure leg cleared an earlier revision: not sealed yet.
    assert_eq!(Bench::wave(&reading, "w1")["state"], "in-review");
    ok(bench.review(REVIEWER_1, "w1", "structure", json!([])));
    let reading = bench.read();
    let w1 = Bench::wave(&reading, "w1");
    assert_eq!(w1["state"], "sealed", "{w1:#}");
    assert_eq!(w1["rounds"], 2);
    assert_eq!(
        bench.unit_reading(&reading, "implement-compiler")["ladder"]["reviewed"]["value"],
        "yes"
    );
    // A sealed wave takes no further candidate.
    let late = commit(&bench.repo, "main", "late", &["app/a.rs"]);
    refused(
        bench.candidate(BUILDER_A, "inspect-source", &late),
        "factory.wave.sealed",
    );

    // The next wave opens; its unit still needs a locked plan.
    let c1 = commit(&bench.repo, "main", "c1b", &["docs/c.md"]);
    refused(
        bench.candidate(BUILDER_A, "review-adversarially", &c1),
        "factory.wave.plan_not_locked",
    );
    ok(bench.lock(PLANNER, "review-adversarially"));
    ok(bench.candidate(BUILDER_A, "review-adversarially", &c1));
    assert_eq!(Bench::wave(&bench.read(), "w2")["state"], "building");

    // The document still loads and round-trips with the plan present.
    let bytes = std::fs::read(&bench.state).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("wavePlans"));
    let provider =
        epilogos_factory::developmental_read::FactoryDevelopmentalFileProvider::open(&bench.state)
            .unwrap();
    let copy = bench.repo.join("../copy.json");
    epilogos_factory::developmental_read::FactoryDevelopmentalFileProvider::create(
        &copy,
        provider.state().clone(),
    )
    .unwrap();
    assert_eq!(bytes, std::fs::read(&copy).unwrap());
}

#[test]
fn an_unreadable_integration_repository_reads_as_unknown_never_as_merged() {
    let bench = Bench::new();
    ok(bench.dial(PLANNER, "inspect-source", "P0", &["app/"]));
    ok(bench.lock(PLANNER, "inspect-source"));
    ok(bench.act(
        Some(PLANNER),
        json!({"act": "compose", "run_ref": RUN, "wave": "w1",
            "units": [bench.unit("inspect-source")],
            "integration": {"repo": bench.repo.display().to_string(), "onto": "main"}}),
    ));
    let a1 = commit(&bench.repo, "main", "a1", &["app/a.rs"]);
    ok(bench.candidate(BUILDER_A, "inspect-source", &a1));
    merge(&bench.repo, &a1);
    std::fs::rename(&bench.repo, bench.repo.with_extension("moved")).unwrap();
    let reading = bench.read();
    let unit = bench.unit_reading(&reading, "inspect-source");
    assert_eq!(unit["ladder"]["merged"]["value"], "unknown", "{unit:#}");
    assert!(!unit["ladder"]["merged"]["basis"]
        .as_str()
        .unwrap()
        .is_empty());
    let w1 = Bench::wave(&reading, "w1");
    assert_eq!(w1["state"], "unread", "{w1:#}");
    assert!(reading["integration"]["tip"].is_null());
}
