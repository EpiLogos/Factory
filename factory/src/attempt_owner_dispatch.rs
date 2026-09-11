//! Consequential native owner calls attached to the existing persisted attempt.
//!
//! Factory records its own intent before transport. That intent is not an AIKit
//! receipt or evidence that a worker ran. Replay never sends again. Recovery
//! observes the original delivery and settles every pending local call last.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::{
    FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptReading,
    FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION,
};
use crate::core::run::RunRef;
use crate::native_owner::{
    invoke_native_owner_bounded, NativeOwnerInvocation, AIKIT_CAW_CONTRACT_REVISION,
    DEFAULT_OWNER_TIMEOUT_MS,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

pub const FACTORY_ATTEMPT_OWNER_ACTION: &str = "factory.attempt-owner-action/v1";
pub const FACTORY_ATTEMPT_OWNER_RECEIPT: &str = "factory.attempt-owner-receipt/v1";
#[path = "attempt_task_dispatch.rs"]
mod task_dispatch;
pub use task_dispatch::AIKIT_TASK_CONTRACT_REVISION;

const TRANSPORT_OBSERVATION: &str = "factory.attempt-owner-transport/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptOwnerRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    /// Factory Execution identity, distinct from the owner delivery and session.
    pub execution_ref: String,
    pub invocation: NativeOwnerInvocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryAttemptOwnerReceipt {
    pub contract: String,
    pub request_ref: String,
    pub run_ref: RunRef,
    pub attempt_ref: String,
    pub replayed: bool,
    pub needs_reconciliation: bool,
    pub transport_observation: OwnerOperationReceipt,
    pub owner_receipt: Option<OwnerOperationReceipt>,
    /// A response that could not be retained is still returned to the caller.
    pub retention_error: Option<String>,
    /// Missing current readback must not be represented by a stale snapshot.
    pub reading: Option<FactoryAttemptReading>,
}

#[derive(Debug)]
pub struct AttemptOwnerError(pub String);
impl Display for AttemptOwnerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl Error for AttemptOwnerError {}
fn error(value: impl Display) -> AttemptOwnerError {
    AttemptOwnerError(value.to_string())
}

fn record<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, AttemptOwnerError> {
    reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == reference)
        .ok_or_else(|| error("attempt is not retained in this canonical Run"))
}
fn intent_ref(request: &FactoryAttemptOwnerRequest) -> String {
    format!("factory-attempt-owner-call:{}", request.request_ref)
}
fn is_transport(receipt: &OwnerOperationReceipt) -> bool {
    receipt.owner_ref == "factory" && receipt.contract == TRANSPORT_OBSERVATION
}
fn pending(phase: OwnerOperationPhase) -> bool {
    matches!(
        phase,
        OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
    )
}
fn terminal(phase: OwnerOperationPhase) -> bool {
    matches!(
        phase,
        OwnerOperationPhase::Returned
            | OwnerOperationPhase::Failed
            | OwnerOperationPhase::Cancelled
            | OwnerOperationPhase::ReconciledNoReplay
    )
}
fn latest_transports(attempt: &FactoryAttemptRecord) -> BTreeMap<String, OwnerOperationReceipt> {
    let mut latest = BTreeMap::new();
    for receipt in &attempt.observations {
        if is_transport(receipt) {
            latest.insert(receipt.operation_ref.clone(), receipt.clone());
        }
    }
    latest
}
fn same_delivery(receipt: &OwnerOperationReceipt, session: &str, delivery: &str) -> bool {
    is_transport(receipt)
        && receipt.payload["agentSession"].as_str() == Some(session)
        && receipt.payload["deliveryRef"].as_str() == Some(delivery)
}
fn action_request(
    request: &FactoryAttemptOwnerRequest,
    revision: u64,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: format!(
            "{}:{}:{suffix}",
            request.projection_ref, request.request_ref
        ),
        caller: request.caller.clone(),
        run_ref: request.run_ref.clone(),
        expected_revision: revision,
        authority: request.authority.clone(),
        operation,
    }
}

/// Retry only a local transaction, never the external invocation. The exact
/// same owner observation is idempotent, not a new receipt or a new effect.
fn retain(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> Result<(), AttemptOwnerError> {
    for _ in 0..8 {
        let reading = store.reading().map_err(error)?;
        let attempt = record(&reading, &request.attempt_ref)?;
        if let FactoryAttemptOperation::RecordObservation { receipt, .. } = &operation {
            if let Some(existing) = attempt
                .dispatch
                .iter()
                .chain(&attempt.observations)
                .find(|existing| existing.receipt_ref == receipt.receipt_ref)
            {
                return if existing == receipt {
                    Ok(())
                } else {
                    Err(error("owner receipt identity has conflicting content"))
                };
            }
            if !is_transport(receipt) {
                if let Some(existing) = attempt
                    .observations
                    .iter()
                    .rev()
                    .chain(attempt.dispatch.iter())
                    .find(|existing| {
                        existing.owner_ref == receipt.owner_ref
                            && existing.operation_ref == receipt.operation_ref
                    })
                {
                    // Out-of-order read results remain in their transport
                    // envelope, but do not overwrite a known terminal result.
                    if terminal(existing.phase) && !terminal(receipt.phase) {
                        return Ok(());
                    }
                    if terminal(existing.phase)
                        && terminal(receipt.phase)
                        && existing.phase != receipt.phase
                    {
                        return Err(error(
                            "conflicting terminal owner results require explicit re-resolution",
                        ));
                    }
                }
            }
        }
        match store.apply(action_request(
            request,
            reading.revision,
            suffix,
            operation.clone(),
        )) {
            Ok(_) => return Ok(()),
            Err(failure) => {
                if store.reading().map_err(error)?.revision == reading.revision {
                    return Err(error(failure));
                }
            }
        }
    }
    Err(error(
        "native receipt retention remained contended; inspect the delivery, do not resend",
    ))
}

fn stamp(
    mut receipt: OwnerOperationReceipt,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    receipt.phase = phase;
    receipt.payload["detail"] = detail;
    let bytes = serde_json::to_vec(&(&receipt.operation_ref, phase, &receipt.payload))
        .expect("JSON values serialize");
    receipt.receipt_ref = format!("factory-owner-transport:{}", blake3::hash(&bytes).to_hex());
    receipt
}

fn result(
    store: &FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    replayed: bool,
    transport: OwnerOperationReceipt,
    owner_receipt: Option<OwnerOperationReceipt>,
    mut retention_error: Option<String>,
) -> FactoryAttemptOwnerReceipt {
    let reading = match store.reading() {
        Ok(reading) => Some(reading),
        Err(failure) => {
            retention_error = Some(failure.to_string());
            None
        }
    };
    let unresolved = reading
        .as_ref()
        .and_then(|reading| record(reading, &request.attempt_ref).ok())
        .is_none_or(|attempt| {
            latest_transports(attempt)
                .values()
                .any(|receipt| pending(receipt.phase))
        });
    FactoryAttemptOwnerReceipt {
        contract: FACTORY_ATTEMPT_OWNER_RECEIPT.into(),
        request_ref: request.request_ref.clone(),
        run_ref: request.run_ref.clone(),
        attempt_ref: request.attempt_ref.clone(),
        replayed,
        needs_reconciliation: unresolved || pending(transport.phase) || retention_error.is_some(),
        transport_observation: transport,
        owner_receipt,
        retention_error,
        reading,
    }
}

/// Dispatch or observe an already provisioned native session. The existing
/// owner-action CLI delegates here; this owns no extra coordinator or store.
pub fn execute_attempt_owner_action(
    state_path: &Path,
    request: FactoryAttemptOwnerRequest,
) -> Result<FactoryAttemptOwnerReceipt, AttemptOwnerError> {
    if request.contract != FACTORY_ATTEMPT_OWNER_ACTION
        || request.request_ref.trim().is_empty()
        || request.projection_ref.trim().is_empty()
        || request.execution_ref.trim().is_empty()
    {
        return Err(error("invalid attempt owner Action identity or contract"));
    }
    let mut store =
        FileAttemptStore::open_run(state_path, request.run_ref.clone()).map_err(error)?;
    let reading = store.reading().map_err(error)?;
    let attempt = record(&reading, &request.attempt_ref)?;
    // Retain the original v1 request digest for existing persisted Action replay.
    let digest = blake3::hash(&serde_json::to_vec(&request).map_err(error)?)
        .to_hex()
        .to_string();
    if let Some(previous) = latest_transports(attempt).get(&intent_ref(&request)) {
        if previous.payload["requestDigest"].as_str() != Some(&digest) {
            return Err(error("owner Action request identity has conflicting input"));
        }
        let current = store.reading().map_err(error)?;
        if request.expected_revision > current.revision {
            return Err(error("replayed Action claims a future Factory revision"));
        }
        store
            .apply(action_request(
                &request,
                current.revision,
                "replay-authority",
                FactoryAttemptOperation::Read,
            ))
            .map_err(error)?;
        let owner = previous
            .payload
            .pointer("/detail/ownerReceipt")
            .cloned()
            .map(serde_json::from_value::<OwnerOperationReceipt>)
            .transpose()
            .map_err(error)?;
        return Ok(result(
            &store,
            &request,
            true,
            previous.clone(),
            owner,
            None,
        ));
    }
    if reading.revision != request.expected_revision {
        return Err(error("stale Factory state revision before owner dispatch"));
    }
    let NativeOwnerInvocation::AikitEncounter {
        binary,
        cwd,
        contract_revision,
        request: packet,
    } = &request.invocation
    else {
        return Err(error(
            "this attempt handoff supports AIKit send/delivery only",
        ));
    };
    if !matches!(contract_revision.as_str(), AIKIT_CAW_CONTRACT_REVISION | AIKIT_TASK_CONTRACT_REVISION) || !cwd.is_absolute() {
        return Err(error(
            "AIKit requires the exact published owner revision and an absolute cwd",
        ));
    }
    let action = packet["action"].as_str().unwrap_or_default();
    let session = packet["agent_session"].as_str().unwrap_or_default();
    let delivery =
        match action {
            "send" => packet.pointer("/turn/delivery_ref"),
            "delivery" => packet.get("delivery_ref"),
            _ => return Err(error(
                "attempt owner Action supports send/delivery, not a guessed cancellation operation",
            )),
        }
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| error("native delivery identity is required"))?;
    if session != attempt.disposition.body.agent_session_ref {
        return Err(error("owner target differs from the arranged AgentSession"));
    }
    if let Some(environment) = &attempt.disposition.working_environment {
        if environment.cwd != *cwd {
            return Err(error(
                "dispatch cwd differs from the declared Candidate/worktree basis",
            ));
        }
    }
    let native_action = format!("action/aikit/encounter-{action}");
    if !attempt
        .disposition
        .authority
        .allowed_action_refs
        .contains(&native_action)
    {
        return Err(error("current disposition does not permit this owner Action"));
    }
    let previous_calls = latest_transports(attempt);
    let mut effective_packet = packet.clone();
    let mut task_admission = None;
    if action == "send" {
        if attempt.status != crate::attempt_runtime::AttemptStatus::Ready {
            return Err(error("attempt is not ready for a new native dispatch"));
        }
        if previous_calls
            .values()
            .any(|receipt| pending(receipt.phase))
        {
            return Err(error(
                "an earlier owner effect is uncertain; reconcile its original delivery first",
            ));
        }
        let protected_or_writing = !attempt.context.allowed_writes.is_empty()
            || attempt.disposition.placement.as_ref().is_some_and(|placement| {
                !placement.required_coverage.is_empty()
                    || placement.write_boundary_ref.is_some()
                    || placement.material_receipt_ref.is_some()
            });
        if protected_or_writing || packet.pointer("/turn/expected_task").is_some() {
            if contract_revision != AIKIT_TASK_CONTRACT_REVISION {
                return Err(error("Protected/writing attempts require AIKit's actual task-dispatch contract; plain encounter delivery is not enforcement"));
            }
            task_admission = Some(task_dispatch::prepare(attempt, binary, cwd, packet,
                attempt.disposition.budget.wall_clock_timeout_ms.unwrap_or(DEFAULT_OWNER_TIMEOUT_MS).min(DEFAULT_OWNER_TIMEOUT_MS)).map_err(error)?);
        }
        if attempt.disposition.working_environment.is_none()
            && attempt
                .disposition
                .placement
                .as_ref()
                .is_some_and(|placement| {
                    !placement
                        .writable_paths
                        .iter()
                        .any(|path| cwd.starts_with(path))
                })
        {
            return Err(error(
                "execution cwd is outside the explicitly resolved working world",
            ));
        }
        let mut text = packet
            .pointer("/turn/packet/text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| error("native send requires explicit bounded work text"))?
            .to_owned();
        if let Some(now) = &attempt.context.now_ref {
            text.push_str(&format!("\nTask NOW: {now}"));
        }
        if let Some(now) = &attempt.context.now_path {
            text.push_str(&format!("\nTask output directory: {}", now.display()));
        }
        text.push_str(&format!(
            "\nTask: {}\nFactory attempt: {}\nExecution: {}\nReturn destination: {}",
            attempt.task_ref,
            attempt.attempt_ref,
            request.execution_ref,
            attempt.disposition.return_destination_ref
        ));
        effective_packet["turn"]["packet"]["text"] = json!(text);
    } else {
        if !previous_calls.values().any(|receipt| {
            same_delivery(receipt, session, delivery) && receipt.payload["action"] == "send"
        }) {
            return Err(error(
                "delivery is not the retained original dispatch of this attempt",
            ));
        }
        if previous_calls.values().any(|receipt| {
            same_delivery(receipt, session, delivery)
                && receipt.payload["executionRef"] != request.execution_ref
        }) {
            return Err(error(
                "delivery observation cannot rename the Factory Execution",
            ));
        }
    }
    let payload = json!({
        "requestRef":request.request_ref,"requestDigest":digest,"action":action,
        "agentSession":session,"deliveryRef":delivery,"executionRef":request.execution_ref,
        "nativeContractRevision":contract_revision,"cwd":cwd,
        "taskAdmission":task_admission,
        "dispatchPacket":effective_packet,
        "meaning":"Factory transport intent; not an owner receipt, quiescence or task Return"
    });
    let base = OwnerOperationReceipt {
        owner_ref: "factory".into(),
        contract: TRANSPORT_OBSERVATION.into(),
        operation_ref: intent_ref(&request),
        receipt_ref: String::new(),
        source_revision: "factory.attempt-owner-transport/v1".into(),
        phase: OwnerOperationPhase::Dispatching,
        evidence_refs: BTreeSet::new(),
        partial_effect_refs: BTreeSet::new(),
        payload,
    };
    let intent = stamp(base, OwnerOperationPhase::Dispatching, Value::Null);
    if action == "send" {
        if let Some(prepared) = &attempt.central_preparation {
            if prepared.placement_validation["enforcement"] != "native-actions" && task_admission.is_none() {
                return Err(error("protected Central work requires the task-bound AIKit dispatch admission"));
            }
            prepared.validate_dispatch_basis().map_err(error)?;
        }
    }
    // Atomic local intent publication arbitrates concurrent writers. External
    // dispatch happens only after this native transaction is durably committed.
    store
        .apply(action_request(
            &request,
            request.expected_revision,
            "intent",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: intent.clone(),
            },
        ))
        .map_err(error)?;
    let timeout = attempt
        .disposition
        .budget
        .wall_clock_timeout_ms
        .unwrap_or(DEFAULT_OWNER_TIMEOUT_MS)
        .min(DEFAULT_OWNER_TIMEOUT_MS);
    let effective_invocation = NativeOwnerInvocation::AikitEncounter {
        binary: binary.clone(),
        cwd: cwd.clone(),
        contract_revision: contract_revision.clone(),
        request: effective_packet,
    };
    let owner = match invoke_native_owner_bounded(&effective_invocation, timeout) {
        Ok(receipt) => receipt,
        Err(failure) => {
            let uncertain = stamp(
                intent,
                OwnerOperationPhase::Uncertain,
                json!({"transportError":failure.to_string(),"instruction":"inspect original delivery; never resend"}),
            );
            let retention = retain(
                &mut store,
                &request,
                "uncertain",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: uncertain.clone(),
                },
            )
            .err()
            .map(|failure| failure.to_string());
            return Ok(result(
                &store,
                &request,
                false,
                uncertain,
                None,
                retention,
            ));
        }
    };
    let completion = (|| -> Result<OwnerOperationReceipt, AttemptOwnerError> {
        let current = store.reading().map_err(error)?;
        task_dispatch::validate_response(record(&current, &request.attempt_ref)?, delivery, &owner).map_err(error)?;
        // First retain the actual owner evidence. Never mark our pending call
        // observed until every canonical worker/observation update succeeded.
        let current = store.reading().map_err(error)?;
        if record(&current, &request.attempt_ref)?.dispatch.is_none()
            && matches!(
                owner.phase,
                OwnerOperationPhase::Submitted
                    | OwnerOperationPhase::Returned
                    | OwnerOperationPhase::Failed
                    | OwnerOperationPhase::Cancelled
            )
        {
            store
                .apply(action_request(
                    &request,
                    current.revision,
                    "dispatch",
                    FactoryAttemptOperation::RecordDispatch {
                        attempt_ref: request.attempt_ref.clone(),
                        execution_ref: request.execution_ref.clone(),
                        receipt: owner.clone(),
                    },
                ))
                .map_err(error)?;
        } else {
            retain(
                &mut store,
                &request,
                "owner-observation",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: owner.clone(),
                },
            )?;
        }
        let reading = store.reading().map_err(error)?;
        let calls: Vec<_> = latest_transports(record(&reading, &request.attempt_ref)?)
            .into_values()
            .filter(|receipt| {
                same_delivery(receipt, session, delivery) && pending(receipt.phase)
            })
            .collect();
        let phase = if matches!(
            owner.phase,
            OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
        ) {
            OwnerOperationPhase::Uncertain
        } else {
            OwnerOperationPhase::Observed
        };
        let mut own = stamp(
            intent.clone(),
            phase,
            json!({"ownerReceipt":owner,"ownerReceiptRef":owner.receipt_ref}),
        );
        for call in calls {
            let mut settled = stamp(
                call,
                phase,
                json!({"ownerReceipt":owner,"ownerReceiptRef":owner.receipt_ref,
                "reconciledBy":request.request_ref,"meaning":"owner observation only; no replay or task Return"}),
            );
            settled.evidence_refs.extend(owner.evidence_refs.clone());
            settled.evidence_refs.insert(owner.receipt_ref.clone());
            settled
                .partial_effect_refs
                .extend(owner.partial_effect_refs.clone());
            retain(
                &mut store,
                &request,
                &format!("settle:{}", settled.operation_ref),
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: settled.clone(),
                },
            )?;
            if settled.operation_ref == intent.operation_ref {
                own = settled;
            }
        }
        Ok(own)
    })();
    match completion {
        Ok(transport) => Ok(result(
            &store,
            &request,
            false,
            transport,
            Some(owner),
            None,
        )),
        Err(failure) => {
            let uncertain = stamp(
                intent,
                OwnerOperationPhase::Uncertain,
                json!({"ownerReceipt":owner,"ownerReceiptRef":owner.receipt_ref,
                "retentionFailure":failure.to_string(),"instruction":"inspect original delivery; never resend"}),
            );
            let _ = retain(
                &mut store,
                &request,
                "retention-failed",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: uncertain.clone(),
                },
            );
            Ok(result(
                &store,
                &request,
                false,
                uncertain,
                Some(owner),
                Some(failure.to_string()),
            ))
        }
    }
}
