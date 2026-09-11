//! Native task-bound AIKit dispatch. A retained configuration is only a preview:
//! expected_task is enforced by the actual AIKit owner under its operation lock.
use crate::attempt_runtime::FactoryAttemptRecord;
use crate::native_process;
use serde_json::{json, Value};
use std::{path::Path, process::Command, time::Duration};

/// The additive owner contract that first admits exact task/Agency assertions.
/// This is a contract basis, not proof that an arbitrary binary has these bytes.
pub const AIKIT_TASK_CONTRACT_REVISION: &str = "8804866fb49ec5072aaedcfcf032c7a078fa585f";

pub(super) fn prepare(
    attempt: &FactoryAttemptRecord,
    binary: &Path,
    cwd: &Path,
    request: &Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let expected = request.pointer("/turn/expected_task").filter(|v| v.is_object())
        .ok_or("Protected task dispatch requires the native expected_task assertion, not a cached protection label")?;
    let output = native_process::output(Command::new(binary).args([
        "-C", cwd.to_str().ok_or("Task cwd is not UTF-8")?,
        "encounter-task-read", "--agent-session", &attempt.disposition.body.agent_session_ref,
    ]), Duration::from_millis(timeout_ms.min(30_000)))
        .map_err(|e| format!("Native task reading failed before dispatch: {e}"))?;
    if !output.status.success() { return Err("Native AIKit task reading refused; no unprotected fallback".into()); }
    let task: Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid native task reading: {e}"))?;
    if task["schema"] != "aikit.encounter-task/v1" || task["ready"] != true {
        return Err("The selected AIKit task is absent or its preparation is incomplete".into());
    }
    let allocation = &task["allocation"]["allocation"];
    let participant = &attempt.disposition.participant;
    let placement = attempt.disposition.placement.as_ref()
        .ok_or("A protected worker requires the explicit situated placement bounds")?;
    let intent = json!({
        "revision":task["revision"], "task_ref":attempt.task_ref,
        "now_ref":placement.now_ref, "now_revision":allocation["revision"]["revision"],
        "policy_revision":placement.policy_revision, "cwd":cwd,
        "agent_ref":participant.agent_ref, "agency_ref":participant.agency_ref,
        "world_binding_ref":participant.world_binding_ref, "source_ref":participant.source_ref,
        "source_revision":participant.source_revision, "source_digest":participant.source_digest
    });
    if expected != &intent || task["request"]["central"]["task_ref"] != attempt.task_ref
        || task["request"]["cwd"] != json!(cwd)
        || task["agency_revision"] != request["turn"]["expected_binding_revision"]
        || allocation["now_ref"] != placement.now_ref
        || allocation["writable_destination"] != placement.now_path
        || allocation["policy"]["revision"] != placement.policy_revision {
        return Err("Prepared AIKit task differs from this exact Factory attempt/source/NOW/cwd basis; re-resolve explicitly".into());
    }
    let requirements = &task["requirements"];
    if requirements["schema"] != "workcell.write-boundary/v1"
        || requirements["policy_ref"] != placement.policy_ref
        || requirements["policy_revision"] != placement.policy_revision
        || requirements["authority_ref"] != placement.authority_ref
        || task["request"]["authority_ref"] != placement.authority_ref
        || task["inspection"]["schema"] != "workcell.prepared-write-boundary/v1"
        || task["inspection"]["requirements"] != *requirements
        || task["inspection"]["requirements_digest"].as_str().is_none_or(str::is_empty) {
        return Err("Task material requirements are not the exact selected Factory placement/authority".into());
    }
    let writable = strings(&requirements["writable_paths"])?;
    let now = Path::new(&placement.now_path);
    if !writable.iter().any(|p| Path::new(p) == now) || writable.iter().any(|p| {
        let path = Path::new(p);
        !path.is_absolute() || (path != now && !placement.writable_paths.iter().any(|allowed| path.starts_with(allowed)))
    }) {
        return Err("Actual task writable paths widen the Factory attempt or omit its NOW".into());
    }
    let protected = strings(&requirements["protected_paths"])?;
    if placement.protected_paths.iter().any(|p| !protected.iter().any(|q| Path::new(p).starts_with(q))) {
        return Err("Task boundary omits required Factory source protection".into());
    }
    let coverage = strings(&requirements["required_coverage"])?;
    if placement.required_coverage.iter().any(|c| !coverage.contains(&c.as_str())) {
        return Err("Task boundary does not request all required Factory coverage".into());
    }
    if placement.write_boundary_ref.as_ref().is_some_and(|r| task["inspection"]["requirements_digest"] != *r) {
        return Err("Declared write-boundary identity differs from the native requirements digest".into());
    }
    let body = &attempt.disposition.body;
    if body.material_world_ref.is_some() || body.workcell_ref.is_some() || placement.material_receipt_ref.is_some() {
        let world = &task["material"]["world"];
        if body.material_world_ref.as_deref() != world["world_ref"].as_str()
            || body.workcell_ref.as_deref() != world["workcell_ref"].as_str()
            || placement.material_receipt_ref.as_ref().is_some_and(|r| world["world_ref"] != *r) {
            return Err("Required Factory material is not the native task's actual Workcell world".into());
        }
    }
    Ok(json!({"contract":"factory.aikit-task-admission/v1", "taskRevision":task["revision"],
        "expectedTask":expected, "requirementsDigest":task["inspection"]["requirements_digest"],
        "materialWorld":task["material"]["world"],
        "standing":"preview-matched; native locked admission still required at send"}))
}

fn strings(value: &Value) -> Result<Vec<&str>, String> {
    value.as_array().ok_or("Native task requirement is not an array")?.iter()
        .map(|v| v.as_str().filter(|s| !s.is_empty()).ok_or_else(|| "Native requirement contains an empty/non-string identity".into()))
        .collect()
}

pub(super) fn validate_response(
    attempt: &FactoryAttemptRecord,
    delivery: &str,
    receipt: &crate::attempt_runtime::OwnerOperationReceipt,
) -> Result<(), String> {
    let expected = attempt.observations.iter().find_map(|observation| {
        (observation.payload["action"] == "send" && observation.payload["deliveryRef"] == delivery)
            .then(|| observation.payload.pointer("/taskAdmission/expectedTask")).flatten()
    });
    if let Some(expected) = expected {
        if crate::native_owner::locate_delivery(&receipt.payload)
            .and_then(|v| v.pointer("/request/submission/turn/expected_task")) != Some(expected) {
            return Err("Native delivery changed or omitted the retained task/Agency/source assertions".into());
        }
    }
    Ok(())
}
