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
    invoke_native_owner_bounded, NativeOwnerError, NativeOwnerInvocation,
    AIKIT_CAW_CONTRACT_REVISION, DEFAULT_OWNER_TIMEOUT_MS,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

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

#[derive(Clone, Serialize, Deserialize)]
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
impl std::fmt::Debug for FactoryAttemptOwnerReceipt {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FactoryAttemptOwnerReceipt")
            .field("contract", &self.contract)
            .field("replayed", &self.replayed)
            .field("needs_reconciliation", &self.needs_reconciliation)
            .field("transport_phase", &self.transport_observation.phase)
            .field("has_owner_receipt", &self.owner_receipt.is_some())
            .field("private_evidence", &"withheld")
            .finish()
    }
}

#[derive(Clone)]
pub struct AttemptOwnerError {
    message: String,
    cause: Option<Arc<dyn Error + Send + Sync>>,
    // Actual returned receipt only. It carries no live error; no cycle/history
    // reconstruction or new durable result carrier is created.
    native_result: Option<Arc<FactoryAttemptOwnerReceipt>>,
}
impl std::fmt::Debug for AttemptOwnerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttemptOwnerError")
            .field("has_cause", &self.cause.is_some())
            .field("private_evidence", &"withheld")
            .finish()
    }
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
#[derive(Clone, Debug)]
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
        // A later failed readback must not hide an already-published native
        // uncertainty from the generic no-retry fence. Both actual errors
        // remain retained and explicitly accessible; this chooses a chain.
        if crate::native_publication_uncertainty(&self.secondary).is_some() {
            Some(&self.secondary)
        } else if crate::native_publication_uncertainty(&self.primary).is_some() {
            Some(&self.primary)
        } else {
            Some(&self.secondary)
        }
    }
}
impl AttemptOwnerError {
    /// The exact uncertain public receipt produced by this failed invocation.
    pub fn native_result(&self) -> Option<&FactoryAttemptOwnerReceipt> {
        self.native_result.as_deref()
    }
    fn with_receipt(mut self, receipt: FactoryAttemptOwnerReceipt) -> Self {
        self.native_result = Some(Arc::new(receipt));
        self
    }
    /// Original actual invocation, including the native IO/capture source chain.
    /// The private native object is not formatted or serialized by this wrapper.
    pub fn original_native_owner_error(&self) -> Option<&NativeOwnerError> {
        let cause = self.cause.as_deref()?;
        cause.downcast_ref::<NativeOwnerError>().or_else(|| {
            cause
                .downcast_ref::<RetentionFailures>()?
                .primary
                .original_native_owner_error()
        })
    }
    /// Original retained refusal when a distinct later failure was added.
    pub fn primary_retention_error(&self) -> Option<&AttemptOwnerError> {
        self.cause
            .as_deref()?
            .downcast_ref::<RetentionFailures>()
            .map(|causes| &causes.primary)
    }
    /// Distinct actual post-invocation retention failure, when one occurred.
    pub fn secondary_retention_error(&self) -> Option<&AttemptOwnerError> {
        self.cause
            .as_deref()?
            .downcast_ref::<RetentionFailures>()
            .map(|causes| &causes.secondary)
    }
    pub(crate) fn with_secondary_native(self, cause: impl Error + Send + Sync + 'static) -> Self {
        self.with_secondary(native_error(cause))
    }
    fn with_secondary(self, secondary: AttemptOwnerError) -> Self {
        let native_result = self.native_result.clone();
        Self {
            native_result,
            message: self.to_string(),
            cause: Some(Arc::new(RetentionFailures {
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
        native_result: None,
    }
}
fn native_error(value: impl Error + Send + Sync + 'static) -> AttemptOwnerError {
    AttemptOwnerError {
        message: value.to_string(),
        cause: Some(Arc::new(value)),
        native_result: None,
    }
}

// Owner errors may contain raw output, payloads and argv-bearing diagnostics.
// Keep those actual objects private; public text contains only native type facts.
fn invocation_error(failure: NativeOwnerError) -> AttemptOwnerError {
    let message = match &failure {
        NativeOwnerError::Spawn { owner, error, .. } => format!(
            "{owner} owner transport I/O failure ({:?}, native errno {:?}); effects unknown; inspect original delivery, do not resend",
            error.kind(), error.raw_os_error()),
        NativeOwnerError::Io(error) => format!(
            "native owner I/O failure ({:?}, native errno {:?}); effects unknown; do not resend",
            error.kind(), error.raw_os_error()),
        NativeOwnerError::CommandFailed { owner, code, .. } => format!(
            "{owner} owner command failed with code {code:?}; private native response retained; do not resend"),
        NativeOwnerError::Json(_) => "native owner JSON failure; private cause retained; do not resend".into(),
        NativeOwnerError::InvalidResponse(_) => "invalid native owner response; private cause retained; effects unknown; do not resend".into(),
        NativeOwnerError::InvalidInvocation(_) => "invalid native owner invocation; private cause retained; do not resend".into(),
        NativeOwnerError::ContractRevisionMismatch { owner, .. } => format!(
            "{owner} native owner contract mismatch; private cause retained; do not resend"),
        NativeOwnerError::ToolObservation(_) => "native owner tool observation failed; private cause retained; effects unknown; do not resend".into(),
    };
    AttemptOwnerError {
        message,
        cause: Some(Arc::new(failure)),
        native_result: None,
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
                match store.reading() {
                    Ok(current) if current.revision != reading.revision => {}
                    Ok(_) => return Err(native_error(failure)),
                    Err(readback) => {
                        return Err(native_error(failure).with_secondary_native(readback));
                    }
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
    mut retention_failure: Option<AttemptOwnerError>,
) -> Result<FactoryAttemptOwnerReceipt, AttemptOwnerError> {
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
            let readback = native_error(failure);
            retention_failure = Some(match retention_failure {
                Some(primary) => primary.with_secondary(readback),
                None => readback,
            });
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
    let receipt = FactoryAttemptOwnerReceipt {
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
    };
    match retention_failure {
        Some(failure) => Err(failure.with_receipt(receipt)),
        None => Ok(receipt),
    }
}

// Same persisted intent/receipt owner; one retention attempt path, never a
// second invocation or a compensating write after publication uncertainty.
fn invocation_failure(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    intent: OwnerOperationReceipt,
    failure: NativeOwnerError,
) -> AttemptOwnerError {
    let primary = invocation_error(failure);
    let uncertain = stamp(
        intent,
        OwnerOperationPhase::Uncertain,
        json!({"failure":primary.to_string(),"instruction":"inspect original delivery, do not resend"}),
    );
    let retention_failure = retain(
        store,
        request,
        "uncertain",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: uncertain.clone(),
        },
    )
    .err();
    match result(store, request, false, uncertain, None, retention_failure) {
        Ok(receipt) => primary.with_receipt(receipt),
        Err(secondary) => {
            let native_result = secondary.native_result.clone();
            let mut failure = primary.with_secondary(secondary);
            failure.native_result = native_result;
            failure
        }
    }
}

fn completion_failure(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    intent: OwnerOperationReceipt,
    owner: OwnerOperationReceipt,
    mut failure: AttemptOwnerError,
) -> AttemptOwnerError {
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
        .expect_err("actual failed completion retains its original cause")
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
        return result(&store, &request, true, previous.clone(), owner, None);
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
            return Err(invocation_failure(&mut store, &request, intent, failure));
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
        Ok(transport) => result(&store, &request, false, transport, Some(owner), None),
        Err(failure) => Err(completion_failure(
            &mut store, &request, intent, owner, failure,
        )),
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod publication_tests {
    use super::*;
    use crate::attempt_learning::publication_tests::{native_observation, native_retention_source};
    use std::cell::Cell;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::rc::Rc;

    #[test]
    fn actual_owner_retention_keeps_apply_cause_when_followup_native_read_fails() {
        use crate::attempt_runtime::FactoryAttemptError;
        use crate::project_development_store::ProjectDevelopmentStoreError;
        use std::cell::RefCell;
        use std::fs::File;
        use std::io::{self, Read};
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;

        fn actual_store_io(failure: &AttemptOwnerError) -> &io::Error {
            let mapped = failure
                .source()
                .unwrap()
                .downcast_ref::<FactoryAttemptError>()
                .unwrap();
            let FactoryAttemptError::NativeStore(retained) = mapped else {
                panic!("original native store failure was reduced to a message");
            };
            // Preserve the existing public string while retaining the original
            // typed object without broadly formatting its private body.
            assert_eq!(
                mapped.to_string(),
                format!(
                    "Factory attempt error: InvalidOperation({:?})",
                    retained.original_native_store_error().to_string()
                )
            );
            match retained.original_native_store_error() {
                ProjectDevelopmentStoreError::Io(cause) => cause,
                _ => panic!("the real filesystem failure is not an IO cause"),
            }
        }

        let (root, mut store, base) = native_retention_source();
        let path = root.path().join("state.json");
        let physical = path.canonicalize().unwrap();
        let held_path = root.path().join("held-original-state.json");
        assert!(!held_path.exists());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut held_original = File::open(&path).unwrap();
        let original_metadata = held_original.metadata().unwrap();
        let original_identity = (original_metadata.dev(), original_metadata.ino());
        assert_eq!(original_metadata.nlink(), 1);
        let parent_metadata = std::fs::metadata(path.parent().unwrap()).unwrap();
        let parent_identity = (parent_metadata.dev(), parent_metadata.ino());
        assert_eq!(parent_identity, {
            let metadata = std::fs::metadata(physical.parent().unwrap()).unwrap();
            (metadata.dev(), metadata.ino())
        });
        let before_bytes = std::fs::read(&path).unwrap();
        let before_revision = store.reading().unwrap().revision;
        let request = FactoryAttemptOwnerRequest {
            contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
            request_ref: base.request_ref,
            projection_ref: base.projection_ref,
            caller: base.caller,
            run_ref: base.run_ref,
            expected_revision: before_revision,
            authority: base.authority,
            attempt_ref: base.attempt_ref,
            execution_ref: "execution:actual-double-retention-failure".into(),
            invocation: NativeOwnerInvocation::AikitEncounter {
                binary: std::env::current_exe().unwrap(),
                cwd: root.path().to_path_buf(),
                transport: None,
                contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                request: Value::Null,
            },
        };
        // This is a real current Factory reading. No external owner is invoked
        // and no successful AIKit/provider response is supplied.
        let observed_owner = native_observation(&store);
        let failed_receipt_ref = observed_owner.receipt_ref.clone();
        let seen = Rc::new(Cell::new(0));
        let staged = Rc::new(RefCell::new(None::<(PathBuf, (u64, u64))>));
        let observer_seen = seen.clone();
        let observer_staged = staged.clone();
        let observer_path = path.clone();
        let observer_held_path = held_path.clone();
        crate::native_file_transaction::observe_next_privacy_copy(move |stage| {
            observer_seen.set(observer_seen.get() + 1);
            assert_eq!(observer_seen.get(), 1);
            let parent = std::fs::metadata(stage.parent().unwrap()).unwrap();
            assert_eq!((parent.dev(), parent.ino()), parent_identity);
            let source = std::fs::symlink_metadata(&observer_path).unwrap();
            assert!(source.is_file());
            assert_eq!((source.dev(), source.ino()), original_identity);
            let stage_metadata = std::fs::symlink_metadata(stage).unwrap();
            assert!(stage_metadata.is_file());
            assert_eq!(stage_metadata.nlink(), 1);
            assert_eq!(stage_metadata.uid(), source.uid());
            assert_eq!(stage_metadata.mode() & 0o7777, 0o600);
            assert!(std::fs::read(stage).unwrap().is_empty());
            *observer_staged.borrow_mut() = Some((
                stage.to_path_buf(),
                (stage_metadata.dev(), stage_metadata.ino()),
            ));
            // Actual stage privacy drift is detected before candidate bytes.
            std::fs::set_permissions(stage, std::fs::Permissions::from_mode(0o777)).unwrap();
            // Preserve the exact original inode/bytes but make a later owner
            // no-follow read refuse this genuinely replaced source leaf.
            std::fs::rename(&observer_path, &observer_held_path).unwrap();
            symlink(&observer_held_path, &observer_path).unwrap();
        });
        let failure = retain(
            &mut store,
            &request,
            "actual-double-native-failure",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: observed_owner,
            },
        )
        .unwrap_err();
        assert_eq!(seen.get(), 1, "unreadable basis cannot authorize a retry");
        assert!(failure.native_result().is_none());
        assert!(crate::native_publication_uncertainty(&failure).is_none());
        let primary = failure.primary_retention_error().unwrap();
        let secondary = failure.secondary_retention_error().unwrap();
        let first_io = actual_store_io(primary);
        assert_eq!(first_io.kind(), io::ErrorKind::InvalidData);
        assert_eq!(first_io.raw_os_error(), None);
        assert!(first_io
            .to_string()
            .starts_with("native stage privacy changed before candidate bytes"));
        let later_io = actual_store_io(secondary);
        assert_eq!(later_io.raw_os_error(), Some(libc::ELOOP));
        assert_ne!(primary.to_string(), secondary.to_string());
        assert_eq!(failure.to_string(), primary.to_string());
        assert_eq!(
            failure.source().unwrap().source().unwrap().to_string(),
            secondary.to_string(),
            "generic chain retains the actual follow-up failure"
        );
        let primary_mapped = primary
            .source()
            .unwrap()
            .downcast_ref::<FactoryAttemptError>()
            .unwrap();
        assert!(!format!("{primary_mapped:?}").contains("native stage privacy changed"));
        assert!(!format!("{failure:?}").contains("native stage privacy changed"));
        let (stage, stage_identity) = staged.borrow().as_ref().unwrap().clone();
        let metadata = std::fs::symlink_metadata(&stage).unwrap();
        assert!(metadata.is_file());
        assert_eq!((metadata.dev(), metadata.ino()), stage_identity);
        assert!(std::fs::read(&stage).unwrap().is_empty());
        let mut held_bytes = Vec::new();
        held_original.read_to_end(&mut held_bytes).unwrap();
        assert_eq!(held_bytes, before_bytes);
        let retained_metadata = std::fs::symlink_metadata(&held_path).unwrap();
        assert_eq!(
            (retained_metadata.dev(), retained_metadata.ino()),
            original_identity
        );
        assert_eq!(retained_metadata.mode() & 0o7777, 0o600);
        let substituted = std::fs::symlink_metadata(&path).unwrap();
        assert!(substituted.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&path).unwrap(), held_path);
        // Restore only the named owned symlink and preserved original inode.
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&held_path, &path).unwrap();
        let restored = std::fs::symlink_metadata(&path).unwrap();
        assert_eq!((restored.dev(), restored.ino()), original_identity);
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
        let current = store.reading().unwrap();
        assert_eq!(current.revision, before_revision);
        let attempt = record(&current, &request.attempt_ref).unwrap();
        assert!(!attempt
            .dispatch
            .iter()
            .chain(&attempt.observations)
            .any(|receipt| receipt.receipt_ref == failed_receipt_ref));
        assert!(!held_path.exists());
        assert!(std::fs::read(&stage).unwrap().is_empty());

        // Exercise the other changed mapping with actual owned-file corruption
        // and the real store decoder, then restore before inspecting the error.
        std::fs::write(&path, b"{").unwrap();
        let json_failure = store.reading().unwrap_err();
        std::fs::write(&path, &before_bytes).unwrap();
        let FactoryAttemptError::NativeStore(retained_json) = &json_failure else {
            panic!("actual store JSON cause was reduced to a message");
        };
        let ProjectDevelopmentStoreError::Json(original_json) =
            retained_json.original_native_store_error()
        else {
            panic!("actual corrupt source did not return its JSON cause");
        };
        assert!(original_json.is_eof());
        assert_eq!(
            json_failure.to_string(),
            format!(
                "Factory attempt error: InvalidOperation({:?})",
                retained_json.original_native_store_error().to_string()
            )
        );
        assert!(!format!("{json_failure:?}").contains(original_json.to_string().as_str()));
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
        assert_eq!(store.reading().unwrap().revision, before_revision);
        let final_source = std::fs::symlink_metadata(&path).unwrap();
        assert_eq!((final_source.dev(), final_source.ino()), original_identity);
    }

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
            let lexical_for_observer = path.clone();
            crate::native_file_transaction::observe_next_publication(move |published| {
                assert_eq!(
                    published, lexical_for_observer,
                    "publisher retains its raw public address"
                );
                // Establish the actual candidate inode through both Mac path
                // spellings while it exists, before the adversarial move.
                assert_eq!(published.canonicalize().unwrap(), physical_for_observer);
                let published_file = std::fs::File::open(published).unwrap();
                let physical_file = std::fs::File::open(&physical_for_observer).unwrap();
                let published_identity = published_file.metadata().unwrap();
                let physical_identity = physical_file.metadata().unwrap();
                assert_eq!(
                    (published_identity.dev(), published_identity.ino()),
                    (physical_identity.dev(), physical_identity.ino())
                );
                seen_for_observer.set(seen_for_observer.get() + 1);
                if move_after_publication {
                    std::fs::rename(published, &retained_for_observer).unwrap();
                } else {
                    std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                }
            });
            let returned_error =
                completion_failure(&mut store, &request, intent, owner.clone(), primary);
            let returned = returned_error.native_result().unwrap();
            assert!(crate::native_publication_uncertainty(&returned_error).is_some(),
                "generic caller must still find typed publication uncertainty after failed readback");
            assert_eq!(
                returned_error
                    .primary_retention_error()
                    .unwrap()
                    .to_string(),
                primary_message
            );
            let secondary = returned_error.secondary_retention_error().unwrap();
            assert!(
                secondary.source().is_some(),
                "actual distinct native cause remains retained"
            );
            if move_after_publication {
                assert!(
                    crate::native_publication_uncertainty(secondary).is_none(),
                    "later failed readback is not itself fabricated publication uncertainty"
                );
                assert!(crate::native_publication_uncertainty(
                    returned_error.primary_retention_error().unwrap()
                )
                .is_some());
            } else {
                assert!(crate::native_publication_uncertainty(secondary).is_some());
            }
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

// Opt-in qualification of real transport/held pipes and this persisted failure
// consumer. Canonical fixture model labels are controlled Factory admission
// metadata, never a realised provider, AIKit reply or worker completion.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod native_failure_tests {
    use super::*;
    use crate::attempt_learning::publication_tests::native_retention_source;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixDatagram;
    use std::time::{Duration, Instant};

    const CANARY: &[u8] = b"PRIVATE-OWNER-PIPE-CANARY";

    fn fixture() -> (
        tempfile::TempDir,
        FileAttemptStore,
        FactoryAttemptOwnerRequest,
    ) {
        let root = std::env::var_os("FACTORY_NATIVE_EVIDENCE_DIR")
            .expect("explicit admitted evidence root is required");
        let root = Path::new(&root);
        assert!(root.is_absolute() && root.is_dir());
        assert_eq!(root.canonicalize().unwrap(), root);
        let scratch = std::env::temp_dir().canonicalize().unwrap();
        assert!(
            scratch.starts_with(root),
            "TMPDIR must be the admitted evidence root/subtree"
        );
        let (directory, store, base) = native_retention_source();
        let reading = store.reading().unwrap();
        let attempt = record(&reading, &base.attempt_ref).unwrap();
        let request = FactoryAttemptOwnerRequest {
            contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
            request_ref: format!("owner-native:{}", base.request_ref),
            projection_ref: base.projection_ref,
            caller: base.caller.clone(),
            run_ref: base.run_ref,
            expected_revision: reading.revision,
            authority: base.authority,
            attempt_ref: base.attempt_ref,
            execution_ref: "execution:actual-native-failure-consumer".into(),
            invocation: NativeOwnerInvocation::AikitEncounter {
                binary: directory.path().join("actually-absent-owner"),
                cwd: directory.path().to_path_buf(),
                transport: None,
                contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                request: json!({"action":"send", "agent_session":attempt.disposition.body.agent_session_ref,
                    "turn":{"delivery_ref":"delivery:actual-native-failure", "sender":base.caller.caller_ref,
                        "expected_binding_revision":"binding:controlled-native-fixture",
                        "packet":{"audience":[attempt.disposition.participant.agent_ref],
                            "text":"Controlled OS transport failure qualification; no model or worker Return."}}}),
            },
        };
        (directory, store, request)
    }

    fn retain_intent(
        store: &mut FileAttemptStore,
        request: &FactoryAttemptOwnerRequest,
    ) -> OwnerOperationReceipt {
        let NativeOwnerInvocation::AikitEncounter {
            binary,
            cwd,
            contract_revision,
            request: packet,
            ..
        } = &request.invocation
        else {
            unreachable!()
        };
        let digest = blake3::hash(&serde_json::to_vec(request).unwrap())
            .to_hex()
            .to_string();
        let intent = stamp(
            OwnerOperationReceipt {
                owner_ref: "factory".into(),
                contract: TRANSPORT_OBSERVATION.into(),
                operation_ref: intent_ref(request),
                receipt_ref: String::new(),
                source_revision: format!("factory-state:{}", request.expected_revision),
                phase: OwnerOperationPhase::Dispatching,
                evidence_refs: BTreeSet::new(),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"requestDigest":digest,"agentSession":packet["agent_session"],
                "deliveryRef":packet["turn"]["delivery_ref"],"action":"send","executionRef":request.execution_ref,
                "transport":{"binary":binary,"cwd":cwd,"contractRevision":contract_revision},"taskAdmission":null}),
            },
            OwnerOperationPhase::Dispatching,
            json!({"meaning":"actual controlled intent before real OS transport; no provider success"}),
        );
        store
            .apply(action_request(
                request,
                request.expected_revision,
                "intent",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: intent.clone(),
                },
            ))
            .unwrap();
        intent
    }

    fn timed_out_real_pipe_owner(directory: &Path) -> NativeOwnerError {
        // Two actual SCM_RIGHTS pipe copies remain held by this test until the
        // owned native client has timed out/reaped. No escaped child is spawned.
        let python = std::env::var_os("FACTORY_NATIVE_PYTHON_BIN")
            .expect("qualified actual Python pin required");
        let python = Path::new(&python);
        assert!(python.is_absolute() && python.is_file());
        let pin =
            std::env::var("FACTORY_NATIVE_PYTHON_SHA256").expect("qualified Python SHA required");
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(python).unwrap())),
            pin
        );
        // A native socket pair avoids the platform UDS path-length limit in
        // real allocated ProjectCentral scratch. Only this owned test socket
        // is deliberately inherited by the actual child; qualification is serial.
        let (socket, child_socket) = UnixDatagram::pair().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let old_flags = unsafe { libc::fcntl(child_socket.as_raw_fd(), libc::F_GETFD) };
        assert!(old_flags >= 0);
        assert_eq!(
            unsafe {
                libc::fcntl(
                    child_socket.as_raw_fd(),
                    libc::F_SETFD,
                    old_flags & !libc::FD_CLOEXEC,
                )
            },
            0
        );

        let receiver = std::thread::Builder::new()
            .name("owned-native-pipe-transfer".into())
            .spawn(move || {
                let mut marker = [0u8; 1];
                let mut control = [0usize; 32]; // aligned, bounded ancillary buffer
                let mut iov = libc::iovec {
                    iov_base: marker.as_mut_ptr().cast(),
                    iov_len: marker.len(),
                };
                let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
                message.msg_iov = &mut iov;
                message.msg_iovlen = 1;
                message.msg_control = control.as_mut_ptr().cast();
                message.msg_controllen = std::mem::size_of_val(&control) as _;
                assert_eq!(
                    unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, 0) },
                    1,
                    "real pipe transfer failed: {}",
                    io::Error::last_os_error()
                );
                assert_eq!(marker, [b'P']);
                assert_eq!(message.msg_flags & libc::MSG_CTRUNC, 0);
                let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
                assert!(!header.is_null());
                assert_eq!(unsafe { (*header).cmsg_level }, libc::SOL_SOCKET);
                assert_eq!(unsafe { (*header).cmsg_type }, libc::SCM_RIGHTS);
                assert_eq!(unsafe { (*header).cmsg_len } as usize, unsafe {
                    libc::CMSG_LEN((2 * std::mem::size_of::<libc::c_int>()) as _)
                }
                    as usize);
                let fds = unsafe {
                    std::slice::from_raw_parts(libc::CMSG_DATA(header).cast::<libc::c_int>(), 2)
                };
                assert!(fds[0] >= 0 && fds[1] >= 0 && fds[0] != fds[1]);
                // The returned native handles remain owned in the finished thread's
                // result until join; this is a genuine incomplete-EOF condition.
                unsafe { [OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])] }
            })
            .unwrap();
        let script = directory.join("actual-pipe-owner.py");
        assert!(
            !python.to_string_lossy().chars().any(char::is_whitespace),
            "actual shebang interpreter path must be unambiguous"
        );
        let source = format!("#!{}\nimport array,os,signal,socket\nos.write(1,{:?}.encode())\nos.write(2,{:?}.encode())\ns=socket.socket(fileno={})\ns.sendmsg([b'P'],[(socket.SOL_SOCKET,socket.SCM_RIGHTS,array.array('i',[1,2]))])\ns.close()\nwhile True: signal.pause()\n",
            python.display(), std::str::from_utf8(CANARY).unwrap(), std::str::from_utf8(CANARY).unwrap(), child_socket.as_raw_fd());
        fs::write(&script, source).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let invocation = NativeOwnerInvocation::AikitEncounter {
            binary: script,
            cwd: directory.to_path_buf(),
            transport: None,
            contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
            request: json!({"action":"delivery","agent_session":"session:test","delivery_ref":"delivery:actual-native-failure"}),
        };
        let native_result = invoke_native_owner_bounded(&invocation, 1_500);
        drop(child_socket); // close this owned inherited endpoint before any later child
        let deadline = Instant::now() + Duration::from_secs(4);
        while !receiver.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            receiver.is_finished(),
            "owned finite FD receiver did not finish; cleanup uncertain"
        );
        let held_pipes = receiver.join().expect("actual FD receiver failed");
        let failure = native_result.unwrap_err();
        let NativeOwnerError::Spawn { error, .. } = &failure else {
            panic!("native capture cause missing");
        };
        let capture =
            crate::native_process::capture_failure(error).expect("actual same typed failure");
        assert!(capture.process_started() && capture.timed_out() && capture.client_reaped());
        assert!(!capture.stdout_eof() && !capture.stderr_eof());
        assert!(capture
            .stdout()
            .windows(CANARY.len())
            .any(|window| window == CANARY));
        assert!(capture
            .stderr()
            .windows(CANARY.len())
            .any(|window| window == CANARY));
        drop(held_pipes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(python).unwrap())),
            pin
        );
        failure
    }

    fn assert_private(failure: &AttemptOwnerError) {
        let canary = std::str::from_utf8(CANARY).unwrap();
        assert!(!format!("{failure}").contains(canary));
        assert!(!format!("{failure:?}").contains(canary));
        let receipt = failure.native_result().unwrap();
        assert!(!format!("{receipt:?}").contains(canary));
        assert!(!serde_json::to_string(receipt).unwrap().contains(canary));
        assert!(receipt.needs_reconciliation && receipt.owner_receipt.is_none());
    }

    #[test]
    #[ignore = "requires admitted private TMPDIR/evidence and pinned actual Python; native OS only"]
    fn actual_timeout_incomplete_pipes_retain_original_private_capture() {
        let (directory, mut store, request) = fixture();
        let intent = retain_intent(&mut store, &request);
        let failure = timed_out_real_pipe_owner(directory.path());
        let error = invocation_failure(&mut store, &request, intent, failure);
        assert_private(&error);
        let NativeOwnerError::Spawn { error: io, .. } =
            error.original_native_owner_error().unwrap()
        else {
            panic!("original cause erased");
        };
        let capture = crate::native_process::capture_failure(io).unwrap();
        assert!(capture.timed_out() && !capture.stdout_eof() && capture.client_reaped());
        let state = directory.path().join("state.json");
        let retained = fs::read(&state).unwrap();
        let historical = execute_attempt_owner_action(&state, request).unwrap();
        assert!(historical.replayed && historical.needs_reconciliation);
        assert_eq!(
            fs::read(&state).unwrap(),
            retained,
            "history read must neither resend nor settle"
        );
    }

    #[test]
    #[ignore = "requires admitted private TMPDIR/evidence; actual public dispatcher/OS missing executable"]
    fn actual_missing_executable_is_typed_failure_with_single_intent_and_no_resend() {
        let (directory, store, request) = fixture();
        let state = directory.path().join("state.json");
        let before = store.reading().unwrap().revision;
        let error = execute_attempt_owner_action(&state, request.clone()).unwrap_err();
        assert_private(&error);
        let NativeOwnerError::Spawn { error: io, .. } =
            error.original_native_owner_error().unwrap()
        else {
            panic!("original native Spawn cause lost");
        };
        assert_eq!(io.kind(), io::ErrorKind::NotFound);
        let capture = crate::native_process::capture_failure(io).unwrap();
        assert!(!capture.process_started());
        assert!(capture.status().is_none());
        assert_eq!(capture.cause().raw_os_error(), Some(libc::ENOENT));
        let reading = store.reading().unwrap();
        assert_eq!(
            reading.revision,
            before + 2,
            "one intent and one actual uncertainty; no retry"
        );
        assert_eq!(
            latest_transports(record(&reading, &request.attempt_ref).unwrap()).len(),
            1
        );
        let retained = fs::read(&state).unwrap();
        let historical = execute_attempt_owner_action(&state, request.clone()).unwrap();
        assert!(historical.replayed && historical.needs_reconciliation);
        assert_eq!(fs::read(&state).unwrap(), retained);
        // A second actual isolated canonical attempt exercises the public CLI
        // consumer's first failed invocation, not a pre-dispatch refusal.
        let (cli_directory, cli_store, cli_request) = fixture();
        let cli_state = cli_directory.path().join("state.json");
        let args = vec![
            cli_state.to_string_lossy().into_owned(),
            "-".into(),
            "--json".into(),
        ];
        let failure = crate::attempt_owner_cli::execute_attempt_owner_cli(
            &args,
            Some(&serde_json::to_string(&cli_request).unwrap()),
        )
        .unwrap_err();
        let owner_error = failure
            .source()
            .unwrap()
            .downcast_ref::<AttemptOwnerError>()
            .unwrap();
        assert!(owner_error.original_native_owner_error().is_some());
        assert_eq!(
            failure.native_owner_failure_result().unwrap(),
            &serde_json::to_value(owner_error.native_result().unwrap()).unwrap()
        );
        assert!(!format!("{failure:?}").contains(std::str::from_utf8(CANARY).unwrap()));
        assert_eq!(
            cli_store.reading().unwrap().revision,
            cli_request.expected_revision + 2
        );
        assert_eq!(fs::read(&state).unwrap(), retained);
    }

    #[test]
    #[ignore = "requires admitted private TMPDIR/evidence and pinned actual Python; actual publisher observer"]
    fn actual_capture_and_post_publish_uncertainty_remain_distinct_without_compensation() {
        let (directory, mut store, request) = fixture();
        let intent = retain_intent(&mut store, &request);
        let before = store.reading().unwrap().revision;
        let native = timed_out_real_pipe_owner(directory.path());
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = count.clone();
        crate::native_file_transaction::observe_next_publication(move |path| {
            observed.set(observed.get() + 1);
            fs::set_permissions(path, fs::Permissions::from_mode(0o777)).unwrap();
        });
        let error = invocation_failure(&mut store, &request, intent, native);
        assert_private(&error);
        assert_eq!(
            count.get(),
            1,
            "already-published uncertainty forbids any compensation/retry"
        );
        let NativeOwnerError::Spawn { error: io, .. } =
            error.original_native_owner_error().unwrap()
        else {
            panic!("original capture lost");
        };
        assert!(crate::native_process::capture_failure(io)
            .unwrap()
            .timed_out());
        assert!(
            crate::native_publication_uncertainty(error.secondary_retention_error().unwrap())
                .is_some()
        );
        assert!(crate::native_publication_uncertainty(&error).is_some());
        assert!(error
            .native_result()
            .unwrap()
            .publication_uncertainty
            .is_some());
        let current =
            FileAttemptStore::open_run(directory.path().join("state.json"), request.run_ref)
                .unwrap()
                .reading()
                .unwrap();
        assert_eq!(current.revision, before + 1);
        assert!(record(&current, &request.attempt_ref)
            .unwrap()
            .readable_return
            .is_none());
    }

    #[test]
    #[ignore = "requires admitted private TMPDIR/evidence; actual Factory source observation/schema only"]
    fn actual_source_receipt_projection_and_historical_replay_keep_public_schema() {
        let (directory, mut store, request) = fixture();
        let mut intent = retain_intent(&mut store, &request);
        let owner = crate::attempt_learning::publication_tests::native_observation(&store);
        intent = stamp(
            intent,
            OwnerOperationPhase::Observed,
            json!({"ownerReceipt":owner}),
        );
        retain(
            &mut store,
            &request,
            "actual-source-observation",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: intent.clone(),
            },
        )
        .unwrap();
        let receipt = result(&store, &request, false, intent, Some(owner), None).unwrap();
        let value = serde_json::to_value(&receipt).unwrap();
        let keys = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from([
                "contract",
                "requestRef",
                "runRef",
                "attemptRef",
                "replayed",
                "needsReconciliation",
                "transportObservation",
                "ownerReceipt",
                "retentionError",
                "reading"
            ])
        );
        assert!(!receipt.needs_reconciliation);
        let decoded: FactoryAttemptOwnerReceipt = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        let state = directory.path().join("state.json");
        let bytes = fs::read(&state).unwrap();
        let args = vec![state.to_string_lossy().into_owned(), "-".into()];
        let text = crate::attempt_owner_cli::execute_attempt_owner_cli(
            &args,
            Some(&serde_json::to_string(&request).unwrap()),
        )
        .unwrap();
        assert_eq!(text, format!("{}\nRun: {}\nAttempt: {}\nRequest: {}\nReplayed: true\nNeeds reconciliation: false\nOwner phase: Observed",
            FACTORY_ATTEMPT_OWNER_RECEIPT, request.run_ref, request.attempt_ref, request.request_ref));
        assert_eq!(fs::read(&state).unwrap(), bytes);
        // This proves Factory source projection/replay only. No actual AIKit
        // successful delivery, realised model or worker Return is asserted.
    }
}
