//! Fixture praxis receipts for admission tests.
//!
//! Mirrors AIKit's `aikit.encounter-task-praxis/v1` wire shape (verbatim
//! snake_case members, kebab-case enum values) so admission tests carry the
//! receipt the new fail-closed admission law demands. These are test
//! doubles, not resolved AIKit readings.

use serde_json::{json, Value};
use std::collections::BTreeSet;

/// A `claim: resolved` receipt resolving every given ref with
/// `standing: available` for one workflow unit.
pub fn resolved_receipt(workflow_unit_ref: &str, praxis_refs: &BTreeSet<String>) -> Value {
    json!({
        "schema": "aikit.encounter-task-praxis/v1",
        "resolution_hash": "resolution:fixture",
        "claim": "resolved",
        "units": [{
            "workflow_unit_ref": workflow_unit_ref,
            "resolved": praxis_refs.iter().map(|reference| json!({
                "reference": reference,
                "form": "skill",
                "revision": "r1",
                "standing": "available"
            })).collect::<Vec<_>>(),
            "refusals": []
        }],
        "standing": "resolution standing only; fixture receipt"
    })
}

/// A `claim: refused` receipt refusing one named ref, for fail-closed tests.
/// Only the suites that exercise the refusal include a caller; the fixture is
/// included wholesale by every admission suite, so the unused inclusions are
/// expected.
#[allow(dead_code)]
pub fn refused_receipt(workflow_unit_ref: &str, reference: &str) -> Value {
    json!({
        "schema": "aikit.encounter-task-praxis/v1",
        "resolution_hash": "resolution:fixture",
        "claim": "refused",
        "units": [{
            "workflow_unit_ref": workflow_unit_ref,
            "resolved": [],
            "refusals": [{
                "reference": reference,
                "code": "ref-absent",
                "condition": "praxisRef is absent from the resolved catalogue",
                "recovery": "author or register the Skill the praxisRef names"
            }]
        }],
        "standing": "resolution standing only; fixture receipt"
    })
}
