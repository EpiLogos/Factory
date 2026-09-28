//! The task-oriented frontdoor help (command-encounter classification §4):
//! bare/help print the outcome-grouped reference with no side effects, and
//! the everyday developmental nouns lead. Every exact route stays listed —
//! the simplification concerns exposure and actionability, not removal.

use std::process::Command;

fn run_factory(args: &[&str]) -> (String, i32) {
    let bin = env!("CARGO_BIN_EXE_factory");
    let output = Command::new(bin)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("factory {args:?} should run: {e}"));
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        output.status.code().unwrap_or(1),
    )
}

#[test]
fn bare_and_help_print_the_task_oriented_reference_without_side_effects() {
    for args in [vec!["--help"], vec!["help"], vec!["-h"]] {
        let (stdout, code) = run_factory(&args);
        assert_eq!(code, 0, "help never fails: {args:?}");
        assert!(stdout.contains("Software Factory"), "{args:?}: {stdout}");
        assert!(
            stdout.contains("developmental work through Commissions"),
            "the opening line names the product's act: {args:?}: {stdout}"
        );
        // The everyday developmental nouns lead the reference, before the
        // operator material.
        let everyday = stdout.find("Developmental work (everyday entry");
        let operator = stdout.find("Binding and specimen (Operator)");
        assert!(everyday.is_some(), "{args:?}: {stdout}");
        assert!(operator.is_some(), "{args:?}: {stdout}");
        assert!(
            everyday.unwrap() < operator.unwrap(),
            "everyday work leads operator depth: {args:?}"
        );
        // Exact routes are never lost from the reference.
        for route in [
            "factory development commission",
            "factory development current-work",
            "factory development observations",
            "factory action invoke",
            "factory telemetry stats",
            "factory project setup",
            "factory config apply",
        ] {
            assert!(stdout.contains(route), "{args:?}: missing `{route}`");
        }
    }
}
