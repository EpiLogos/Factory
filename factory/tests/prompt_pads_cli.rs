//! The Prompt pads verbs through the real `factory` binary: the wiring the unit
//! tests of `prompt_pads` do not reach (help, capabilities, the process entry and
//! its refusal documents). Nothing here asks Central: every pad written has
//! Runs at Off, so no `ctrl` runs and no schedule is resolved.

use epilogos_factory::prompt_pads::{
    PADS_CAPABILITY_REF, PADS_DUE, PADS_READING, PADS_RUN_BEGIN, PADS_RUN_RECORD,
    PADS_WRITE_RECEIPT,
};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const PROJECT: &str = "project:O-I";

fn factory(args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("factory should start");
    let mut input = child.stdin.take().expect("stdin is piped");
    if let Some(text) = stdin {
        input.write_all(text.as_bytes()).expect("stdin is written");
    }
    drop(input);
    let output = child.wait_with_output().expect("factory should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(1),
    )
}

fn answer(args: &[&str], stdin: Option<&str>) -> Value {
    let (stdout, stderr, code) = factory(args, stdin);
    assert_eq!(code, 0, "factory {args:?} failed: {stderr}");
    serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("{args:?} answered {stdout}: {error}"))
}

fn write_request(index: usize, expected: &str, pad: Value) -> String {
    json!({
        "project_ref":PROJECT,"index":index,"expected_revision":expected,"pad":pad,
        "caller":{"callerRef":"oi:desktop","projectionKind":"desktop-human","lineage":["oi:desktop"]},
        "authority":{"authorityRef":"central:native-action-authority#owner","nativeOwner":"factory",
            "capabilityRef":PADS_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true}
    })
    .to_string()
}

#[test]
fn the_five_verbs_are_in_help_and_in_capabilities() {
    let (help, _, code) = factory(&["help"], None);
    assert_eq!(code, 0);
    for route in [
        "factory development pads <state> <project-ref>",
        "factory development pads-write",
        "factory development pads-run",
        "factory development pads-record",
        "factory development pads-due",
    ] {
        assert!(help.contains(route), "help lacks `{route}`");
    }
    let capabilities = answer(&["capabilities", "--json"], None);
    let commands = capabilities["commands"].as_array().unwrap();
    for command in [
        "development.pads",
        "development.pads-write",
        "development.pads-run",
        "development.pads-record",
        "development.pads-due",
    ] {
        assert!(commands.iter().any(|value| value == command), "{command}");
    }
    let contracts = capabilities["nativeContracts"].as_array().unwrap();
    for contract in [
        PADS_READING,
        PADS_WRITE_RECEIPT,
        PADS_RUN_BEGIN,
        PADS_RUN_RECORD,
        PADS_DUE,
    ] {
        assert!(
            contracts.iter().any(|value| value == contract),
            "{contract}"
        );
    }
}

#[test]
fn a_pad_goes_through_its_life_by_the_binary() {
    let state = tempfile::tempdir().unwrap();
    let state = state.path().to_str().unwrap();

    let empty = answer(&["development", "pads", state, PROJECT, "--json"], None);
    assert_eq!(empty["schema"], PADS_READING);
    assert_eq!(empty["revision"], "absent");

    let written = answer(
        &["development", "pads-write", state, "-", "--json"],
        Some(&write_request(
            4,
            "absent",
            json!({"prompt":"Review the open branch and say what is left.","choke":1}),
        )),
    );
    assert_eq!(written["schema"], PADS_WRITE_RECEIPT);
    assert_eq!(written["changed"], true);

    let read = answer(&["development", "pads", state, PROJECT, "--json"], None);
    assert_eq!(read["revision"], written["revision"]);
    assert_eq!(
        read["slots"][4]["pad"]["name"],
        "Review the open branch and say"
    );

    let begun = answer(
        &["development", "pads-run", state, "-", "--json"],
        Some(&json!({"project_ref":PROJECT,"index":4,"trigger":"press"}).to_string()),
    );
    assert_eq!(begun["created"], true);
    assert_eq!(
        begun["prompt"],
        "Review the open branch and say what is left."
    );

    let ended = answer(
        &["development", "pads-record", state, "-", "--json"],
        Some(
            &json!({"run_ref":begun["run_ref"],"state":"done","task_ref":"task:1","at_unix_ms":4_000_000_000_000_i64})
                .to_string(),
        ),
    );
    assert_eq!(ended["run"]["state"], "done");

    let read = answer(&["development", "pads", state, PROJECT, "--json"], None);
    assert_eq!(read["slots"][4]["last"]["task_ref"], "task:1");
    assert!(read["slots"][4]["running"].is_null());

    // Nothing is scheduled, so nothing is due, and the answer says why.
    let due = answer(
        &[
            "development",
            "pads-due",
            state,
            PROJECT,
            "--now",
            "4000000000000",
            "--since",
            "3999999000000",
            "--json",
        ],
        None,
    );
    assert_eq!(due["schema"], PADS_DUE);
    assert_eq!(due["due"], json!([]));
    assert!(due["reason"].is_string());
}

#[test]
fn a_refusal_is_a_document_on_stdout_under_json_and_a_line_on_stderr_without() {
    let state = tempfile::tempdir().unwrap();
    let state = state.path().to_str().unwrap();
    let request = json!({"project_ref":PROJECT,"index":3,"trigger":"press"}).to_string();

    let (stdout, _, code) = factory(
        &["development", "pads-run", state, "-", "--json"],
        Some(&request),
    );
    assert_eq!(code, 2);
    let document: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(document["schema"], "factory.prompt-pads-refusal/v1");
    assert_eq!(document["code"], "empty_pad");
    assert_eq!(document["effect"], "Nothing was recorded.");

    let (stdout, stderr, code) = factory(&["development", "pads-run", state, "-"], Some(&request));
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("Pad 3 is empty"), "{stderr}");
}

#[test]
fn a_write_that_is_not_a_persons_is_refused_by_the_binary_and_writes_nothing() {
    let state = tempfile::tempdir().unwrap();
    let state_path = state.path().to_str().unwrap();
    let mut request: Value =
        serde_json::from_str(&write_request(0, "absent", json!({"prompt":"p"}))).unwrap();
    request["caller"]["projectionKind"] = json!("situated-agent");
    let (stdout, _, code) = factory(
        &["development", "pads-write", state_path, "-", "--json"],
        Some(&request.to_string()),
    );
    assert_eq!(code, 2);
    let document: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(document["code"], "not_human");
    assert!(std::fs::read_dir(state.path()).unwrap().next().is_none());
}
