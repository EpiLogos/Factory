//! Durable public operation boundary for commissioned Factory execution attempts.
//!
//! This module continues the existing [`crate::orchestration`] coordinator; it
//! does not introduce another scheduler. The file envelope retains the exact
//! Factory Run and authored Workflow source basis needed to reopen the existing
//! serialisable `OrchestrationSnapshot` in another process. Every open recompiles
//! the authored source and restores the snapshot against the retained Run, so a
//! stale/mutated source, replenished retry grant, lost writer reservation or
//! topology drift is refused rather than silently normalised.
//!
//! Foreign-owner receipts are evidence about execution actuality. In particular,
//! an AIKit native delivery phase of `returned` means the provider turn ended; it
//! is not by itself Factory verification, artifact Return, human inclusion or
//! Recognition. Those determinations stay separate below.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::core::run::{Run, RunRef, WorkflowUnitRef};
use crate::execution_intelligence::ExecutionDisposition;
use crate::orchestration::{
    ExecutableOrchestration, ExecutionLaunch, LegRecord, OrchestrationError, OrchestrationSnapshot,
    RetryGrant, ReturnedArtifact,
};
use crate::workflow::{compile_workflow, CompiledWorkflowUnit, WorkflowSource};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const FACTORY_ATTEMPT_STATE: &str = "factory.attempt-state/v1";
pub const FACTORY_ATTEMPT_ACTION: &str = "factory.attempt-action/v1";
pub const FACTORY_ATTEMPT_READING: &str = "factory.attempt-reading/v1";
pub const FACTORY_ATTEMPT_CAPABILITY_REF: &str = "capability/factory/operate-attempt";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptSeed {
    pub run: Run,
    pub workflow_source: WorkflowSource,
}

/// The actual participant admitted for this attempt. A durable Central profile
/// is optional; no profile is minted for a transient/current-World participant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SituatedParticipant {
    pub agent_ref: String,
    pub agency_ref: String,
    pub world_binding_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
    pub source_ref: String,
    pub source_revision: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionBody {
    pub model_ref: String,
    pub provider_ref: String,
    pub route_ref: String,
    pub harness_ref: String,
    pub harness_composition_ref: String,
    pub agent_session_ref: String,
    pub session_space_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_world_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workcell_ref: Option<String>,
}

/// Required protection and the actually realised coverage are separate so a
/// retry, alternate harness or relocation cannot silently weaken the task law.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlacementProtection {
    pub now_ref: String,
    pub now_path: String,
    pub policy_ref: String,
    pub policy_revision: String,
    pub authority_ref: String,
    #[serde(default)]
    pub writable_paths: BTreeSet<String>,
    #[serde(default)]
    pub protected_paths: BTreeSet<String>,
    #[serde(default)]
    pub required_coverage: BTreeSet<String>,
    #[serde(default)]
    pub effective_coverage: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_boundary_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_receipt_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_ceiling_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_preference_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_grant_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_attempts: Option<u32>,
}

/// Factory's complete situated execution determination. `selection` remains the
/// existing Execution Intelligence decision; this structure records what was
/// actually arranged around that decision for one concrete attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SituatedExecutionDisposition {
    pub selection: ExecutionDisposition,
    pub participant: SituatedParticipant,
    #[serde(default)]
    pub context_refs: BTreeSet<String>,
    #[serde(default)]
    pub praxis_refs: BTreeSet<String>,
    #[serde(default)]
    pub capability_refs: BTreeSet<String>,
    pub body: ExecutionBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementProtection>,
    pub permitted_effects: BTreeSet<String>,
    pub verification_obligations: BTreeSet<String>,
    pub return_address: String,
    pub stop_conditions: String,
    pub escalation_conditions: String,
    pub budget: ExecutionBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerOperationPhase {
    Dispatching,
    Submitted,
    Returned,
    Failed,
    Cancelled,
    Uncertain,
    ReconciledNoReplay,
    Observed,
    Recovered,
    Released,
}

/// Opaque evidence returned by a native owner operation. Factory retains the
/// payload and exact source revision, but interprets only the explicit phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerOperationReceipt {
    pub owner_ref: String,
    pub contract: String,
    pub operation_ref: String,
    pub receipt_ref: String,
    pub source_revision: String,
    pub phase: OwnerOperationPhase,
    #[serde(default)]
    pub evidence_refs: BTreeSet<String>,
    #[serde(default)]
    pub partial_effect_refs: BTreeSet<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationOutcome {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReceipt {
    pub verification_ref: String,
    pub owner_ref: String,
    pub source_revision: String,
    pub outcome: VerificationOutcome,
    #[serde(default)]
    pub obligations: BTreeSet<String>,
    #[serde(default)]
    pub evidence_refs: BTreeSet<String>,
}

/// Immutable additive correlation fact used by #222. This is intentionally a
/// heterogeneous fact ledger rather than a flattened "current context" object:
/// temporal/NOW/material/source changes remain visible as history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptTrackingFact {
    pub fact_ref: String,
    pub kind: String,
    pub owner_ref: String,
    pub subject_ref: String,
    pub source_revision: String,
    #[serde(default)]
    pub evidence_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReresolutionRecord {
    pub resolution_ref: String,
    pub reason: String,
    pub source_revision: String,
    #[serde(default)]
    pub evidence_refs: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_now_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_material_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_harness_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadableReturn {
    pub return_ref: String,
    pub summary: String,
    #[serde(default)]
    pub artifact_refs: BTreeSet<String>,
    #[serde(default)]
    pub evidence_refs: BTreeSet<String>,
    /// Central #152 receipt when that owner operation has actually occurred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiving_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiving_source_revision: Option<String>,
    #[serde(default)]
    pub archive_refs: BTreeSet<String>,
    #[serde(default)]
    pub regression_observation_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptRecord {
    pub attempt_ref: String,
    pub task_ref: String,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub reserved_execution_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_ref: Option<String>,
    pub disposition: SituatedExecutionDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<OwnerOperationReceipt>,
    #[serde(default)]
    pub observations: Vec<OwnerOperationReceipt>,
    #[serde(default)]
    pub verifications: Vec<VerificationReceipt>,
    #[serde(default)]
    pub tracking: Vec<AttemptTrackingFact>,
    #[serde(default)]
    pub reresolutions: Vec<ReresolutionRecord>,
    #[serde(default)]
    pub failure_evidence_refs: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readable_return: Option<ReadableReturn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredAttemptState {
    schema: String,
    revision: u64,
    run: Run,
    workflow_source: WorkflowSource,
    snapshot: OrchestrationSnapshot,
    attempts: BTreeMap<String, FactoryAttemptRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryAttemptReading {
    pub contract: String,
    pub revision: u64,
    pub run_ref: RunRef,
    pub run_revision: u64,
    pub topology_revision: u64,
    pub workflow_key: String,
    pub workflow_source_ref: String,
    pub workflow_source_revision: String,
    pub workflow_source_digest: String,
    pub legs: BTreeMap<WorkflowUnitRef, LegRecord>,
    pub attempts: Vec<FactoryAttemptRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptActionRequest {
    pub contract: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub operation: FactoryAttemptOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum FactoryAttemptOperation {
    StartSerial {
        attempt_ref: String,
        task_ref: String,
        parent_journey_ref: String,
        workflow_unit_ref: WorkflowUnitRef,
        disposition: SituatedExecutionDisposition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_grant: Option<RetryGrant>,
        #[serde(default)]
        tracking: Vec<AttemptTrackingFact>,
    },
    StartFork {
        parent_journey_ref: String,
        attempts: Vec<AttemptStart>,
    },
    BindDispatch {
        attempt_ref: String,
        execution_ref: String,
        receipt: OwnerOperationReceipt,
    },
    RecordObservation {
        attempt_ref: String,
        receipt: OwnerOperationReceipt,
    },
    RecordVerification {
        attempt_ref: String,
        verification: VerificationReceipt,
    },
    RecordTracking {
        attempt_ref: String,
        fact: AttemptTrackingFact,
    },
    RecordReresolution {
        attempt_ref: String,
        resolution: ReresolutionRecord,
    },
    RequestCancellation {
        attempt_ref: String,
    },
    AcceptCancellation {
        attempt_ref: String,
    },
    RecordProcessTermination {
        attempt_ref: String,
    },
    MarkQuiescent {
        attempt_ref: String,
    },
    Fail {
        attempt_ref: String,
        reason: String,
        #[serde(default)]
        evidence_refs: BTreeSet<String>,
    },
    Retry {
        attempt_ref: String,
        task_ref: String,
        parent_journey_ref: String,
        workflow_unit_ref: WorkflowUnitRef,
        grant_ref: String,
        disposition: SituatedExecutionDisposition,
        #[serde(default)]
        tracking: Vec<AttemptTrackingFact>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reresolution: Option<ReresolutionRecord>,
    },
    ReturnArtifact {
        attempt_ref: String,
        artifact: ReturnedArtifact,
        readable_return: ReadableReturn,
    },
    IncorporateLateResult {
        attempt_ref: String,
    },
    AdvanceSubject {
        subject_ref: String,
        revision: String,
    },
    AttachReceiving {
        attempt_ref: String,
        receiving_ref: String,
        source_revision: String,
        #[serde(default)]
        evidence_refs: BTreeSet<String>,
    },
    AttachArchive {
        attempt_ref: String,
        archive_ref: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        regression_observation_ref: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptStart {
    pub attempt_ref: String,
    pub task_ref: String,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub disposition: SituatedExecutionDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_grant: Option<RetryGrant>,
    #[serde(default)]
    pub tracking: Vec<AttemptTrackingFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryAttemptActionReceipt {
    pub contract: String,
    pub projection_ref: String,
    pub run_ref: RunRef,
    pub previous_revision: u64,
    pub next_revision: u64,
    pub operation: String,
    pub attempt_refs: Vec<String>,
    pub standing: String,
}

#[derive(Debug)]
pub struct FileAttemptStore {
    path: PathBuf,
    state: StoredAttemptState,
}

impl FileAttemptStore {
    pub fn initialize(
        path: impl Into<PathBuf>,
        seed: FactoryAttemptSeed,
    ) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        if path.exists() {
            return Err(FactoryAttemptError::AlreadyExists(path));
        }
        let workflow = compile_workflow(seed.workflow_source.clone())?;
        let engine = ExecutableOrchestration::new(workflow, seed.run)?;
        let state = StoredAttemptState {
            schema: FACTORY_ATTEMPT_STATE.into(),
            revision: 1,
            run: engine.run().clone(),
            workflow_source: seed.workflow_source,
            snapshot: engine.snapshot(),
            attempts: BTreeMap::new(),
        };
        validate_state(&state)?;
        let store = Self { path, state };
        store.persist_new()?;
        Ok(store)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        let state = read_state(&path)?;
        validate_state(&state)?;
        Ok(Self { path, state })
    }

    pub fn reading(&self) -> Result<FactoryAttemptReading, FactoryAttemptError> {
        reading_for(&self.state)
    }

    pub fn apply(
        &mut self,
        request: FactoryAttemptActionRequest,
    ) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
        validate_action_request(&request)?;
        let lock = lock_path(&self.path)?;
        let result = (|| {
            let mut state = read_state(&self.path)?;
            validate_state(&state)?;
            if state.run.reference() != &request.run_ref {
                return Err(FactoryAttemptError::RunMismatch {
                    addressed: request.run_ref.to_string(),
                    stored: state.run.reference().to_string(),
                });
            }
            if state.revision != request.expected_revision {
                return Err(FactoryAttemptError::RevisionConflict {
                    expected: request.expected_revision,
                    actual: state.revision,
                });
            }
            let previous_revision = state.revision;
            let (operation, attempt_refs) = apply_operation(&mut state, request.operation)?;
            state.revision = state
                .revision
                .checked_add(1)
                .ok_or(FactoryAttemptError::RevisionOverflow)?;
            validate_state(&state)?;
            persist_replace(&self.path, &state)?;
            self.state = state;
            Ok(FactoryAttemptActionReceipt {
                contract: FACTORY_ATTEMPT_ACTION.into(),
                projection_ref: request.projection_ref,
                run_ref: request.run_ref,
                previous_revision,
                next_revision: self.state.revision,
                operation,
                attempt_refs,
                standing: "native-factory-attempt-state; owner receipts are evidence, not whole-feature acceptance".into(),
            })
        })();
        FileExt::unlock(&lock)?;
        result
    }

    fn persist_new(&self) -> Result<(), FactoryAttemptError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.path)?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&self.path);
            return Err(error.into());
        }
        sync_parent(&self.path)?;
        Ok(())
    }
}

fn apply_operation(
    state: &mut StoredAttemptState,
    operation: FactoryAttemptOperation,
) -> Result<(String, Vec<String>), FactoryAttemptError> {
    let workflow = compile_workflow(state.workflow_source.clone())?;
    let mut engine = state.snapshot.restore(workflow, state.run.clone())?;
    let mut attempt_refs = Vec::new();
    let operation_name = match operation {
        FactoryAttemptOperation::StartSerial {
            attempt_ref,
            task_ref,
            parent_journey_ref,
            workflow_unit_ref,
            disposition,
            retry_grant,
            tracking,
        } => {
            let start = AttemptStart {
                attempt_ref: attempt_ref.clone(),
                task_ref,
                workflow_unit_ref,
                disposition,
                retry_grant,
                tracking,
            };
            let (launch, record) = prepare_start(state, &engine, &start, None)?;
            engine.start_serial(parent_journey_ref, &start.workflow_unit_ref, launch)?;
            state.attempts.insert(attempt_ref.clone(), record);
            attempt_refs.push(attempt_ref);
            "start-serial"
        }
        FactoryAttemptOperation::StartFork {
            parent_journey_ref,
            attempts,
        } => {
            if attempts.len() < 2 {
                return Err(FactoryAttemptError::InvalidOperation(
                    "a fork requires at least two attempts".into(),
                ));
            }
            let mut launches = BTreeMap::new();
            let mut records = Vec::new();
            let mut attempt_ids = BTreeSet::new();
            let mut unit_ids = BTreeSet::new();
            for start in &attempts {
                if !attempt_ids.insert(start.attempt_ref.clone()) {
                    return Err(FactoryAttemptError::DuplicateReference(
                        start.attempt_ref.clone(),
                    ));
                }
                if !unit_ids.insert(start.workflow_unit_ref.clone()) {
                    return Err(FactoryAttemptError::InvalidOperation(
                        "a fork cannot launch the same WorkflowUnit twice".into(),
                    ));
                }
                let (launch, record) = prepare_start(state, &engine, start, None)?;
                launches.insert(start.workflow_unit_ref.clone(), launch);
                records.push(record);
            }
            let units = attempts
                .iter()
                .map(|start| start.workflow_unit_ref.clone())
                .collect::<Vec<_>>();
            engine.fork(parent_journey_ref, &units, launches)?;
            for record in records {
                attempt_refs.push(record.attempt_ref.clone());
                state.attempts.insert(record.attempt_ref.clone(), record);
            }
            "start-fork"
        }
        FactoryAttemptOperation::BindDispatch {
            attempt_ref,
            execution_ref,
            receipt,
        } => {
            validate_owner_receipt(&receipt)?;
            let record = state
                .attempts
                .get_mut(&attempt_ref)
                .ok_or_else(|| FactoryAttemptError::UnknownAttempt(attempt_ref.clone()))?;
            if record.execution_ref.is_some() || record.dispatch.is_some() {
                return Err(FactoryAttemptError::InvalidOperation(
                    "attempt dispatch is already bound".into(),
                ));
            }
            if !matches!(
                receipt.phase,
                OwnerOperationPhase::Dispatching
                    | OwnerOperationPhase::Submitted
                    | OwnerOperationPhase::Returned
                    | OwnerOperationPhase::Failed
                    | OwnerOperationPhase::Cancelled
                    | OwnerOperationPhase::Uncertain
            ) {
                return Err(FactoryAttemptError::InvalidOperation(
                    "dispatch receipt has a non-dispatch phase".into(),
                ));
            }
            engine.bind_execution_identity(
                &record.workflow_unit_ref,
                &record.reserved_execution_ref,
                &execution_ref,
            )?;
            record.execution_ref = Some(execution_ref);
            record.dispatch = Some(receipt);
            attempt_refs.push(attempt_ref);
            "bind-dispatch"
        }
        FactoryAttemptOperation::RecordObservation {
            attempt_ref,
            receipt,
        } => {
            validate_owner_receipt(&receipt)?;
            let record = attempt_mut(state, &attempt_ref)?;
            ensure_no_duplicate_receipt(record, &receipt.receipt_ref)?;
            record.observations.push(receipt);
            attempt_refs.push(attempt_ref);
            "record-observation"
        }
        FactoryAttemptOperation::RecordVerification {
            attempt_ref,
            verification,
        } => {
            validate_verification(&verification)?;
            let record = attempt_mut(state, &attempt_ref)?;
            if record
                .verifications
                .iter()
                .any(|existing| existing.verification_ref == verification.verification_ref)
            {
                return Err(FactoryAttemptError::DuplicateReference(
                    verification.verification_ref,
                ));
            }
            let required = &record.disposition.verification_obligations;
            if verification.outcome == VerificationOutcome::Passed
                && (!verification.obligations.is_superset(required)
                    || verification.evidence_refs.is_empty())
            {
                return Err(FactoryAttemptError::VerificationIncomplete(
                    attempt_ref.clone(),
                ));
            }
            record.verifications.push(verification);
            attempt_refs.push(attempt_ref);
            "record-verification"
        }
        FactoryAttemptOperation::RecordTracking { attempt_ref, fact } => {
            validate_tracking(&fact)?;
            let record = attempt_mut(state, &attempt_ref)?;
            if record
                .tracking
                .iter()
                .any(|existing| existing.fact_ref == fact.fact_ref)
            {
                return Err(FactoryAttemptError::DuplicateReference(fact.fact_ref));
            }
            record.tracking.push(fact);
            attempt_refs.push(attempt_ref);
            "record-tracking"
        }
        FactoryAttemptOperation::RecordReresolution {
            attempt_ref,
            resolution,
        } => {
            validate_reresolution(&resolution)?;
            let record = attempt_mut(state, &attempt_ref)?;
            if record
                .reresolutions
                .iter()
                .any(|existing| existing.resolution_ref == resolution.resolution_ref)
            {
                return Err(FactoryAttemptError::DuplicateReference(
                    resolution.resolution_ref,
                ));
            }
            record.reresolutions.push(resolution);
            attempt_refs.push(attempt_ref);
            "record-reresolution"
        }
        FactoryAttemptOperation::RequestCancellation { attempt_ref } => {
            let unit = attempt_unit(state, &attempt_ref)?;
            engine.request_cancellation(&unit)?;
            attempt_refs.push(attempt_ref);
            "request-cancellation"
        }
        FactoryAttemptOperation::AcceptCancellation { attempt_ref } => {
            let unit = attempt_unit(state, &attempt_ref)?;
            engine.accept_cancellation(&unit)?;
            attempt_refs.push(attempt_ref);
            "accept-cancellation"
        }
        FactoryAttemptOperation::RecordProcessTermination { attempt_ref } => {
            let unit = attempt_unit(state, &attempt_ref)?;
            engine.record_process_termination(&unit)?;
            attempt_refs.push(attempt_ref);
            "record-process-termination"
        }
        FactoryAttemptOperation::MarkQuiescent { attempt_ref } => {
            let unit = attempt_unit(state, &attempt_ref)?;
            engine.mark_quiescent(&unit)?;
            attempt_refs.push(attempt_ref);
            "mark-quiescent"
        }
        FactoryAttemptOperation::Fail {
            attempt_ref,
            reason,
            evidence_refs,
        } => {
            if reason.trim().is_empty() || evidence_refs.is_empty() {
                return Err(FactoryAttemptError::InvalidOperation(
                    "failing an attempt requires reason and evidence".into(),
                ));
            }
            let unit = attempt_unit(state, &attempt_ref)?;
            engine.fail(&unit, reason)?;
            attempt_mut(state, &attempt_ref)?
                .failure_evidence_refs
                .extend(evidence_refs);
            attempt_refs.push(attempt_ref);
            "fail"
        }
        FactoryAttemptOperation::Retry {
            attempt_ref,
            task_ref,
            parent_journey_ref,
            workflow_unit_ref,
            grant_ref,
            disposition,
            tracking,
            reresolution,
        } => {
            let prior = current_attempt_for_unit(state, &engine, &workflow_unit_ref)?;
            let grant = engine.retry_grant(&grant_ref).cloned().ok_or_else(|| {
                FactoryAttemptError::InvalidOperation("unknown retry grant".into())
            })?;
            let start = AttemptStart {
                attempt_ref: attempt_ref.clone(),
                task_ref,
                workflow_unit_ref: workflow_unit_ref.clone(),
                disposition,
                retry_grant: Some(grant),
                tracking,
            };
            let (launch, mut record) = prepare_start(state, &engine, &start, Some(prior))?;
            if let Some(resolution) = reresolution {
                validate_reresolution(&resolution)?;
                record.reresolutions.push(resolution);
            }
            engine.retry(parent_journey_ref, &workflow_unit_ref, &grant_ref, launch)?;
            state.attempts.insert(attempt_ref.clone(), record);
            attempt_refs.push(attempt_ref);
            "retry"
        }
        FactoryAttemptOperation::ReturnArtifact {
            attempt_ref,
            artifact,
            readable_return,
        } => {
            validate_readable_return(&readable_return, &artifact)?;
            let unit = attempt_unit(state, &attempt_ref)?;
            {
                let record = attempt_mut(state, &attempt_ref)?;
                if record.readable_return.is_some() {
                    return Err(FactoryAttemptError::InvalidOperation(
                        "attempt already has a readable Return".into(),
                    ));
                }
                if !has_passing_verification(record) {
                    return Err(FactoryAttemptError::VerificationIncomplete(
                        attempt_ref.clone(),
                    ));
                }
                let execution_ref = record.execution_ref.as_deref().ok_or_else(|| {
                    FactoryAttemptError::InvalidOperation(
                        "cannot Return before native execution identity is bound".into(),
                    )
                })?;
                if artifact.producing_execution_ref != execution_ref {
                    return Err(FactoryAttemptError::InvalidOperation(
                        "Return artifact names another execution".into(),
                    ));
                }
            }
            engine.return_artifact(&unit, artifact)?;
            attempt_mut(state, &attempt_ref)?.readable_return = Some(readable_return);
            attempt_refs.push(attempt_ref);
            "return-artifact"
        }
        FactoryAttemptOperation::IncorporateLateResult { attempt_ref } => {
            let unit = attempt_unit(state, &attempt_ref)?;
            let record = attempt_mut(state, &attempt_ref)?;
            if !has_passing_verification(record) {
                return Err(FactoryAttemptError::VerificationIncomplete(
                    attempt_ref.clone(),
                ));
            }
            engine.incorporate_late_result(&unit)?;
            attempt_refs.push(attempt_ref);
            "incorporate-late-result"
        }
        FactoryAttemptOperation::AdvanceSubject {
            subject_ref,
            revision,
        } => {
            if subject_ref.trim().is_empty() || revision.trim().is_empty() {
                return Err(FactoryAttemptError::InvalidOperation(
                    "subject advance requires subject and revision".into(),
                ));
            }
            engine.advance_subject(subject_ref, revision);
            "advance-subject"
        }
        FactoryAttemptOperation::AttachReceiving {
            attempt_ref,
            receiving_ref,
            source_revision,
            evidence_refs,
        } => {
            if receiving_ref.trim().is_empty()
                || source_revision.trim().is_empty()
                || evidence_refs.is_empty()
            {
                return Err(FactoryAttemptError::InvalidOperation(
                    "receiving correlation requires receipt, source revision and evidence".into(),
                ));
            }
            let readable = attempt_mut(state, &attempt_ref)?
                .readable_return
                .as_mut()
                .ok_or_else(|| {
                    FactoryAttemptError::InvalidOperation("no readable Return".into())
                })?;
            if readable
                .receiving_ref
                .as_deref()
                .is_some_and(|current| current != receiving_ref)
            {
                return Err(FactoryAttemptError::InvalidOperation(
                    "a different receiving receipt is already attached".into(),
                ));
            }
            if readable
                .receiving_source_revision
                .as_deref()
                .is_some_and(|current| current != source_revision)
            {
                return Err(FactoryAttemptError::InvalidOperation(
                    "a different receiving source revision is already attached".into(),
                ));
            }
            readable.receiving_ref = Some(receiving_ref);
            readable.receiving_source_revision = Some(source_revision);
            readable.evidence_refs.extend(evidence_refs);
            attempt_refs.push(attempt_ref);
            "attach-receiving"
        }
        FactoryAttemptOperation::AttachArchive {
            attempt_ref,
            archive_ref,
            regression_observation_ref,
        } => {
            if archive_ref.trim().is_empty() {
                return Err(FactoryAttemptError::InvalidOperation(
                    "archive reference cannot be empty".into(),
                ));
            }
            let readable = attempt_mut(state, &attempt_ref)?
                .readable_return
                .as_mut()
                .ok_or_else(|| {
                    FactoryAttemptError::InvalidOperation("no readable Return".into())
                })?;
            readable.archive_refs.insert(archive_ref);
            if let Some(reference) = regression_observation_ref {
                if reference.trim().is_empty() {
                    return Err(FactoryAttemptError::InvalidOperation(
                        "regression observation reference cannot be empty".into(),
                    ));
                }
                readable.regression_observation_refs.insert(reference);
            }
            attempt_refs.push(attempt_ref);
            "attach-archive"
        }
    };
    state.run = engine.run().clone();
    state.snapshot = engine.snapshot();
    Ok((operation_name.into(), attempt_refs))
}

fn prepare_start(
    state: &StoredAttemptState,
    engine: &ExecutableOrchestration,
    start: &AttemptStart,
    prior: Option<&FactoryAttemptRecord>,
) -> Result<(ExecutionLaunch, FactoryAttemptRecord), FactoryAttemptError> {
    required_text(&start.attempt_ref, "attemptRef")?;
    required_text(&start.task_ref, "taskRef")?;
    if state.attempts.contains_key(&start.attempt_ref) {
        return Err(FactoryAttemptError::DuplicateReference(
            start.attempt_ref.clone(),
        ));
    }
    let unit = engine
        .workflow()
        .units
        .values()
        .find(|unit| unit.reference == start.workflow_unit_ref)
        .ok_or_else(|| FactoryAttemptError::InvalidOperation("unknown workflow unit".into()))?;
    validate_disposition(
        &start.disposition,
        engine.run().reference(),
        unit,
        start.retry_grant.as_ref(),
    )?;
    for fact in &start.tracking {
        validate_tracking(fact)?;
    }
    unique_tracking(&start.tracking)?;
    if let Some(prior) = prior {
        ensure_retry_protection(prior, &start.disposition)?;
    }
    let reserved_execution_ref = format!("factory-attempt:{}", start.attempt_ref);
    let launch = ExecutionLaunch {
        execution_ref: reserved_execution_ref.clone(),
        disposition: start.disposition.selection.clone(),
        retry_grant: start.retry_grant.clone(),
    };
    Ok((
        launch,
        FactoryAttemptRecord {
            attempt_ref: start.attempt_ref.clone(),
            task_ref: start.task_ref.clone(),
            workflow_unit_ref: start.workflow_unit_ref.clone(),
            reserved_execution_ref,
            execution_ref: None,
            disposition: start.disposition.clone(),
            dispatch: None,
            observations: Vec::new(),
            verifications: Vec::new(),
            tracking: start.tracking.clone(),
            reresolutions: Vec::new(),
            failure_evidence_refs: BTreeSet::new(),
            readable_return: None,
        },
    ))
}

fn validate_disposition(
    disposition: &SituatedExecutionDisposition,
    run_ref: &RunRef,
    unit: &CompiledWorkflowUnit,
    retry_grant: Option<&RetryGrant>,
) -> Result<(), FactoryAttemptError> {
    let demand = &disposition.selection.demand;
    if demand.run_ref != run_ref.to_string()
        || demand.workflow_unit_ref.as_deref() != Some(unit.reference.to_string().as_str())
        || demand.agency_ref.as_deref() != Some(disposition.participant.agency_ref.as_str())
    {
        return Err(FactoryAttemptError::InvalidDisposition(
            "Run, WorkflowUnit or Agency differs from Execution Intelligence demand".into(),
        ));
    }
    if let Some(profile) = &demand.profile_ref {
        if disposition.participant.profile_ref.as_deref() != Some(profile.as_str()) {
            return Err(FactoryAttemptError::InvalidDisposition(
                "profile-bearing demand must retain the exact profile; profile-less demand stays valid".into(),
            ));
        }
    }
    for (field, value) in [
        ("agentRef", disposition.participant.agent_ref.as_str()),
        ("agencyRef", disposition.participant.agency_ref.as_str()),
        (
            "worldBindingRef",
            disposition.participant.world_binding_ref.as_str(),
        ),
        ("sourceRef", disposition.participant.source_ref.as_str()),
        (
            "sourceRevision",
            disposition.participant.source_revision.as_str(),
        ),
        (
            "sourceDigest",
            disposition.participant.source_digest.as_str(),
        ),
        ("routeRef", disposition.body.route_ref.as_str()),
        ("harnessRef", disposition.body.harness_ref.as_str()),
        (
            "harnessCompositionRef",
            disposition.body.harness_composition_ref.as_str(),
        ),
        (
            "agentSessionRef",
            disposition.body.agent_session_ref.as_str(),
        ),
        (
            "sessionSpaceRef",
            disposition.body.session_space_ref.as_str(),
        ),
    ] {
        required_text(value, field)?;
    }
    if !disposition.participant.source_digest.starts_with("blake3:") {
        return Err(FactoryAttemptError::InvalidDisposition(
            "participant source digest must retain an exact blake3 basis".into(),
        ));
    }
    if disposition.body.model_ref != disposition.selection.selection.model_ref
        || disposition.body.provider_ref != disposition.selection.selection.provider_ref
    {
        return Err(FactoryAttemptError::InvalidDisposition(
            "realised body model/provider differs from selected model/provider".into(),
        ));
    }
    if !disposition.praxis_refs.is_superset(&unit.praxis_refs)
        || !disposition
            .capability_refs
            .is_superset(&unit.capability_refs)
        || disposition.permitted_effects != unit.permitted_effects
        || disposition.verification_obligations != unit.verification_obligations
        || disposition.return_address != unit.return_address
        || disposition.stop_conditions != unit.stop_conditions
        || disposition.escalation_conditions != unit.escalation_conditions
    {
        return Err(FactoryAttemptError::InvalidDisposition(
            "situated arrangement weakens or changes compiled workflow law".into(),
        ));
    }
    if disposition.budget.cost_ceiling_usd != demand.cost_ceiling_usd
        || disposition.budget.latency_preference_ms != demand.latency_preference_ms
    {
        return Err(FactoryAttemptError::InvalidDisposition(
            "execution budget differs from selected demand".into(),
        ));
    }
    if let Some(grant) = retry_grant {
        if disposition.budget.retry_grant_ref.as_deref() != Some(grant.grant_ref.as_str())
            || disposition.budget.maximum_attempts != Some(grant.attempts_allowed)
        {
            return Err(FactoryAttemptError::InvalidDisposition(
                "retry budget differs from the consumed Factory grant".into(),
            ));
        }
    }
    let effectful = disposition.permitted_effects.iter().any(|effect| {
        let effect = effect.to_ascii_lowercase();
        effect.starts_with("write:") || effect.starts_with("mutate:") || effect.contains("write ")
    });
    if (effectful || demand.requires_local_materialisation) && disposition.placement.is_none() {
        return Err(FactoryAttemptError::InvalidDisposition(
            "effectful/materialised attempt requires an explicit placement/protection basis".into(),
        ));
    }
    if let Some(placement) = &disposition.placement {
        for (field, value) in [
            ("nowRef", placement.now_ref.as_str()),
            ("nowPath", placement.now_path.as_str()),
            ("policyRef", placement.policy_ref.as_str()),
            ("policyRevision", placement.policy_revision.as_str()),
            ("authorityRef", placement.authority_ref.as_str()),
        ] {
            required_text(value, field)?;
        }
        if !placement
            .effective_coverage
            .is_superset(&placement.required_coverage)
        {
            return Err(FactoryAttemptError::InvalidDisposition(
                "effective protection does not satisfy required coverage".into(),
            ));
        }
    }
    Ok(())
}

fn ensure_retry_protection(
    prior: &FactoryAttemptRecord,
    next: &SituatedExecutionDisposition,
) -> Result<(), FactoryAttemptError> {
    match (&prior.disposition.placement, &next.placement) {
        (Some(before), Some(after)) => {
            if before.required_coverage != after.required_coverage {
                return Err(FactoryAttemptError::InvalidDisposition(
                    "retry cannot weaken or silently change required protection coverage".into(),
                ));
            }
        }
        (Some(_), None) => {
            return Err(FactoryAttemptError::InvalidDisposition(
                "retry cannot remove required placement/protection".into(),
            ));
        }
        _ => {}
    }
    if prior.disposition.permitted_effects != next.permitted_effects
        || prior.disposition.verification_obligations != next.verification_obligations
    {
        return Err(FactoryAttemptError::InvalidDisposition(
            "retry cannot change permitted effects or verification obligations".into(),
        ));
    }
    Ok(())
}

fn has_passing_verification(record: &FactoryAttemptRecord) -> bool {
    record.verifications.iter().any(|verification| {
        verification.outcome == VerificationOutcome::Passed
            && verification
                .obligations
                .is_superset(&record.disposition.verification_obligations)
            && !verification.evidence_refs.is_empty()
    })
}

fn validate_owner_receipt(receipt: &OwnerOperationReceipt) -> Result<(), FactoryAttemptError> {
    for (field, value) in [
        ("ownerRef", receipt.owner_ref.as_str()),
        ("contract", receipt.contract.as_str()),
        ("operationRef", receipt.operation_ref.as_str()),
        ("receiptRef", receipt.receipt_ref.as_str()),
        ("sourceRevision", receipt.source_revision.as_str()),
    ] {
        required_text(value, field)?;
    }
    Ok(())
}

fn validate_verification(receipt: &VerificationReceipt) -> Result<(), FactoryAttemptError> {
    required_text(&receipt.verification_ref, "verificationRef")?;
    required_text(&receipt.owner_ref, "verification.ownerRef")?;
    required_text(&receipt.source_revision, "verification.sourceRevision")?;
    if receipt.outcome == VerificationOutcome::Passed && receipt.evidence_refs.is_empty() {
        return Err(FactoryAttemptError::InvalidOperation(
            "passed verification requires evidence".into(),
        ));
    }
    Ok(())
}

fn validate_tracking(fact: &AttemptTrackingFact) -> Result<(), FactoryAttemptError> {
    for (field, value) in [
        ("factRef", fact.fact_ref.as_str()),
        ("kind", fact.kind.as_str()),
        ("ownerRef", fact.owner_ref.as_str()),
        ("subjectRef", fact.subject_ref.as_str()),
        ("sourceRevision", fact.source_revision.as_str()),
    ] {
        required_text(value, field)?;
    }
    Ok(())
}

fn unique_tracking(facts: &[AttemptTrackingFact]) -> Result<(), FactoryAttemptError> {
    let mut refs = BTreeSet::new();
    for fact in facts {
        if !refs.insert(&fact.fact_ref) {
            return Err(FactoryAttemptError::DuplicateReference(
                fact.fact_ref.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_reresolution(record: &ReresolutionRecord) -> Result<(), FactoryAttemptError> {
    required_text(&record.resolution_ref, "resolutionRef")?;
    required_text(&record.reason, "resolution.reason")?;
    required_text(&record.source_revision, "resolution.sourceRevision")?;
    if record.evidence_refs.is_empty() {
        return Err(FactoryAttemptError::InvalidOperation(
            "re-resolution requires evidence".into(),
        ));
    }
    Ok(())
}

fn validate_readable_return(
    readable: &ReadableReturn,
    artifact: &ReturnedArtifact,
) -> Result<(), FactoryAttemptError> {
    required_text(&readable.return_ref, "returnRef")?;
    required_text(&readable.summary, "return.summary")?;
    if !readable.artifact_refs.contains(&artifact.artifact_ref)
        || !readable.evidence_refs.is_superset(&artifact.evidence_refs)
    {
        return Err(FactoryAttemptError::InvalidOperation(
            "readable Return must retain the returned artifact and its evidence".into(),
        ));
    }
    Ok(())
}

fn attempt_mut<'a>(
    state: &'a mut StoredAttemptState,
    attempt_ref: &str,
) -> Result<&'a mut FactoryAttemptRecord, FactoryAttemptError> {
    state
        .attempts
        .get_mut(attempt_ref)
        .ok_or_else(|| FactoryAttemptError::UnknownAttempt(attempt_ref.into()))
}

fn attempt_unit(
    state: &StoredAttemptState,
    attempt_ref: &str,
) -> Result<WorkflowUnitRef, FactoryAttemptError> {
    state
        .attempts
        .get(attempt_ref)
        .map(|record| record.workflow_unit_ref.clone())
        .ok_or_else(|| FactoryAttemptError::UnknownAttempt(attempt_ref.into()))
}

fn current_attempt_for_unit<'a>(
    state: &'a StoredAttemptState,
    engine: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let execution_ref = &engine
        .leg(unit)
        .ok_or_else(|| FactoryAttemptError::InvalidOperation("no current leg for retry".into()))?
        .execution_ref;
    state
        .attempts
        .values()
        .find(|record| {
            &record.workflow_unit_ref == unit
                && (record.execution_ref.as_deref() == Some(execution_ref.as_str())
                    || record.reserved_execution_ref == *execution_ref)
        })
        .ok_or_else(|| {
            FactoryAttemptError::CorruptState(
                "current coordinator execution has no durable attempt record".into(),
            )
        })
}

fn ensure_no_duplicate_receipt(
    record: &FactoryAttemptRecord,
    receipt_ref: &str,
) -> Result<(), FactoryAttemptError> {
    if record
        .dispatch
        .as_ref()
        .is_some_and(|receipt| receipt.receipt_ref == receipt_ref)
        || record
            .observations
            .iter()
            .any(|receipt| receipt.receipt_ref == receipt_ref)
    {
        return Err(FactoryAttemptError::DuplicateReference(receipt_ref.into()));
    }
    Ok(())
}

fn validate_action_request(
    request: &FactoryAttemptActionRequest,
) -> Result<(), FactoryAttemptError> {
    if request.contract != FACTORY_ATTEMPT_ACTION {
        return Err(FactoryAttemptError::UnsupportedContract(
            request.contract.clone(),
        ));
    }
    required_text(&request.projection_ref, "projectionRef")?;
    required_text(&request.caller.caller_ref, "callerRef")?;
    if request.caller.lineage.last().map(String::as_str) != Some(request.caller.caller_ref.as_str())
    {
        return Err(FactoryAttemptError::InvalidAuthority(
            "caller lineage must terminate at callerRef".into(),
        ));
    }
    if request.authority.native_owner != "factory"
        || request.authority.capability_ref.as_deref() != Some(FACTORY_ATTEMPT_CAPABILITY_REF)
        || !request.authority.capability_granted
        || !request.authority.action_authorised
        || request.authority.authority_ref.trim().is_empty()
    {
        return Err(FactoryAttemptError::InvalidAuthority(
            "Factory attempt operation requires native Factory authority and capability grant"
                .into(),
        ));
    }
    Ok(())
}

fn validate_state(state: &StoredAttemptState) -> Result<(), FactoryAttemptError> {
    if state.schema != FACTORY_ATTEMPT_STATE {
        return Err(FactoryAttemptError::UnsupportedContract(
            state.schema.clone(),
        ));
    }
    if state.revision == 0 {
        return Err(FactoryAttemptError::CorruptState(
            "zero state revision".into(),
        ));
    }
    let workflow = compile_workflow(state.workflow_source.clone())?;
    let engine = state.snapshot.restore(workflow, state.run.clone())?;
    let mut execution_refs = BTreeSet::new();
    for record in state.attempts.values() {
        required_text(&record.attempt_ref, "attemptRef")?;
        required_text(&record.task_ref, "taskRef")?;
        if record.reserved_execution_ref != format!("factory-attempt:{}", record.attempt_ref) {
            return Err(FactoryAttemptError::CorruptState(
                "attempt reservation identity drifted".into(),
            ));
        }
        let unit = engine
            .workflow()
            .units
            .values()
            .find(|unit| unit.reference == record.workflow_unit_ref)
            .ok_or_else(|| {
                FactoryAttemptError::CorruptState("attempt names unknown unit".into())
            })?;
        validate_disposition(&record.disposition, state.run.reference(), unit, None)?;
        if let Some(grant_ref) = &record.disposition.budget.retry_grant_ref {
            let grant = engine.retry_grant(grant_ref).ok_or_else(|| {
                FactoryAttemptError::CorruptState(
                    "attempt refers to a retry grant absent from the coordinator".into(),
                )
            })?;
            if record.disposition.budget.maximum_attempts != Some(grant.attempts_allowed) {
                return Err(FactoryAttemptError::CorruptState(
                    "attempt retry budget differs from the persisted grant".into(),
                ));
            }
        }
        if let Some(execution_ref) = &record.execution_ref {
            if !execution_refs.insert(execution_ref) {
                return Err(FactoryAttemptError::CorruptState(
                    "execution identity attached to multiple attempts".into(),
                ));
            }
        }
        unique_tracking(&record.tracking)?;
        for fact in &record.tracking {
            validate_tracking(fact)?;
        }
        for receipt in &record.observations {
            validate_owner_receipt(receipt)?;
        }
        if let Some(receipt) = &record.dispatch {
            validate_owner_receipt(receipt)?;
        }
        for verification in &record.verifications {
            validate_verification(verification)?;
        }
        for resolution in &record.reresolutions {
            validate_reresolution(resolution)?;
        }
        if let Some(readable) = &record.readable_return {
            required_text(&readable.return_ref, "returnRef")?;
            required_text(&readable.summary, "return.summary")?;
        }
    }
    Ok(())
}

fn reading_for(state: &StoredAttemptState) -> Result<FactoryAttemptReading, FactoryAttemptError> {
    let workflow = compile_workflow(state.workflow_source.clone())?;
    let engine = state.snapshot.restore(workflow, state.run.clone())?;
    Ok(FactoryAttemptReading {
        contract: FACTORY_ATTEMPT_READING.into(),
        revision: state.revision,
        run_ref: state.run.reference().clone(),
        run_revision: state.run.revision().get(),
        topology_revision: state.run.map().topology_revision().get(),
        workflow_key: engine.workflow().workflow_key.clone(),
        workflow_source_ref: engine.workflow().source.reference.to_string(),
        workflow_source_revision: engine.workflow().source.revision.clone(),
        workflow_source_digest: engine.workflow().source.digest.clone(),
        legs: engine.legs().clone(),
        attempts: state.attempts.values().cloned().collect(),
    })
}

fn read_state(path: &Path) -> Result<StoredAttemptState, FactoryAttemptError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn persist_replace(path: &Path, state: &StoredAttemptState) -> Result<(), FactoryAttemptError> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("factory-attempt.json"),
        std::process::id(),
        ulid::Ulid::new()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(state)?)?;
    file.sync_all()?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })?;
    sync_parent(path)?;
    Ok(())
}

fn lock_path(path: &Path) -> Result<fs::File, FactoryAttemptError> {
    let lock_path = path.with_extension("attempt.lock");
    if let Some(parent) = lock_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    Ok(lock)
}

fn sync_parent(path: &Path) -> Result<(), FactoryAttemptError> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn required_text(value: &str, field: &'static str) -> Result<(), FactoryAttemptError> {
    if value.trim().is_empty() {
        Err(FactoryAttemptError::EmptyField(field))
    } else {
        Ok(())
    }
}

/// Public `factory attempt` command implementation. The top-level Factory binary
/// delegates here without changing the older Build/development CLI contract.
pub fn execute_attempt_cli(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, FactoryAttemptError> {
    let mut args = args.to_vec();
    let json = remove_flag(&mut args, "--json");
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(attempt_help()),
        Some("init") => {
            let state = args
                .get(1)
                .ok_or_else(|| FactoryAttemptError::Cli("missing attempt state path".into()))?;
            let input = args.get(2).map(String::as_str).unwrap_or("-");
            let seed: FactoryAttemptSeed =
                serde_json::from_str(&read_input(input, stdin_override)?)?;
            let store = FileAttemptStore::initialize(state, seed)?;
            render_reading(store.reading()?, json)
        }
        Some("read") => {
            let state = args
                .get(1)
                .ok_or_else(|| FactoryAttemptError::Cli("missing attempt state path".into()))?;
            render_reading(FileAttemptStore::open(state)?.reading()?, json)
        }
        Some("action") => {
            let state = args
                .get(1)
                .ok_or_else(|| FactoryAttemptError::Cli("missing attempt state path".into()))?;
            let input = args.get(2).map(String::as_str).unwrap_or("-");
            let request: FactoryAttemptActionRequest =
                serde_json::from_str(&read_input(input, stdin_override)?)?;
            let mut store = FileAttemptStore::open(state)?;
            let receipt = store.apply(request)?;
            if json {
                Ok(serde_json::to_string_pretty(&receipt)?)
            } else {
                Ok(format!(
                    "{}\nRun: {}\nOperation: {}\nRevision: {} -> {}\nAttempts: {}\n{}",
                    receipt.contract,
                    receipt.run_ref,
                    receipt.operation,
                    receipt.previous_revision,
                    receipt.next_revision,
                    receipt.attempt_refs.join(", "),
                    receipt.standing
                ))
            }
        }
        Some(other) => Err(FactoryAttemptError::Cli(format!(
            "unknown attempt command `{other}`"
        ))),
    }
}

pub fn attempt_cli_main(args: &[String]) -> std::process::ExitCode {
    match execute_attempt_cli(args, None) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("factory attempt: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

fn attempt_help() -> String {
    "Factory native attempt operations\n\nUsage:\n  factory attempt init   <state> [seed-file|-] [--json]\n  factory attempt read   <state> [--json]\n  factory attempt action <state> [request-file|-] [--json]\n\nThe durable state revalidates the canonical Run/workflow/snapshot basis on every operation. Owner delivery receipts never imply verification or human acceptance.".into()
}

fn render_reading(
    reading: FactoryAttemptReading,
    json: bool,
) -> Result<String, FactoryAttemptError> {
    if json {
        return Ok(serde_json::to_string_pretty(&reading)?);
    }
    Ok(format!(
        "{}\nRun: {} @ {}\nWorkflow: {} ({} @ {})\nState revision: {}\nAttempts: {}",
        reading.contract,
        reading.run_ref,
        reading.run_revision,
        reading.workflow_key,
        reading.workflow_source_ref,
        reading.workflow_source_revision,
        reading.revision,
        reading.attempts.len()
    ))
}

fn read_input(path: &str, stdin_override: Option<&str>) -> Result<String, FactoryAttemptError> {
    if path != "-" {
        return Ok(fs::read_to_string(path)?);
    }
    if let Some(input) = stdin_override {
        return Ok(input.into());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|arg| arg != flag);
    before != args.len()
}

#[derive(Debug)]
pub enum FactoryAttemptError {
    Io(io::Error),
    Json(serde_json::Error),
    Workflow(crate::workflow::WorkflowError),
    Orchestration(OrchestrationError),
    AlreadyExists(PathBuf),
    UnsupportedContract(String),
    EmptyField(&'static str),
    InvalidAuthority(String),
    InvalidDisposition(String),
    InvalidOperation(String),
    UnknownAttempt(String),
    DuplicateReference(String),
    VerificationIncomplete(String),
    RunMismatch { addressed: String, stored: String },
    RevisionConflict { expected: u64, actual: u64 },
    RevisionOverflow,
    CorruptState(String),
    Cli(String),
}

impl Display for FactoryAttemptError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory attempt error: {self:?}")
    }
}

impl Error for FactoryAttemptError {}

impl From<io::Error> for FactoryAttemptError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FactoryAttemptError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<crate::workflow::WorkflowError> for FactoryAttemptError {
    fn from(error: crate::workflow::WorkflowError) -> Self {
        Self::Workflow(error)
    }
}

impl From<OrchestrationError> for FactoryAttemptError {
    fn from(error: OrchestrationError) -> Self {
        Self::Orchestration(error)
    }
}
