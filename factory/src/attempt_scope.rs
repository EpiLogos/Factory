//! What an attempt's scope reaches and may write, read from the attempt's own record (DLW 19.12 Scope; O:I names it WO-SF2).
//!
//! Factory already holds the write side of a scope for every effectful attempt: the placement/protection basis it admitted
//! (`PlacementProtection`: the NOW path, the writable and protected paths, the required and realised coverage, the write
//! boundary), the effects it permits, and the source basis it works from — and dispatch refuses a task whose material write
//! boundary widens those paths (`attempt_task_dispatch.rs`). This reading answers those facts in the Scope's terms for one
//! attempt or every attempt of a Run. It records nothing.
//!
//! What Factory does not hold is named, never guessed: a Reach ring (Object, Scene, Expression, Central), the places an
//! agent may read, and a write mode (propose, draft, direct). No Factory record carries them, and nothing enforces them.

use crate::attempt_runtime::{FactoryAttemptError, FactoryAttemptReading, FactoryAttemptRecord};
use serde_json::{json, Value};

pub const FACTORY_ATTEMPT_SCOPE_READING: &str = "factory.attempt-scope-reading/v1";

/// The parts of a Scope no Factory record carries, each with why.
pub fn not_held() -> Value {
    json!({
        "reach": "No Factory record carries a Reach (Object, Scene, Expression, Central); an attempt's reach is its writable and protected paths.",
        "reads": "No Factory record lists the places an agent may read; the placement names only where it may write and what is protected.",
        "writeMode": "No Factory record carries a write mode (propose, draft, direct), and nothing enforces one; an attempt's writes are bounded by its paths at dispatch.",
    })
}

fn scope_of(record: &FactoryAttemptRecord) -> Value {
    let disposition = &record.disposition;
    let participant = &disposition.participant;
    let placement = disposition.placement.as_ref().map(|placement| {
        json!({
            "nowRef": placement.now_ref,
            "nowPath": placement.now_path,
            "writablePaths": placement.writable_paths,
            "protectedPaths": placement.protected_paths,
            "requiredCoverage": placement.required_coverage,
            "effectiveCoverage": placement.effective_coverage,
            "policyRef": placement.policy_ref,
            "policyRevision": placement.policy_revision,
            "authorityRef": placement.authority_ref,
            "writeBoundaryRef": placement.write_boundary_ref,
            "materialReceiptRef": placement.material_receipt_ref,
        })
    });
    json!({
        "attemptRef": record.attempt_ref,
        "taskRef": record.task_ref,
        "workflowUnitRef": record.workflow_unit_ref,
        "agentRef": participant.agent_ref,
        "positionRef": participant.position_ref,
        "basis": {"sourceRef": participant.source_ref, "sourceRevision": participant.source_revision},
        "writes": placement,
        "writesWhy": if disposition.placement.is_none() {
            json!("This attempt was admitted with no placement: it is neither effectful nor materialised, so it writes nowhere.")
        } else {
            json!("Dispatch refuses a task whose write boundary widens these paths or omits the NOW path.")
        },
        "permittedEffects": disposition.permitted_effects,
    })
}

/// The scope of one attempt (`attempt`), or of every attempt of the reading's Run.
pub fn scope_reading(
    reading: &FactoryAttemptReading,
    attempt: Option<&str>,
) -> Result<Value, FactoryAttemptError> {
    let scopes = match attempt {
        Some(attempt_ref) => {
            let record = reading
                .attempts
                .iter()
                .find(|record| record.attempt_ref == attempt_ref)
                .ok_or_else(|| FactoryAttemptError::UnknownAttempt(attempt_ref.to_owned()))?;
            vec![scope_of(record)]
        }
        None => reading.attempts.iter().map(scope_of).collect(),
    };
    Ok(json!({
        "contract": FACTORY_ATTEMPT_SCOPE_READING,
        "runRef": reading.run_ref,
        "revision": reading.revision,
        "scopes": scopes,
        "notHeld": not_held(),
        "persisted": false,
    }))
}
