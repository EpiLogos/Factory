//! The Run development ledger has a product surface.
//!
//! `DevelopmentObservation` / `OwnerReturnProposal` are the carriers a Run uses
//! to return deferred work to its native owners instead of silently applying it.
//! Until this surface existed the carriers were reachable only from library
//! tests, so nothing outside the crate could register a deferred piece of work
//! or read one back — a capability with no way to demonstrate it. These tests
//! drive the real `factory` binary and then read the same ledger through the
//! library store, so what the command wrote is the owner's own carrier and not
//! a parallel record.

use epilogos_factory::core::run::RunRef;
use epilogos_factory::project_development::{
    DevelopmentObservationKind, PROJECT_DEVELOPMENT_VERSION,
};
use epilogos_factory::project_development_store::{
    FileProjectDevelopmentStore, ProjectDevelopmentStore,
};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB";
const OTHER_RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FCC";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn ledger_root(label: &str) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "factory-development-ledger-{}-{serial}-{label}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    path
}

fn factory(args: &[String], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn observe(root: &Path, run: &str, body: serde_json::Value) -> Output {
    factory(
        &[
            "development".into(),
            "observe".into(),
            root.to_string_lossy().into_owned(),
            run.into(),
            "-".into(),
            "--json".into(),
        ],
        Some(body.to_string().as_bytes()),
    )
}

fn observations(root: &Path, run: &str) -> Output {
    factory(
        &[
            "development".into(),
            "observations".into(),
            root.to_string_lossy().into_owned(),
            run.into(),
            "--json".into(),
        ],
        None,
    )
}

fn deferred(reference: &str, statement: &str) -> serde_json::Value {
    json!({
        "observation_ref": reference,
        "kind": "insufficient-evidence",
        "statement": statement,
        "subject_refs": ["source:crates/aikit-cli/src/star_commands.rs"],
        "evidence_refs": ["session:closeout-proof"],
        "owner_return": {
            "owner_ref": "aikit",
            "source_ref": null,
            "proposal_ref": format!("proposal:{reference}"),
            "recognition_required": true
        }
    })
}

#[test]
fn several_deferred_observations_stand_open_at_once_and_read_back_through_factory() {
    let root = ledger_root("additive");

    let first = observe(&root, RUN, deferred("observation:fork-one", "First fork"));
    assert!(
        first.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(receipt["contract"], PROJECT_DEVELOPMENT_VERSION);
    assert_eq!(receipt["observationCount"], 1);
    assert_eq!(receipt["ownerReturnRequired"], true);

    let second = observe(&root, RUN, deferred("observation:fork-two", "Second fork"));
    assert!(second.status.success());
    let receipt: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    // Additive: the second fork does not replace the first. Fork-ness is
    // registration behaviour, so both stay open in the same Run ledger.
    assert_eq!(receipt["observationCount"], 2);

    let reading = observations(&root, RUN);
    assert!(reading.status.success());
    let reading: serde_json::Value = serde_json::from_slice(&reading.stdout).unwrap();
    assert_eq!(reading["observationCount"], 2);
    let refs: Vec<&str> = reading["observations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|observation| observation["observation_ref"].as_str().unwrap())
        .collect();
    assert_eq!(refs, vec!["observation:fork-one", "observation:fork-two"]);

    // The command wrote the owner's own carrier: the library store loads the
    // same ledger and sees the same observations, kind and proposal intact.
    let store = FileProjectDevelopmentStore::new(&root);
    let ledger = store
        .load(&RunRef::from_str(RUN).unwrap())
        .unwrap()
        .expect("the ledger the CLI wrote is loadable by the owner's store");
    assert_eq!(ledger.observations.len(), 2);
    assert_eq!(
        ledger.observations[0].kind,
        DevelopmentObservationKind::InsufficientEvidence
    );
    assert!(
        ledger.observations[0]
            .owner_return
            .as_ref()
            .unwrap()
            .recognition_required,
        "a deferred proposal returns through Recognition, never self-applied"
    );
}

#[test]
fn a_duplicate_observation_ref_is_refused_rather_than_overwriting_the_open_one() {
    let root = ledger_root("duplicate");
    assert!(observe(&root, RUN, deferred("observation:same", "First"))
        .status
        .success());

    let repeat = observe(&root, RUN, deferred("observation:same", "Different text"));
    assert!(!repeat.status.success(), "a duplicate ref must be refused");
    let stderr = String::from_utf8_lossy(&repeat.stderr);
    assert!(
        stderr.contains("observation:same"),
        "the refusal names the duplicate ref: {stderr}"
    );

    let reading = observations(&root, RUN);
    let reading: serde_json::Value = serde_json::from_slice(&reading.stdout).unwrap();
    assert_eq!(reading["observationCount"], 1);
    assert_eq!(reading["observations"][0]["statement"], "First");
}

#[test]
fn a_request_cannot_smuggle_an_observation_into_another_runs_ledger() {
    let root = ledger_root("run-mismatch");
    let mut body = deferred("observation:smuggled", "Belongs elsewhere");
    body["run_ref"] = json!(OTHER_RUN);

    let output = observe(&root, RUN, body);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match the addressed Run"),
        "the refusal explains the mismatch: {stderr}"
    );

    // Neither Run's ledger gained anything.
    for run in [RUN, OTHER_RUN] {
        let reading = observations(&root, run);
        let reading: serde_json::Value = serde_json::from_slice(&reading.stdout).unwrap();
        assert_eq!(reading["observationCount"], 0);
    }
}

#[test]
fn a_run_that_deferred_nothing_reads_as_zero_observations_not_as_an_error() {
    let root = ledger_root("absent");
    let output = observations(&root, RUN);
    assert!(
        output.status.success(),
        "an empty ledger is a real answer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reading: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["observationCount"], 0);
    assert_eq!(reading["runRef"], RUN);
    assert!(reading["observations"].as_array().unwrap().is_empty());
}

#[test]
fn the_ledger_operations_are_declared_capabilities_of_the_native_cli() {
    let output = factory(&["capabilities".into(), "--json".into()], None);
    assert!(output.status.success());
    let capabilities: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let commands: Vec<&str> = capabilities["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command.as_str().unwrap())
        .collect();
    assert!(commands.contains(&"development.observe"), "{commands:?}");
    assert!(
        commands.contains(&"development.observations"),
        "{commands:?}"
    );
}
