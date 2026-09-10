//! Factory-owned attempt application values. Foreign facts remain revisioned
//! owner evidence; an arrangement or a delivery ACK is never completed work.

use crate::action_projection::FactoryActionProjectionRequest;
use crate::core::identity::Revision;
use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::developmental_read::FactoryRevisionedOwnerRef;
use crate::execution_intelligence::ExecutionDisposition;
use crate::journey::JourneyRef;
use crate::orchestration::{OrchestrationSnapshot, ReturnedArtifact, SynthesisRecord};
use crate::workflow::WorkflowSourceProvenance;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ATTEMPT_ACTION: &str = "factory.attempt-action/v1";
pub const ATTEMPT_READING: &str = "factory.attempt-reading/v1";
pub const ATTEMPT_FIELD: &str = "factory.attempt-field/v1";
pub const ATTEMPT_ACTION_REF: &str = "action/factory/attempt";
pub const ATTEMPT_CAPABILITY: &str = "capability/factory/attempt";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptActionRequest {
    pub contract: String,
    /// Existing caller, Action, subject, Run and authority contract, unchanged.
    pub projection: FactoryActionProjectionRequest,
    pub expected_state_revision: Revision,
    pub expected_run_revision: Revision,
    pub writer_owner: String,
    pub writer_epoch: u64,
    pub operation: FactoryAttemptOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum FactoryAttemptOperation {
    Initialise {
        journey_ref: JourneyRef,
        workflow_source: WorkflowSourceProvenance,
        workflow_key: String,
    },
    Begin {
        /// Two or more launches require an explicit source-proven independent fork.
        launches: Vec<AttemptLaunch>,
    },
    Detach { attempt_ref: String },
    RequestCancellation { attempt_ref: String },
    /// Durable reservation. Only the native dispatch application may invoke it.
    Dispatch { attempt_ref: String },
    Collect {
        attempt_ref: String,
        observation: AttemptObservation,
        artifacts: Vec<ReturnedArtifact>,
    },
    Reconcile {
        attempt_ref: String,
        reconciliation: AttemptReconciliation,
    },
    Retry {
        previous_attempt_ref: String,
        launch: Box<AttemptLaunch>,
        /// Required for a changed explicit model/provider pin, never inferred.
        model_change_authority: Option<FactoryRevisionedOwnerRef>,
    },
    Verify {
        attempt_ref: String,
        verification: Box<IndependentAttemptVerification>,
    },
    Return {
        attempt_ref: String,
        result: HumanAttemptReturn,
    },
    Trace {
        attempt_ref: String,
        trace: AttemptTrace,
    },
    RevokeRetry { grant_ref: String },
    Synthesize {
        verification: Box<IndependentAttemptVerification>,
        record: SynthesisRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptLaunch {
    /// Factory reservation identity, not an Actuation Execution or AgentSession.
    pub attempt_ref: String,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub arrangement: SituatedExecutionArrangement,
}

/// The established EI decision is composed with exact operative requirements;
/// no second model registry, Agent identity, Context or material owner is made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SituatedExecutionArrangement {
    pub disposition: ExecutionDisposition,
    pub subject_ref: String,
    pub basis_revision: String,
    pub agent_ref: String,
    pub agency: FactoryRevisionedOwnerRef,
    pub authority: FactoryRevisionedOwnerRef,
    pub context: Vec<FactoryRevisionedOwnerRef>,
    pub praxis: Vec<FactoryRevisionedOwnerRef>,
    pub body: FactoryRevisionedOwnerRef,
    pub model_variant: Option<String>,
    pub route: FactoryRevisionedOwnerRef,
    pub session: AddressedSession,
    pub material: Vec<FactoryRevisionedOwnerRef>,
    pub placement: PlacementRequirements,
    pub permitted_effects: BTreeSet<String>,
    pub verification_obligations: BTreeSet<String>,
    pub budget: AttemptBudget,
    pub stop_conditions: String,
    pub rationale: String,
    pub rationale_evidence: Vec<FactoryRevisionedOwnerRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddressedSession {
    pub agent_session_ref: String,
    pub session_space_ref: String,
    pub binding_revision: String,
    pub binding_evidence: FactoryRevisionedOwnerRef,
    pub harness_ref: String,
    pub sender_ref: String,
    pub disclosed_source_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlacementRequirements {
    pub policy: FactoryRevisionedOwnerRef,
    pub now: Option<FactoryRevisionedOwnerRef>,
    pub writable_source_refs: BTreeSet<String>,
    pub protected_source_refs: BTreeSet<String>,
    pub required_coverage: BTreeSet<String>,
    /// A preparation receipt alone does not establish effective confinement.
    pub effective_boundary: Option<FactoryRevisionedOwnerRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptBudget {
    pub grant_ref: String,
    /// Total admissions, including the first attempt; never reset on recovery.
    pub maximum_attempts: u32,
    pub stop_at_unix_ms: u64,
    /// Bound on the owner CLI call, not a claim of worker-process termination.
    pub owner_call_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptObservation {
    pub observation_ref: String,
    pub source: FactoryRevisionedOwnerRef,
    pub summary: String,
    pub partial_effect_refs: BTreeSet<String>,
    pub unknown_effect_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptReconciliation {
    pub reconciliation_ref: String,
    pub current_source: FactoryRevisionedOwnerRef,
    pub material_evidence: Vec<FactoryRevisionedOwnerRef>,
    pub retained_effect_refs: BTreeSet<String>,
    pub resolved_unknown_refs: BTreeSet<String>,
    pub unresolved_effect_refs: BTreeSet<String>,
    /// Exact Actuation/Workcell evidence, not accepted cancellation or a timeout.
    pub quiescence: Option<FactoryRevisionedOwnerRef>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndependentAttemptVerification {
    pub verification_ref: String,
    pub verifier_agent_ref: String,
    pub verifier_agency_ref: String,
    pub verifier_execution_ref: String,
    pub disposition: ExecutionDisposition,
    pub review_of: BTreeSet<WorkflowUnitRef>,
    pub artifact_refs: BTreeSet<String>,
    pub satisfied_obligations: BTreeSet<String>,
    pub evidence: Vec<FactoryRevisionedOwnerRef>,
    pub passed: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HumanAttemptReturn {
    pub return_ref: String,
    pub what_changed: String,
    pub remaining_work: Vec<String>,
    pub attention_needed: Vec<String>,
    pub evidence_refs: BTreeSet<String>,
    /// Central receiving is a separate owner operation, not implicit inclusion.
    pub receiving_ref: Option<FactoryRevisionedOwnerRef>,
    pub archive_refs: Vec<FactoryRevisionedOwnerRef>,
    pub regression_observation_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptTrace {
    pub temporal_source_refs: Vec<FactoryRevisionedOwnerRef>,
    pub activity_spans: Vec<OwnerCursorSpan>,
    /// Existing public Factory telemetry identities; not copied model usage.
    pub telemetry_refs: BTreeSet<String>,
    pub archive_refs: Vec<FactoryRevisionedOwnerRef>,
    pub reentry_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerCursorSpan {
    pub source: FactoryRevisionedOwnerRef,
    pub first_cursor: u64,
    pub last_cursor: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryPhase {
    Dispatching,
    Submitted,
    Returned,
    Failed,
    Cancelled,
    Uncertain,
    ReconciledNoReplay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptDelivery {
    pub delivery_ref: String,
    pub phase: DeliveryPhase,
    pub owner_revision: String,
    pub connection_digest: String,
    /// Every received native response is retained, even for uncertain outcomes.
    pub native_responses: Vec<serde_json::Value>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptFacts {
    pub attempt_ref: String,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub arrangement: SituatedExecutionArrangement,
    pub delivery: Option<AttemptDelivery>,
    pub observations: BTreeMap<String, AttemptObservation>,
    pub artifacts: BTreeMap<String, ReturnedArtifact>,
    pub reconciliation: Option<AttemptReconciliation>,
    pub verification: Option<IndependentAttemptVerification>,
    pub human_return: Option<HumanAttemptReturn>,
    pub trace: AttemptTrace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAttemptField {
    pub contract: String,
    pub journey_ref: JourneyRef,
    pub workflow_source: WorkflowSourceProvenance,
    pub workflow_key: String,
    /// No Run copy and no deserialised mutation authority is stored here.
    pub coordinator: OrchestrationSnapshot,
    pub attempts: BTreeMap<String, AttemptFacts>,
    pub applied_actions: BTreeMap<String, AppliedAttemptAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedAttemptAction {
    pub request_digest: String,
    pub receipt: FactoryAttemptActionReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttemptActionStatus {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptActionReceipt {
    pub contract: String,
    pub projection_ref: String,
    pub run_ref: RunRef,
    pub authority_ref: String,
    pub previous_state_revision: Revision,
    pub next_state_revision: Revision,
    pub previous_run_revision: Revision,
    pub next_run_revision: Revision,
    pub status: AttemptActionStatus,
    pub affected_attempt_refs: Vec<String>,
}
