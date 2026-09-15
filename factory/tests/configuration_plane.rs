//! Conformance of the Factory owner lane (#299 C3D) against the frozen C0
//! contract of the Configuration Plane Wayfinder.
//!
//! Every verb runs against a sandbox developmental state in a temp directory —
//! never against live state — through the real `factory` binary, and every
//! emitted document is validated against the vendored frozen JSON Schemas
//! (`tests/support/configuration-schemas/`, byte-identical copies of the C0
//! schemas; see PROVENANCE.md there).

use jsonschema::Validator;
use serde_json::{json, Value};
use sha2::Digest;
use std::fs;
use std::process::{Command, Output};

const SETTING_REF: &str = "software-factory:binding:central-project";

fn factory_bin() -> &'static str {
    env!("CARGO_BIN_EXE_factory")
}

/// Run the real binary; return (exit code succeeded?, stdout).
fn run_factory(args: &[&str], stdin: Option<&str>) -> (bool, String) {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(factory_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn factory binary");
    if let Some(stdin_value) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin piped")
            .write_all(stdin_value.as_bytes())
            .expect("write stdin");
    }
    let output: Output = child.wait_with_output().expect("wait for factory");
    (
        output.status.success(),
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
    )
}

fn schema(name: &str) -> Validator {
    let path = format!(
        "{}/tests/support/configuration-schemas/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let document: Value =
        serde_json::from_str(&fs::read_to_string(path).expect("vendored schema")).expect("schema");
    Validator::new(&document).expect("schema compiles")
}

fn parse_stdout(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("bare JSON document on stdout")
}

/// One sandbox: a Factory developmental state document, the Central project
/// source document the binding verifies against, and the project scope
/// address (the state document path — Factory's native project address).
struct Sandbox {
    directory: tempfile::TempDir,
    state_path: std::path::PathBuf,
    scope: String,
    source_path: std::path::PathBuf,
    value: Value,
}

impl Sandbox {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let state_path = directory.path().join("developmental.json");
        let manifest =
            epilogos_factory::conformance::create_developmental_conformance_state(&state_path)
                .expect("sandbox developmental state");
        let _ = manifest;
        let source_path = directory.path().join("central-project.json");
        fs::write(
            &source_path,
            r#"{"schema":"central.project/v1","project_id":"project:o-i","human_source":{"title":"O:I"},"wiki":{"schema":"central.wiki/v1"}}"#,
        )
        .expect("central project source");
        let value = json!([{
            "central_project_ref": "project:o-i",
            "source_path": source_path.to_string_lossy(),
        }]);
        let scope = format!("project:{}", state_path.display());
        Self {
            directory,
            state_path,
            scope,
            source_path,
            value,
        }
    }

    fn value_string(&self) -> String {
        self.value.to_string()
    }
}

#[test]
fn contribution_is_a_bare_document_valid_against_the_frozen_schema() {
    let (ok, stdout) = run_factory(&["config-contribution", "--json"], None);
    assert!(ok, "config-contribution must succeed: {stdout}");
    let document = parse_stdout(&stdout);

    let contribution_schema = schema("oi.configuration-contribution-v1.schema.json");
    let errors: Vec<_> = contribution_schema
        .iter_errors(&document)
        .map(|error| format!("{}: {}", error.instance_path, error))
        .collect();
    assert!(
        errors.is_empty(),
        "contribution violates frozen schema: {errors:?}"
    );

    // Owner identity is the frozen product_id, never the CLI name.
    assert_eq!(document["owner"]["owner_ref"], "software-factory");
    assert_eq!(document["owner"]["owner_kind"], "product");

    // Structural mapping onto the v2 disclosure plane (C0 §17): owner_ref ==
    // the product_id the `system --json` disclosure carries.
    let (ok, system_stdout) = run_factory(&["system", "--json"], None);
    assert!(ok);
    let disclosure = parse_stdout(&system_stdout);
    assert_eq!(disclosure["product_id"], document["owner"]["owner_ref"]);
    assert_eq!(disclosure["schema"], "oi.product-settings-disclosure/v2");

    // The one honest setting: a genuine Factory-owned binding, with the four
    // verbs disclosed and the native ref inside Factory's own namespace.
    let setting = &document["sections"][0]["settings"][0];
    assert_eq!(setting["setting_ref"], SETTING_REF);
    assert_eq!(setting["section_ref"], document["sections"][0]["id"]);
    assert_eq!(setting["writable"], true);
    assert_eq!(
        setting["operations"],
        json!({"validate": true, "plan": true, "apply": true, "reset": true})
    );
    assert!(setting["native_ref"]
        .as_str()
        .unwrap()
        .starts_with("factory:"));
}

#[test]
fn validate_answers_the_requested_value_without_mutating_anything() {
    let sandbox = Sandbox::new();
    let state_before = fs::read_to_string(&sandbox.state_path).unwrap();

    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok, "validate failed: {stdout}");
    let validation = parse_stdout(&stdout);
    schema("oi.config-validation-v1.schema.json")
        .validate(&validation)
        .expect("validation document satisfies the frozen schema");
    assert_eq!(validation["schema"], "oi.config-validation/v1");
    assert_eq!(validation["valid"], true);
    assert!(
        validation["violations"]
            .as_array()
            .is_none_or(|violations| violations.is_empty()),
        "a valid answer carries no violations: {validation}"
    );
    assert_eq!(
        validation["scope"]["scopeKind"]
            .as_str()
            .or(validation["scope"]["scope_kind"].as_str()),
        Some("project")
    );

    // A wrong binding (source identifies another project) answers invalid, and
    // still mutates nothing.
    fs::write(
        &sandbox.source_path,
        r#"{"schema":"central.project/v1","project_id":"project:other"}"#,
    )
    .unwrap();
    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok, "an invalid answer is still an answer: {stdout}");
    let validation = parse_stdout(&stdout);
    assert_eq!(validation["valid"], false);
    assert_eq!(
        validation["violations"][0]["code"],
        "central-project-source"
    );
    assert_eq!(
        fs::read_to_string(&sandbox.state_path).unwrap(),
        state_before,
        "validate must not mutate the addressed state"
    );
}

#[test]
fn plan_apply_receipt_and_idempotent_replay_round_trip_in_a_sandbox() {
    let sandbox = Sandbox::new();

    // plan — owner-minted plan_id and digest, validated against the frozen schema.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok, "plan failed: {stdout}");
    let plan = parse_stdout(&stdout);
    schema("oi.config-plan-v1.schema.json")
        .validate(&plan)
        .expect("plan satisfies the frozen schema");
    let plan_id = plan["plan_id"].as_str().unwrap().to_owned();
    let plan_digest = plan["plan_digest"].as_str().unwrap().to_owned();
    assert!(plan_id.starts_with("factory-plan-"));
    assert_eq!(plan_digest.len(), 64);

    // plan_digest verifies over the canonical body: plan_id zeroed, plan_digest
    // zeroed, expires zeroed, every *_unix_ms zeroed (this owner zeroes the
    // members; it does not remove them).
    let mut canonical = plan.clone();
    canonical["plan_id"] = json!("");
    canonical["plan_digest"] = json!("");
    canonical["expires_at_unix_ms"] = json!(0);
    zero_unix_ms(&mut canonical);
    let mut hasher = sha2::Sha256::new();
    hasher.update(serde_json::to_string(&canonical).unwrap().as_bytes());
    let recomputed: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        plan_digest, recomputed,
        "plan_digest must verify over the canonical body"
    );

    // apply under a caller-minted changeset — the binding lands in the state.
    let plan_file = sandbox.directory.path().join("plan.json");
    fs::write(&plan_file, serde_json::to_string_pretty(&plan).unwrap()).unwrap();
    let (ok, stdout) = run_factory(
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_file.to_str().unwrap(),
            "--changeset",
            "cs-sandbox-1",
        ],
        None,
    );
    assert!(ok, "apply failed: {stdout}");
    let receipt = parse_stdout(&stdout);
    schema("oi.config-receipt-v1.schema.json")
        .validate(&receipt)
        .expect("receipt satisfies the frozen schema");
    assert_eq!(receipt["outcome"], "applied");
    assert_eq!(receipt["changeset_id"], "cs-sandbox-1");
    assert_eq!(receipt["plan_digest"], json!(plan_digest));
    let original_receipt_id = receipt["receipt_id"].as_str().unwrap().to_owned();
    assert!(receipt["native_ref"]
        .as_str()
        .unwrap()
        .starts_with("factory:developmental-state:"));

    // The native state really carries the binding now.
    let stored: Value =
        serde_json::from_str(&fs::read_to_string(&sandbox.state_path).unwrap()).unwrap();
    let links = stored["state"]["centralProjectLinks"].as_object().unwrap();
    assert_eq!(links.len(), 1, "the binding is in the developmental state");

    // Replay of the exact idempotency key (owner, changeset, setting, scope,
    // plan_digest) returns no_op naming the original receipt — no re-execution.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_file.to_str().unwrap(),
            "--changeset",
            "cs-sandbox-1",
        ],
        None,
    );
    assert!(ok, "replay failed: {stdout}");
    let replay = parse_stdout(&stdout);
    schema("oi.config-receipt-v1.schema.json")
        .validate(&replay)
        .expect("replay receipt satisfies the schema");
    assert_eq!(replay["outcome"], "no_op");
    assert_eq!(replay["original_receipt_id"], json!(original_receipt_id));
    assert_ne!(replay["receipt_id"], json!(original_receipt_id));

    // The same plan under a NEW changeset is a new key: it executes again, the
    // native admission finds the identical binding already present, and the
    // owner truthfully answers no_op.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_file.to_str().unwrap(),
            "--changeset",
            "cs-sandbox-2",
        ],
        None,
    );
    assert!(ok, "apply under a new changeset failed: {stdout}");
    let second = parse_stdout(&stdout);
    assert_eq!(second["outcome"], "no_op");
    assert_eq!(second["original_receipt_id"], json!(original_receipt_id));
}

#[test]
fn reset_unbinds_replays_no_op_and_reports_an_empty_reset_as_no_op() {
    let sandbox = Sandbox::new();
    let plan_file = sandbox.directory.path().join("plan.json");
    let (ok, stdout) = run_factory(
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok, "plan failed: {stdout}");
    fs::write(&plan_file, stdout).unwrap();
    let (ok, stdout) = run_factory(
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_file.to_str().unwrap(),
            "--changeset",
            "cs-sandbox-bind",
        ],
        None,
    );
    assert!(ok, "apply failed: {stdout}");
    assert_eq!(parse_stdout(&stdout)["outcome"], "applied");

    // Reset under its own changeset removes the binding for real.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "reset",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--changeset",
            "cs-sandbox-unbind",
        ],
        None,
    );
    assert!(ok, "reset failed: {stdout}");
    let receipt = parse_stdout(&stdout);
    schema("oi.config-receipt-v1.schema.json")
        .validate(&receipt)
        .expect("reset receipt satisfies the frozen schema");
    assert_eq!(receipt["operation"], "reset");
    assert_eq!(receipt["outcome"], "applied");
    let original = receipt["receipt_id"].as_str().unwrap().to_owned();
    let stored: Value =
        serde_json::from_str(&fs::read_to_string(&sandbox.state_path).unwrap()).unwrap();
    assert!(
        stored["state"]["centralProjectLinks"]
            .as_object()
            .is_none_or(|links| links.is_empty()),
        "reset removed the binding"
    );

    // Exact-key replay answers no_op naming the original receipt.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "reset",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--changeset",
            "cs-sandbox-unbind",
        ],
        None,
    );
    assert!(ok);
    let replay = parse_stdout(&stdout);
    assert_eq!(replay["outcome"], "no_op");
    assert_eq!(replay["original_receipt_id"], json!(original));

    // A reset of a project with nothing bound changes nothing: no_op.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "reset",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--changeset",
            "cs-sandbox-empty",
        ],
        None,
    );
    assert!(ok);
    assert_eq!(parse_stdout(&stdout)["outcome"], "no_op");
}

#[test]
fn failures_leave_a_structured_error_document_on_stdout_and_exit_nonzero() {
    let sandbox = Sandbox::new();
    let schema_error = schema("oi.config-error-v1.schema.json");
    let assert_error = |ok: bool, stdout: &str, expected_code: &str| -> Value {
        assert!(!ok, "expected failure, got success: {stdout}");
        let error = parse_stdout(stdout);
        schema_error
            .validate(&error)
            .unwrap_or_else(|_| panic!("error document violates the frozen schema: {error}"));
        assert_eq!(error["error_code"], expected_code, "{error}");
        error
    };

    // A foreign product's setting is never absorbed (#299 §18 C3D).
    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            "ai-kit:resolution:model.default",
            "--scope",
            &sandbox.scope,
            "--value",
            "true",
        ],
        None,
    );
    assert_error(ok, &stdout, "unsupported_setting");

    // A scope kind outside the frozen registry.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            "galaxy:andromeda",
            "--value",
            "[]",
        ],
        None,
    );
    assert_error(ok, &stdout, "unknown_scope_kind");

    // A scope kind the setting does not allow.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            "world",
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert_error(ok, &stdout, "unsupported_scope");

    // A project scope with no state document behind it is not addressable.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "validate",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            "project:/nonexistent/state.json",
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert_error(ok, &stdout, "unsupported_scope");

    // A malformed plan document is refused by schema.
    let (ok, stdout) = run_factory(
        &["config", "apply", "--json", "--plan-file", "-"],
        Some("{\"schema\": \"oi.config-plan/v1\", \"nonsense\": true}"),
    );
    assert_error(ok, &stdout, "unsupported_schema");

    // A plan whose digest does not cover its body is refused.
    let (ok, stdout) = run_factory(
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok);
    let mut tampered = parse_stdout(&stdout);
    tampered["changes"][0]["summary"] = json!("tampered after minting");
    let (ok, stdout) = run_factory(
        &["config", "apply", "--json", "--plan-file", "-"],
        Some(&serde_json::to_string(&tampered).unwrap()),
    );
    assert_error(ok, &stdout, "validation_failed");

    // Missing required arguments are structured failures too.
    let (ok, stdout) = run_factory(&["config", "apply", "--json"], None);
    assert_error(ok, &stdout, "validation_failed");
}

#[test]
fn apply_refuses_when_the_planned_change_no_longer_validates() {
    let sandbox = Sandbox::new();
    let (ok, stdout) = run_factory(
        &[
            "config",
            "plan",
            "--json",
            "--setting",
            SETTING_REF,
            "--scope",
            &sandbox.scope,
            "--value",
            &sandbox.value_string(),
        ],
        None,
    );
    assert!(ok, "plan failed: {stdout}");
    let plan_file = sandbox.directory.path().join("plan.json");
    fs::write(&plan_file, &stdout).unwrap();

    // The Central source drifts to another project after minting.
    fs::write(
        &sandbox.source_path,
        r#"{"schema":"central.project/v1","project_id":"project:moved"}"#,
    )
    .unwrap();

    let (ok, stdout) = run_factory(
        &[
            "config",
            "apply",
            "--json",
            "--plan-file",
            plan_file.to_str().unwrap(),
            "--changeset",
            "cs-sandbox-late",
        ],
        None,
    );
    assert!(
        !ok,
        "apply must refuse when the plan no longer validates: {stdout}"
    );
    let error = parse_stdout(&stdout);
    schema("oi.config-error-v1.schema.json")
        .validate(&error)
        .expect("error schema");
    assert_eq!(error["error_code"], "validation_failed");
}

/// Zero every `*_unix_ms` field, recursively (07 §4.5 convention).
fn zero_unix_ms(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map
                .keys()
                .filter(|key| key.ends_with("_unix_ms"))
                .cloned()
                .collect();
            for key in keys {
                map.insert(key, json!(0i64));
            }
            for (_, child) in map.iter_mut() {
                zero_unix_ms(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                zero_unix_ms(item);
            }
        }
        _ => {}
    }
}
