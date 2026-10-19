//! Native task-bound AIKit dispatch. A retained configuration is only a preview:
//! expected_task is enforced by the actual AIKit owner under its operation lock.
use crate::attempt_runtime::{
    FactoryAttemptReading, FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

#[path = "attempt_task_material.rs"]
mod material;

/// The additive owner contract that first admits exact task/Agency assertions.
/// This is a contract basis, not proof that an arbitrary binary has these bytes.
pub const AIKIT_TASK_CONTRACT_REVISION: &str = "8804866fb49ec5072aaedcfcf032c7a078fa585f";

/// Factory's observation that the dispatched attempt was (or could not be)
/// recorded on the session's bound AIKit task.
pub(crate) const TASK_BINDING_CONTRACT: &str = "factory.aikit-task-attempt-binding/v1";
const TASK_BINDING_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) fn prepare(
    reading: &FactoryAttemptReading,
    attempt: &FactoryAttemptRecord,
    binary: &Path,
    cwd: &Path,
    transport: Option<&crate::native_aikit_route::AikitOwnerTransport>,
    request: &Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(30_000));
    let expected = request.pointer("/turn/expected_task").filter(|v| v.is_object())
        .ok_or("Protected task dispatch requires the native expected_task assertion, not a cached protection label")?;
    let output = crate::native_aikit_route::output(
        binary,
        cwd,
        transport,
        &[
            "session-space".into(),
            "encounter-task-read".into(),
            "--agent-session".into(),
            attempt.disposition.body.agent_session_ref.clone(),
        ],
        Duration::from_millis(timeout_ms.min(30_000)),
    )
    .map_err(|e| format!("Native task reading failed before dispatch: {e}"))?;
    if !output.status.success() {
        return Err(format!("Native AIKit task reading refused; no unprotected fallback; exit {:?}; stderr {}; response {}",
            output.status.code(), bounded_text(&output.stderr), bounded_text(&output.stdout)));
    }
    let task: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid native task reading: {e}"))?;
    if task["schema"] != "aikit.encounter-task/v1" || task["ready"] != true {
        return Err(format!(
            "The selected AIKit task is absent or its preparation is incomplete; native reading {}",
            bounded_text(&output.stdout)
        ));
    }
    let allocation = &task["allocation"]["allocation"];
    let participant = &attempt.disposition.participant;
    let placement = attempt
        .disposition
        .placement
        .as_ref()
        .ok_or("A protected worker requires the explicit situated placement bounds")?;
    let intent = json!({
        "revision":task["revision"], "task_ref":attempt.task_ref,
        "now_ref":placement.now_ref, "now_revision":allocation["revision"]["revision"],
        "policy_revision":placement.policy_revision, "cwd":cwd,
        "agent_ref":participant.agent_ref, "agency_ref":participant.agency_ref,
        "world_binding_ref":participant.world_binding_ref, "source_ref":participant.source_ref,
        "source_revision":participant.source_revision, "source_digest":participant.source_digest
    });
    if expected != &intent
        || task["request"]["central"]["task_ref"] != attempt.task_ref
        || task["request"]["cwd"] != json!(cwd)
        || task["agency_revision"] != request["turn"]["expected_binding_revision"]
        || allocation["now_ref"] != placement.now_ref
        || allocation["writable_destination"] != placement.now_path
        || allocation["policy"]["revision"] != placement.policy_revision
    {
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
        || task["inspection"]["requirements_digest"]
            .as_str()
            .is_none_or(str::is_empty)
    {
        return Err(
            "Task material requirements are not the exact selected Factory placement/authority"
                .into(),
        );
    }
    let writable = strings(&requirements["writable_paths"])?;
    let now = Path::new(&placement.now_path);
    if !writable.iter().any(|p| Path::new(p) == now)
        || writable.iter().any(|p| {
            let path = Path::new(p);
            !path.is_absolute()
                || (path != now
                    && !placement
                        .writable_paths
                        .iter()
                        .any(|allowed| path.starts_with(allowed)))
        })
    {
        return Err("Actual task writable paths widen the Factory attempt or omit its NOW".into());
    }
    let protected = strings(&requirements["protected_paths"])?;
    if placement
        .protected_paths
        .iter()
        .any(|p| !protected.iter().any(|q| Path::new(p).starts_with(q)))
    {
        return Err("Task boundary omits required Factory source protection".into());
    }
    let coverage = strings(&requirements["required_coverage"])?;
    if placement
        .required_coverage
        .iter()
        .any(|c| !coverage.contains(&c.as_str()))
    {
        return Err("Task boundary does not request all required Factory coverage".into());
    }
    if placement
        .write_boundary_ref
        .as_ref()
        .is_some_and(|r| task["inspection"]["requirements_digest"] != *r)
    {
        return Err(
            "Declared write-boundary identity differs from the native requirements digest".into(),
        );
    }
    let prepared_material = material::inspect(
        reading,
        &material::MaterialAttemptBasis::from(attempt),
        &task,
        transport,
        deadline,
    )?;
    let body = &attempt.disposition.body;
    if body.material_world_ref.is_some()
        || body.workcell_ref.is_some()
        || placement.material_receipt_ref.is_some()
    {
        let world = prepared_material
            .as_ref()
            .map(|reading| &reading["materialReading"]["receipt_world"])
            .unwrap_or(&task["material"]["world"]);
        if body.material_world_ref.as_deref() != world["world_ref"].as_str()
            || body.workcell_ref.as_deref() != world["workcell_ref"].as_str()
            || placement
                .material_receipt_ref
                .as_ref()
                .is_some_and(|r| world["world_ref"] != *r)
        {
            return Err(
                "Required Factory material is not the native task's actual Workcell world".into(),
            );
        }
    }
    let mut admission = json!({"contract":"factory.aikit-task-admission/v1", "taskRevision":task["revision"],
        "expectedTask":expected, "requirementsDigest":task["inspection"]["requirements_digest"],
        "materialWorld":task["material"]["world"],
        "standing":"preview-matched; native locked admission still required at send"});
    if !task["prepared_run"].is_null() {
        admission["preparedRunScope"] = task["prepared_run"].clone();
    }
    if let Some(material) = prepared_material {
        admission["nativePreparedMaterial"] = material;
    }
    Ok(admission)
}

/// Record the dispatched attempt on the session's bound AIKit task, so the
/// session can say which attempt it serves (`aikit session-space
/// encounter-task-attempts`). The dispatch is already made and retained; this
/// is Factory telling the task owner what it dispatched, never a second effect
/// on the worker. An owner that does not take the record (an older AIKit, no
/// bound task) answers a refusal, and that refusal is the observation:
/// `status: "refused"`/`"unavailable"` beside the owner's words. A replay of
/// the same record is the owner's no-op.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_to_task(
    reading: &FactoryAttemptReading,
    attempt: &FactoryAttemptRecord,
    state_path: &Path,
    execution_ref: &str,
    delivery_ref: &str,
    binary: &Path,
    cwd: &Path,
    transport: Option<&crate::native_aikit_route::AikitOwnerTransport>,
) -> OwnerOperationReceipt {
    let session = &attempt.disposition.body.agent_session_ref;
    let run_ref = reading.run_ref.to_string();
    let record = json!({"attempt": {
        "task_ref": attempt.task_ref, "attempt_ref": attempt.attempt_ref, "run_ref": run_ref,
        "state_path": state_path, "workflow_unit": attempt.workflow_unit_ref,
        "execution_ref": execution_ref, "delivery_ref": delivery_ref, "recorded_by": "factory",
    }});
    let asked = crate::native_aikit_route::output(
        binary,
        cwd,
        transport,
        &[
            "session-space".into(),
            "encounter-task-attempts".into(),
            "--agent-session".into(),
            session.clone(),
            "--request-json".into(),
            record.to_string(),
        ],
        TASK_BINDING_TIMEOUT,
    );
    let (status, owner) = match asked {
        Err(failure) => ("unavailable", json!({"refusal": failure.to_string()})),
        Ok(output) if !output.status.success() => (
            "refused",
            json!({"exitCode": output.status.code(), "refusal": bounded_text(&output.stderr)}),
        ),
        Ok(output) => match serde_json::from_slice::<Value>(&output.stdout) {
            Ok(answer)
                if answer["schema"] == "aikit.encounter-task-attempts/v1"
                    && answer["attempts"].as_array().is_some_and(|attempts| {
                        attempts
                            .iter()
                            .any(|served| served["attempt_ref"] == attempt.attempt_ref.as_str())
                    }) =>
            {
                ("recorded", json!({"taskRevision": answer["revision"]}))
            }
            _ => (
                "refused",
                json!({"refusal": format!("AIKit answered without the recorded attempt: {}", bounded_text(&output.stdout))}),
            ),
        },
    };
    let operation_ref = format!("factory-aikit-task-attempt:{}", attempt.attempt_ref);
    let payload = json!({"agentSession": session, "taskRef": attempt.task_ref,
        "attemptRef": attempt.attempt_ref, "runRef": run_ref, "statePath": state_path,
        "status": status, "owner": owner,
        "detail": {"meaning": "the attempt Factory dispatched, recorded on the session's bound AIKit task; not worker execution, a Return or a choice"}});
    let receipt_ref = format!(
        "factory-aikit-task-binding:{}",
        blake3::hash(
            &serde_json::to_vec(&(&operation_ref, OwnerOperationPhase::Observed, &payload))
                .expect("JSON values serialize")
        )
        .to_hex()
    );
    OwnerOperationReceipt {
        owner_ref: "factory".into(),
        contract: TASK_BINDING_CONTRACT.into(),
        operation_ref,
        receipt_ref,
        source_revision: format!("factory-state:{}", reading.revision),
        phase: OwnerOperationPhase::Observed,
        evidence_refs: BTreeSet::new(),
        partial_effect_refs: BTreeSet::new(),
        payload,
    }
}

fn strings(value: &Value) -> Result<Vec<&str>, String> {
    value
        .as_array()
        .ok_or("Native task requirement is not an array")?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Native requirement contains an empty/non-string identity".into())
        })
        .collect()
}

fn bounded_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned()
}

pub(super) fn validate_response(
    attempt: &FactoryAttemptRecord,
    delivery: &str,
    receipt: &crate::attempt_runtime::OwnerOperationReceipt,
) -> Result<(), String> {
    let expected = attempt.observations.iter().find_map(|observation| {
        (observation.payload["action"] == "send" && observation.payload["deliveryRef"] == delivery)
            .then(|| observation.payload.pointer("/taskAdmission/expectedTask"))
            .flatten()
    });
    if let Some(expected) = expected {
        if receipt.phase == crate::attempt_runtime::OwnerOperationPhase::ReconciledNoReplay
            && receipt.payload["schema"] == "factory.aikit-no-delivery-reconciliation/v1"
            && receipt.payload.get("expectedTask") == Some(expected)
            && receipt.payload["deliveryRef"] == delivery
        {
            // Exact no-effect observation, not a synthetic delivery record.
            return Ok(());
        }
        if crate::native_owner::locate_delivery(&receipt.payload)
            .and_then(|v| v.pointer("/request/submission/turn/expected_task"))
            != Some(expected)
        {
            return Err(
                "Native delivery changed or omitted the retained task/Agency/source assertions"
                    .into(),
            );
        }
    }
    Ok(())
}
