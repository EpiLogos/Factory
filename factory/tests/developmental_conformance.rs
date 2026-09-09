//! Native-binary proof that Factory can generate and serve its complete public
//! developmental conformance surface without consumer reconstruction.

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn factory(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .output()
        .unwrap()
}

fn successful_json(args: &[&str]) -> Value {
    let output = factory(args);
    assert!(
        output.status.success(),
        "factory failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn generated_state_is_deterministic_and_drives_every_native_read() {
    let home = TempDir::new().unwrap();
    let first = home.path().join("first.json");
    let second = home.path().join("second.json");
    let manifest = successful_json(&["conformance", "developmental-state", path(&first), "--json"]);
    let second_manifest = successful_json(&[
        "conformance",
        "developmental-state",
        path(&second),
        "--json",
    ]);
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
    assert_eq!(manifest["contract"], second_manifest["contract"]);
    assert_eq!(
        manifest["contract"],
        "factory.developmental-conformance-manifest/v1"
    );

    for (operation, reference_field, expected_contract) in [
        ("project", "projectRef", "factory.project-reading/v1"),
        ("journey", "journeyRef", "factory.journey-reading/v1"),
        ("run", "runRef", "factory.run-reading/v1"),
        (
            "workflow-unit",
            "workflowUnitRef",
            "factory.workflow-unit-reading/v1",
        ),
        (
            "execution-telemetry",
            "telemetryRef",
            "factory.execution-telemetry-reading/v1",
        ),
        (
            "routine-continuation",
            "invocationRef",
            "factory.routine-continuation-reading/v1",
        ),
    ] {
        let reference = manifest[reference_field].as_str().unwrap();
        let reading =
            successful_json(&["development", operation, path(&first), reference, "--json"]);
        assert_eq!(reading["contract"], expected_contract);
    }

    let telemetry = successful_json(&[
        "development",
        "execution-telemetry",
        path(&first),
        manifest["telemetryRef"].as_str().unwrap(),
        "--json",
    ]);
    assert_eq!(telemetry["modelUsage"]["availability"], "unavailable");
    assert_eq!(telemetry["materialUsage"]["availability"], "unavailable");
    assert!(telemetry["modelUsage"]["observations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(telemetry["materialUsage"]["observations"]
        .as_array()
        .unwrap()
        .is_empty());

    let before = std::fs::read(&first).unwrap();
    let refused = factory(&["conformance", "developmental-state", path(&first), "--json"]);
    assert!(!refused.status.success());
    assert_eq!(std::fs::read(&first).unwrap(), before);
}
