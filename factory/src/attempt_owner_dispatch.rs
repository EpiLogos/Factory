//! Consequential native owner calls attached to the existing persisted attempt.
//!
//! Factory records its own intent before transport. That intent is not an AIKit
//! receipt or evidence that a worker ran. An interrupted call is never resent by
//! replaying this Action: an explicit delivery read reconciles its owner state.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::{
    FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptReading,
    FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION,
};
use crate::core::run::RunRef;
use crate::native_owner::{
    invoke_native_owner, NativeOwnerInvocation, AIKIT_CAW_CONTRACT_REVISION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

pub const FACTORY_ATTEMPT_OWNER_ACTION: &str = "factory.attempt-owner-action/v1";
pub const FACTORY_ATTEMPT_OWNER_RECEIPT: &str = "factory.attempt-owner-receipt/v1";
const TRANSPORT_OBSERVATION: &str = "factory.attempt-owner-transport/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptOwnerRequest {
    pub contract: String,
    /// Stable identity of this explicit owner call, distinct from UI projection.
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    /// Factory Execution identity; never an AgentSession or delivery identifier.
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
    pub reading: FactoryAttemptReading,
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
    attempt_ref: &str,
) -> Result<&'a FactoryAttemptRecord, AttemptOwnerError> {
    reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == attempt_ref)
        .ok_or_else(|| error("attempt is not retained in this canonical Run"))
}

fn intent_ref(request: &FactoryAttemptOwnerRequest) -> String {
    format!("factory-attempt-owner-call:{}", request.request_ref)
}

fn is_transport(receipt: &OwnerOperationReceipt) -> bool {
    receipt.owner_ref == "factory" && receipt.contract == TRANSPORT_OBSERVATION
}

fn last_transport<'a>(
    attempt: &'a FactoryAttemptRecord,
    operation_ref: &str,
) -> Option<&'a OwnerOperationReceipt> {
    attempt
        .observations
        .iter()
        .rev()
        .find(|receipt| is_transport(receipt) && receipt.operation_ref == operation_ref)
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

/// Persist evidence with fresh CAS after transport, without ever repeating the
/// external effect on a conflict. The finite retries concern local retention.
fn retain(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> Result<(), AttemptOwnerError> {
    for _ in 0..8 {
        let reading = store.reading().map_err(error)?;
        match store.apply(action_request(
            request,
            reading.revision,
            suffix,
            operation.clone(),
        )) {
            Ok(_) => return Ok(()),
            Err(failure) => {
                let current = store.reading().map_err(error)?;
                if current.revision == reading.revision {
                    return Err(error(failure));
                }
            }
        }
    }
    Err(error(
        "native receipt retention remained contended; do not resend; reconcile the delivery",
    ))
}

fn transport_receipt(
    request: &FactoryAttemptOwnerRequest,
    digest: &str,
    session: &str,
    delivery: &str,
    action: &str,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    let payload = json!({
        "requestDigest": digest,
        "agentSession": session,
        "deliveryRef": delivery,
        "action": action,
        "executionRef": request.execution_ref,
        "detail": detail
    });
    let encoded = serde_json::to_vec(&payload).expect("JSON values serialize");
    OwnerOperationReceipt {
        owner_ref: "factory".into(),
        contract: TRANSPORT_OBSERVATION.into(),
        operation_ref: intent_ref(request),
        receipt_ref: format!(
            "factory-owner-transport:{}:{phase:?}:{}",
            request.request_ref,
            blake3::hash(&encoded).to_hex()
        ),
        source_revision: format!("factory-state:{}", request.expected_revision),
        phase,
        evidence_refs: BTreeSet::new(),
        partial_effect_refs: BTreeSet::new(),
        payload,
    }
}

fn result(
    store: &FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    replayed: bool,
    transport: OwnerOperationReceipt,
    owner_receipt: Option<OwnerOperationReceipt>,
) -> Result<FactoryAttemptOwnerReceipt, AttemptOwnerError> {
    Ok(FactoryAttemptOwnerReceipt {
        contract: FACTORY_ATTEMPT_OWNER_RECEIPT.into(),
        request_ref: request.request_ref.clone(),
        run_ref: request.run_ref.clone(),
        attempt_ref: request.attempt_ref.clone(),
        replayed,
        needs_reconciliation: matches!(
            transport.phase,
            OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
        ),
        transport_observation: transport,
        owner_receipt,
        reading: store.reading().map_err(error)?,
    })
}

/// Dispatch or observe an already arranged native AIKit session. This does not
/// provision a session, choose a model, install a guard, verify an artifact or
/// manufacture task completion from a provider turn. Other owner operations
/// remain available through their existing explicit native adapter.
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
    let mut store = FileAttemptStore::open_run(state_path, request.run_ref.clone()).map_err(error)?;
    let reading = store.reading().map_err(error)?;
    let attempt = record(&reading, &request.attempt_ref)?;
    let encoded = serde_json::to_vec(&request).map_err(error)?;
    let digest = blake3::hash(&encoded).to_hex().to_string();
    if let Some(previous) = last_transport(attempt, &intent_ref(&request)) {
        if previous.payload["requestDigest"].as_str() != Some(&digest) {
            return Err(error(
                "owner Action identity was reused with different content",
            ));
        }
        // Recheck the public Action authority even on a read-only replay. The
        // original intent is already retained, so its Action is idempotent.
        crate::attempt_runtime::validate_action_request(&action_request(
            &request,
            reading.revision,
            "replay",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: previous.clone(),
            },
        ))
        .map_err(error)?;
        let owner_receipt = previous.payload["detail"]["ownerReceiptRef"]
            .as_str()
            .and_then(|reference| {
                attempt
                    .observations
                    .iter()
                    .find(|receipt| receipt.receipt_ref == reference)
            })
            .cloned();
        return result(&store, &request, true, previous.clone(), owner_receipt);
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
    if contract_revision != AIKIT_CAW_CONTRACT_REVISION {
        return Err(error(
            "AIKit owner contract revision does not match the published adapter",
        ));
    }
    let action = packet["action"].as_str().unwrap_or_default();
    let session = packet["agent_session"].as_str().unwrap_or_default();
    let delivery = match action {
        "send" => packet.pointer("/turn/delivery_ref"),
        "delivery" => packet.get("delivery_ref"),
        _ => {
            return Err(error(
                "attempt owner Action supports send or delivery, not a guessed operation",
            ))
        }
    }
    .and_then(Value::as_str)
    .filter(|value| !value.trim().is_empty())
    .ok_or_else(|| error("an exact native delivery identity is required"))?;
    if session != attempt.disposition.body.agent_session_ref
        || request.execution_ref == session
        || request.execution_ref == delivery
        || attempt
            .execution_ref
            .as_ref()
            .is_some_and(|current| current != &request.execution_ref)
    {
        return Err(error(
            "attempt, Execution and native AgentSession identities disagree",
        ));
    }
    let same_delivery = |receipt: &&OwnerOperationReceipt| {
        is_transport(receipt)
            && receipt.payload["agentSession"].as_str() == Some(session)
            && receipt.payload["deliveryRef"].as_str() == Some(delivery)
    };
    let prior_send = attempt
        .observations
        .iter()
        .filter(same_delivery)
        .any(|receipt| receipt.payload["action"].as_str() == Some("send"));
    let mut effective_packet = packet.clone();
    if action == "send" {
        if !reading.source_current || attempt.execution_ref.is_some() {
            return Err(error(
                "dispatch requires the current source and an unbound attempt",
            ));
        }
        if attempt.observations.iter().any(|receipt| {
            is_transport(receipt) && receipt.payload["action"].as_str() == Some("send")
        }) {
            return Err(error(
                "this attempt already has a dispatch intent; observe its delivery, never resend",
            ));
        }
        let leg = reading
            .legs
            .get(&attempt.workflow_unit_ref)
            .ok_or_else(|| error("attempt has no canonical workflow leg"))?;
        if leg.execution_ref != format!("factory-attempt:{}", request.attempt_ref)
            || leg.status != crate::orchestration::LegStatus::Active
        {
            return Err(error(
                "dispatch cannot revive a historical, cancelled or non-active attempt",
            ));
        }
        if packet.pointer("/turn/sender").and_then(Value::as_str)
            != Some(request.caller.caller_ref.as_str())
            || packet.pointer("/turn/packet/audience")
                != Some(&json!([attempt.disposition.participant.agent_ref]))
        {
            return Err(error(
                "native sender/audience must match the caller and selected enduring Agent",
            ));
        }
        let original = packet
            .pointer("/turn/packet/text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| error("native packet must contain the explicit task"))?;
        let bounds = json!({
            "delegation": leg.delegation,
            "execution": request.execution_ref,
            "disposition": attempt.disposition
        });
        effective_packet["turn"]["packet"]["text"] = Value::String(format!(
            "Factory bounded attempt. These are the authorised task conditions, not new authority:\n{}\n\nTask:\n{original}",
            serde_json::to_string(&bounds).map_err(error)?
        ));
    } else if !prior_send {
        return Err(error(
            "delivery observation must name this attempt's retained native dispatch intent",
        ));
    }
    let invocation = NativeOwnerInvocation::AikitEncounter {
        binary: binary.clone(),
        cwd: cwd.clone(),
        contract_revision: contract_revision.clone(),
        request: effective_packet,
    };
    let intent = transport_receipt(
        &request,
        &digest,
        session,
        delivery,
        action,
        OwnerOperationPhase::Dispatching,
        json!({"meaning": "Factory intent persisted before native transport; no worker result"}),
    );
    // One native CAS: concurrent callers cannot both pass the same opening basis.
    store
        .apply(action_request(
            &request,
            request.expected_revision,
            "intent",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: intent,
            },
        ))
        .map_err(error)?;

    let owner = match invoke_native_owner(&invocation) {
        Ok(receipt) => receipt,
        Err(failure) => {
            let uncertain = transport_receipt(
                &request,
                &digest,
                session,
                delivery,
                action,
                OwnerOperationPhase::Uncertain,
                json!({"failure": failure.to_string(), "instruction": "observe exact owner delivery; do not resend"}),
            );
            retain(
                &mut store,
                &request,
                "uncertain",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: uncertain.clone(),
                },
            )?;
            return result(&store, &request, false, uncertain, None);
        }
    };
    // Retain the actual owner receipt before any consequential interpretation.
    retain(
        &mut store,
        &request,
        "owner-evidence",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: owner.clone(),
        },
    )?;
    let current = store.reading().map_err(error)?;
    let current_attempt = record(&current, &request.attempt_ref)?;
    if current_attempt.execution_ref.is_none()
        && current.source_current
        && matches!(
            owner.phase,
            OwnerOperationPhase::Submitted | OwnerOperationPhase::Returned
        )
        && current
            .legs
            .get(&current_attempt.workflow_unit_ref)
            .is_some_and(|leg| {
                leg.execution_ref == format!("factory-attempt:{}", request.attempt_ref)
                    && leg.status == crate::orchestration::LegStatus::Active
            })
    {
        retain(
            &mut store,
            &request,
            "bind",
            FactoryAttemptOperation::BindDispatch {
                attempt_ref: request.attempt_ref.clone(),
                execution_ref: request.execution_ref.clone(),
                receipt: owner.clone(),
            },
        )?;
    }
    let phase = if matches!(
        owner.phase,
        OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
    ) {
        OwnerOperationPhase::Uncertain
    } else {
        OwnerOperationPhase::Observed
    };
    let settled = transport_receipt(
        &request,
        &digest,
        session,
        delivery,
        action,
        phase,
        json!({"ownerReceiptRef": owner.receipt_ref, "meaning": "transport observation only; task Return is separate"}),
    );
    retain(
        &mut store,
        &request,
        "transport-result",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: settled.clone(),
        },
    )?;
    if action == "delivery" && phase == OwnerOperationPhase::Observed {
        let current = store.reading().map_err(error)?;
        let attempt = record(&current, &request.attempt_ref)?;
        let prior_refs = attempt
            .observations
            .iter()
            .filter(same_delivery)
            .filter(|receipt| receipt.operation_ref != intent_ref(&request))
            .map(|receipt| receipt.operation_ref.clone())
            .collect::<BTreeSet<_>>();
        for operation_ref in prior_refs {
            let Some(previous) = last_transport(attempt, &operation_ref) else {
                continue;
            };
            if !matches!(
                previous.phase,
                OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
            ) {
                continue;
            }
            let mut resolution = previous.clone();
            resolution.phase = OwnerOperationPhase::Observed;
            resolution.receipt_ref = format!(
                "{}:reconciled:{}",
                previous.receipt_ref, request.request_ref
            );
            resolution.payload["detail"] =
                json!({"ownerReceiptRef": owner.receipt_ref, "reconciledBy": request.request_ref});
            retain(
                &mut store,
                &request,
                "reconcile",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: resolution,
                },
            )?;
        }
    }
    result(&store, &request, false, settled, Some(owner))
}
