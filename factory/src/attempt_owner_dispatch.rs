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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_uncertainty: Option<crate::NativePublicationDetails>,
    /// Missing current readback must not be represented by a stale snapshot.
    pub reading: Option<FactoryAttemptReading>,
}

#[derive(Debug)]
pub struct AttemptOwnerError {
    message: String,
    cause: Option<Box<dyn Error + Send + Sync>>,
}
impl Display for AttemptOwnerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl Error for AttemptOwnerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause.as_ref() as &(dyn Error + 'static))
    }
}
/// A later retention failure must not replace the primary refusal. Its
/// actual native cause remains traversable while Display keeps the original
/// owner/semantic failure and both objects remain retained in this wrapper.
#[derive(Debug)]
struct RetentionFailures {
    primary: AttemptOwnerError,
    secondary: AttemptOwnerError,
}
impl Display for RetentionFailures {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.primary, formatter)
    }
}
impl Error for RetentionFailures {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.secondary)
    }
}
impl AttemptOwnerError {
    fn with_secondary(self, secondary: AttemptOwnerError) -> Self {
        Self {
            message: self.to_string(),
            cause: Some(Box::new(RetentionFailures {
                primary: self,
                secondary,
            })),
        }
    }
}

fn error(value: impl Display) -> AttemptOwnerError {
    AttemptOwnerError {
        message: value.to_string(),
        cause: None,
    }
}
fn native_error(value: impl Error + Send + Sync + 'static) -> AttemptOwnerError {
    AttemptOwnerError {
        message: value.to_string(),
        cause: Some(Box::new(value)),
    }
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
        let reading = store.reading().map_err(native_error)?;
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
                // A changed revision after rename is a possible committed effect,
                // not permission to retry an operation with unknown publication.
                if crate::native_publication_uncertainty(&failure).is_some() {
                    return Err(native_error(failure));
                }
                if store.reading().map_err(native_error)?.revision == reading.revision {
                    return Err(native_error(failure));
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
    retention_failure: Option<AttemptOwnerError>,
) -> FactoryAttemptOwnerReceipt {
    let mut publication_uncertainty = retention_failure
        .as_ref()
        .and_then(|error| crate::native_publication_uncertainty(error))
        .map(|failure| failure.details());
    if publication_uncertainty.is_none() {
        // Replay discloses actual retained owner evidence, never error prose.
        publication_uncertainty = transport
            .payload
            .pointer("/detail/publicationUncertainty")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());
    }
    let mut retention_error = retention_failure.as_ref().map(ToString::to_string);
    let reading = match store.reading() {
        Ok(reading) => Some(reading),
        Err(failure) => {
            if let Some(details) = &mut publication_uncertainty {
                if details.readback_observation.is_none() {
                    details.readback_observation =
                        Some(format!("subsequent native readback failed: {failure}"));
                }
            }
            if retention_error.is_none() {
                retention_error = Some(failure.to_string());
            }
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
        publication_uncertainty,
        reading,
    }
}

fn completion_failure(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    intent: OwnerOperationReceipt,
    owner: OwnerOperationReceipt,
    mut failure: AttemptOwnerError,
) -> FactoryAttemptOwnerReceipt {
    let mut detail = json!({"ownerReceipt":owner,"ownerReceiptRef":owner.receipt_ref,
        "retentionFailure":failure.to_string(),"instruction":"inspect original delivery; never resend"});
    if let Some(publication) = crate::native_publication_uncertainty(&failure) {
        detail["publicationUncertainty"] = serde_json::to_value(publication.details())
            .expect("native publication details serialize");
    }
    let mut uncertain = stamp(intent, OwnerOperationPhase::Uncertain, detail);
    if crate::native_publication_uncertainty(&failure).is_none() {
        if let Err(secondary) = retain(
            store,
            request,
            "retention-failed",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: uncertain.clone(),
            },
        ) {
            let mut detail = uncertain.payload["detail"].clone();
            detail["secondaryRetentionError"] = json!(secondary.to_string());
            if let Some(publication) = crate::native_publication_uncertainty(&secondary) {
                detail["secondaryPublicationUncertainty"] =
                    serde_json::to_value(publication.details())
                        .expect("native publication details serialize");
            }
            // These returned failure facts were observed after publication;
            // do not write them again onto that same uncertain attempt source.
            uncertain = stamp(uncertain, OwnerOperationPhase::Uncertain, detail);
            failure = failure.with_secondary(secondary);
        }
    }
    result(store, request, false, uncertain, Some(owner), Some(failure))
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
        FileAttemptStore::open_run(state_path, request.run_ref.clone()).map_err(native_error)?;
    let reading = store.reading().map_err(native_error)?;
    let attempt = record(&reading, &request.attempt_ref)?;
    // Retain the original v1 request digest for existing persisted Action replay.
    let digest = blake3::hash(&serde_json::to_vec(&request).map_err(error)?)
        .to_hex()
        .to_string();
    if let Some(previous) = latest_transports(attempt).get(&intent_ref(&request)) {
        if previous.payload["requestDigest"].as_str() != Some(&digest) {
            return Err(error(
                "owner Action identity was reused with different content",
            ));
        }
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
        let owner = if let Some(value) = previous
            .payload
            .pointer("/detail/ownerReceipt")
            .filter(|value| !value.is_null())
        {
            Some(serde_json::from_value(value.clone()).map_err(error)?)
        } else {
            previous
                .payload
                .pointer("/detail/ownerReceiptRef")
                .and_then(Value::as_str)
                .and_then(|reference| {
                    attempt
                        .observations
                        .iter()
                        .find(|receipt| receipt.receipt_ref == reference)
                })
                .cloned()
        };
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
        transport,
        contract_revision,
        request: packet,
    } = &request.invocation
    else {
        return Err(error(
            "this attempt handoff supports AIKit send/delivery only",
        ));
    };
    if !matches!(
        contract_revision.as_str(),
        AIKIT_CAW_CONTRACT_REVISION | AIKIT_TASK_CONTRACT_REVISION
    ) || !cwd.is_absolute()
    {
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
        .ok_or_else(|| error("an exact native delivery identity is required"))?;
    if session != attempt.disposition.body.agent_session_ref
        || request.execution_ref == session
        || request.execution_ref == delivery
        || request.execution_ref == attempt.reserved_execution_ref
        || attempt
            .execution_ref
            .as_ref()
            .is_some_and(|current| current != &request.execution_ref)
        || reading.attempts.iter().any(|other| {
            other.attempt_ref != request.attempt_ref
                && (other.execution_ref.as_ref() == Some(&request.execution_ref)
                    || other.observations.iter().any(|receipt| {
                        is_transport(receipt)
                            && receipt.payload["executionRef"].as_str()
                                == Some(request.execution_ref.as_str())
                    }))
        })
    {
        return Err(error(
            "attempt, Execution and AgentSession identities disagree or collide",
        ));
    }
    let mut transport_identity =
        json!({"binary":binary, "cwd":cwd, "contractRevision":contract_revision});
    if let Some(transport) = transport {
        transport_identity["transport"] = serde_json::to_value(transport).map_err(error)?;
    }
    let prior_send = attempt.observations.iter().find(|receipt| {
        same_delivery(receipt, session, delivery)
            && receipt.payload["action"].as_str() == Some("send")
    });
    let dispatch_started = std::time::Instant::now();
    let mut effective_packet = packet.clone();
    let mut task_admission = None;
    if action == "send" {
        let leg = reading
            .legs
            .get(&attempt.workflow_unit_ref)
            .ok_or_else(|| error("attempt has no canonical workflow leg"))?;
        if !reading.source_current
            || attempt.execution_ref.is_some()
            || leg.execution_ref != attempt.reserved_execution_ref
            || leg.status != crate::orchestration::LegStatus::Active
            || attempt.observations.iter().any(|receipt| {
                is_transport(receipt) && receipt.payload["action"].as_str() == Some("send")
            })
        {
            return Err(error("dispatch requires a current unbound active attempt with no prior send; inspect its original delivery"));
        }
        let protected = attempt
            .disposition
            .placement
            .as_ref()
            .is_some_and(|placement| {
                !placement.required_coverage.is_empty() || !placement.protected_paths.is_empty()
            });
        let writing = attempt.disposition.permitted_effects.iter().any(|effect| {
            let effect = effect.to_ascii_lowercase();
            effect.starts_with("write:")
                || effect.contains("write ")
                || effect.starts_with("mutate:")
        });
        if protected || writing || packet.pointer("/turn/expected_task").is_some() {
            if contract_revision != AIKIT_TASK_CONTRACT_REVISION {
                return Err(error("Protected/writing attempts require AIKit's actual task-dispatch contract; plain encounter delivery is not enforcement"));
            }
            task_admission = Some(
                task_dispatch::prepare(
                    &reading,
                    attempt,
                    binary,
                    cwd,
                    transport.as_ref(),
                    packet,
                    attempt
                        .disposition
                        .budget
                        .wall_clock_timeout_ms
                        .unwrap_or(DEFAULT_OWNER_TIMEOUT_MS)
                        .min(DEFAULT_OWNER_TIMEOUT_MS),
                )
                .map_err(error)?,
            );
        }
        if packet.pointer("/turn/sender").and_then(Value::as_str)
            != Some(request.caller.caller_ref.as_str())
            || packet.pointer("/turn/packet/audience")
                != Some(&json!([attempt.disposition.participant.agent_ref]))
            || packet
                .pointer("/turn/expected_binding_revision")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(error(
                "native sender, audience and binding revision must name the selected caller/Agent",
            ));
        }
        let task = packet
            .pointer("/turn/packet/text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| error("native packet must contain the explicit task"))?;
        let bounds = json!({"delegation":leg.delegation, "execution":request.execution_ref, "disposition":attempt.disposition,
            "attemptRef":attempt.attempt_ref,"taskRef":attempt.task_ref,
            "workflowBasis":{"ref":reading.workflow_source_ref,"revision":reading.workflow_source_revision,"digest":reading.workflow_source_digest}});
        effective_packet["turn"]["packet"]["text"] = Value::String(format!(
            "Factory bounded attempt. These are the authorised task conditions, not new authority:\n{}\n\nTask:\n{task}",
            serde_json::to_string(&bounds).map_err(error)?));
    } else {
        let sent = prior_send
            .ok_or_else(|| error("delivery read must name this attempt's retained send intent"))?;
        if sent.payload["executionRef"].as_str() != Some(request.execution_ref.as_str())
            || sent
                .payload
                .get("transport")
                .is_some_and(|identity| identity != &transport_identity)
        {
            return Err(error(
                "delivery recovery cannot silently replace its original transport or Execution",
            ));
        }
        // Older v1 intents did not retain transport configuration. Their exact
        // native delivery/session still remains mandatory for compatible reads.
    }
    let timeout_ms = attempt
        .disposition
        .budget
        .wall_clock_timeout_ms
        .unwrap_or(DEFAULT_OWNER_TIMEOUT_MS)
        .min(DEFAULT_OWNER_TIMEOUT_MS);
    if timeout_ms == 0 {
        return Err(error("native owner transport budget is exhausted"));
    }
    let mut intent = stamp(
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: TRANSPORT_OBSERVATION.into(),
            operation_ref: intent_ref(&request),
            receipt_ref: String::new(),
            source_revision: format!("factory-state:{}", request.expected_revision),
            phase: OwnerOperationPhase::Dispatching,
            evidence_refs: BTreeSet::new(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"requestDigest":digest,"agentSession":session,"deliveryRef":delivery,"action":action,
            "executionRef":request.execution_ref,"transport":transport_identity,"taskAdmission":task_admission}),
        },
        OwnerOperationPhase::Dispatching,
        json!({"meaning":"Factory intent before transport, not worker execution"}),
    );
    if action == "send" {
        crate::attempt_runtime::validate_action_request(&action_request(
            &request,
            request.expected_revision,
            "admission",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: intent.clone(),
            },
        ))
        .map_err(error)?;
        if let Some(preflight) = crate::attempt_central::preflight(attempt, cwd).map_err(error)? {
            if preflight["policy"]["data"]["enforcement"] != "native-actions"
                && task_admission.is_none()
            {
                return Err(error("the native policy requires worker interception/material enforcement not established by plain session delivery"));
            }
            intent.payload["placementPreflight"] = preflight.clone();
            intent = stamp(
                intent,
                OwnerOperationPhase::Dispatching,
                json!({"meaning":"Factory intent after fresh native Central readback, not worker enforcement"}),
            );
            let body = effective_packet["turn"]["packet"]["text"]
                .as_str()
                .ok_or_else(|| error("missing bounded task"))?;
            effective_packet["turn"]["packet"]["text"] = json!(format!("{body}\n\nNative Central placement preflight (not new authority or worker confinement):\n{}",serde_json::to_string(&preflight).map_err(error)?));
        }
    }
    let elapsed_ms = dispatch_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let timeout_ms = timeout_ms.saturating_sub(elapsed_ms);
    if timeout_ms == 0 {
        return Err(error(
            "native owner transport budget exhausted during preparation",
        ));
    }
    let invocation = NativeOwnerInvocation::AikitEncounter {
        binary: binary.clone(),
        cwd: cwd.clone(),
        transport: transport.clone(),
        contract_revision: contract_revision.clone(),
        request: effective_packet,
    };
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
        .map_err(native_error)?;
    let owner = match invoke_native_owner_bounded(&invocation, timeout_ms) {
        Ok(owner) => owner,
        Err(failure) => {
            let uncertain = stamp(
                intent,
                OwnerOperationPhase::Uncertain,
                json!({"failure":failure.to_string(),"instruction":"inspect original delivery, do not resend"}),
            );
            let retention_error = retain(
                &mut store,
                &request,
                "uncertain",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: uncertain.clone(),
                },
            )
            .err();
            return Ok(result(
                &store,
                &request,
                false,
                uncertain,
                None,
                retention_error,
            ));
        }
    };
    let completion = (|| -> Result<OwnerOperationReceipt, AttemptOwnerError> {
        retain(
            &mut store,
            &request,
            "owner-evidence",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: owner.clone(),
            },
        )?;
        let current = store.reading().map_err(native_error)?;
        let current_attempt = record(&current, &request.attempt_ref)?;
        task_dispatch::validate_response(current_attempt, delivery, &owner).map_err(error)?;
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
                    leg.execution_ref == current_attempt.reserved_execution_ref
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
        let current = store.reading().map_err(native_error)?;
        let current_attempt = record(&current, &request.attempt_ref)?;
        let mut calls = latest_transports(current_attempt)
            .into_values()
            .filter(|receipt| same_delivery(receipt, session, delivery) && pending(receipt.phase))
            .collect::<Vec<_>>();
        // Current call settles last: any interrupted multi-receipt publication
        // leaves a blocking intent rather than a premature resolved readback.
        calls.sort_by_key(|receipt| receipt.operation_ref == intent.operation_ref);
        let phase = if pending(owner.phase) {
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
        Err(failure) => Ok(completion_failure(
            &mut store, &request, intent, owner, failure,
        )),
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod publication_tests {
    use super::*;
    use crate::attempt_learning::publication_tests::{native_observation, native_retention_source};
    use std::cell::Cell;
    use std::os::unix::fs::PermissionsExt;
    use std::rc::Rc;

    #[test]
    fn actual_fallback_publication_retains_primary_refusal_and_secondary_native_cause() {
        for move_after_publication in [false, true] {
            let (root, mut store, base) = native_retention_source();
            let path = root.path().join("state.json");
            let physical = path.canonicalize().unwrap();
            let retained = root.path().join("retained-published-state.json");
            let request = FactoryAttemptOwnerRequest {
                contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
                request_ref: base.request_ref,
                projection_ref: base.projection_ref,
                caller: base.caller,
                run_ref: base.run_ref,
                expected_revision: store.reading().unwrap().revision,
                authority: base.authority,
                attempt_ref: base.attempt_ref,
                execution_ref: "execution:native-retention-only".into(),
                invocation: NativeOwnerInvocation::AikitEncounter {
                    binary: std::env::current_exe().unwrap(),
                    cwd: root.path().to_path_buf(),
                    transport: None,
                    contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                    request: Value::Null,
                },
            };
            // This owner receipt is the actual Factory source reading, not an
            // invented AIKit response, provider execution or worker Return.
            let owner = native_observation(&store);
            retain(
                &mut store,
                &request,
                "actual-owner-reading",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: owner.clone(),
                },
            )
            .unwrap();
            let mut intent = owner.clone();
            intent.contract = TRANSPORT_OBSERVATION.into();
            intent.operation_ref = format!("factory-owner-call:{}", request.request_ref);
            intent = stamp(
                intent,
                OwnerOperationPhase::Dispatching,
                json!({"meaning":"controlled retention intent; no external owner invoked"}),
            );
            let before = store.reading().unwrap().revision;
            let before_bytes = std::fs::read(&path).unwrap();
            let primary = native_error(
                store
                    .apply(action_request(
                        &request,
                        before,
                        "actual-semantic-refusal",
                        FactoryAttemptOperation::RecordObservation {
                            attempt_ref: "attempt:not-retained".into(),
                            receipt: owner.clone(),
                        },
                    ))
                    .unwrap_err(),
            );
            assert!(crate::native_publication_uncertainty(&primary).is_none());
            let primary_message = primary.to_string();
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before_bytes,
                "actual semantic refusal precedes any publication"
            );
            let seen = Rc::new(Cell::new(0));
            let seen_for_observer = seen.clone();
            let retained_for_observer = retained.clone();
            let physical_for_observer = physical.clone();
            crate::native_file_transaction::observe_next_publication(move |published| {
                assert_eq!(published, physical_for_observer);
                seen_for_observer.set(seen_for_observer.get() + 1);
                if move_after_publication {
                    std::fs::rename(published, &retained_for_observer).unwrap();
                } else {
                    std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                }
            });
            let returned = completion_failure(&mut store, &request, intent, owner.clone(), primary);
            assert_eq!(
                seen.get(),
                1,
                "one actual fallback publication; no retry/settlement"
            );
            assert_eq!(
                returned.retention_error.as_deref(),
                Some(primary_message.as_str())
            );
            assert_eq!(returned.owner_receipt.as_ref(), Some(&owner));
            assert_eq!(
                returned.transport_observation.payload["detail"]["retentionFailure"],
                primary_message
            );
            let secondary = &returned.transport_observation.payload["detail"]
                ["secondaryPublicationUncertainty"];
            assert_eq!(secondary["source_path"], physical.display().to_string());
            assert_eq!(secondary["published"], true);
            assert_eq!(secondary["outcome"], "unknown");
            assert_eq!(secondary["automatic_retry"], false);
            let details = returned.publication_uncertainty.as_ref().unwrap();
            assert_eq!(details.source_path, physical);
            assert!(returned.needs_reconciliation);
            if move_after_publication {
                assert_eq!(details.cause.raw_os_error, Some(libc::ENOENT));
                assert!(returned.reading.is_none());
            }
            let readback = FileAttemptStore::open_run(
                if move_after_publication {
                    &retained
                } else {
                    &path
                },
                request.run_ref,
            )
            .unwrap()
            .reading()
            .unwrap();
            assert_eq!(readback.revision, before + 1);
            let attempt = record(&readback, &request.attempt_ref).unwrap();
            assert!(
                attempt.observations.contains(&owner),
                "actual source owner reading remains retained"
            );
            assert_eq!(
                attempt
                    .observations
                    .iter()
                    .filter(|receipt| receipt.contract == TRANSPORT_OBSERVATION)
                    .count(),
                1
            );
            assert!(
                attempt.readable_return.is_none(),
                "physical retention cannot create a worker Return"
            );
        }
    }
}
