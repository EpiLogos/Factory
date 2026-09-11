//! Factory-owned executable orchestration over the canonical Run/RunMap path.
//!
//! This is deliberately a coordinator, not a second workflow runner. Workflow
//! identity and graph structure come from [`crate::workflow::CompiledWorkflow`];
//! every executable transition is applied through the owning [`Run`] mutation
//! authority, and every launch consumes an Execution Intelligence disposition.

use crate::core::run::{
    CommandOutcome, NodeId, NodeState, Run, RunContractError, RunMutationAuthority,
    RunTopologyCommand, TopologyMutation, WorkflowUnitRef,
};
use crate::execution_intelligence::ExecutionDisposition;
use crate::workflow::{CompiledWorkflow, CompiledWorkflowUnit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[path = "orchestration_persistence.rs"]
mod persistence;
pub use persistence::OrchestrationSnapshot;

pub const ORCHESTRATION_CONTRACT: &str = "factory.agent-orchestration/v1";

/// All information a child receives at the fork boundary. This is a value,
/// not an implicit closure over the parent's process or filesystem.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationContract {
    pub parent_journey_ref: String,
    pub parent_run_ref: String,
    #[serde(rename = "executionUnitRef")]
    pub execution_unit_ref: WorkflowUnitRef,
    pub concern: String,
    pub subject_ref: String,
    pub basis_revision: String,
    pub agent_requirements: crate::workflow::CompiledAgentRequirements,
    pub permitted_effects: BTreeSet<String>,
    pub verification_obligations: BTreeSet<String>,
    pub return_address: String,
    pub stop_conditions: String,
    pub retry_grant: Option<RetryGrant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionLaunch {
    pub execution_ref: String,
    pub disposition: ExecutionDisposition,
    pub retry_grant: Option<RetryGrant>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryGrant {
    pub grant_ref: String,
    pub attempts_allowed: u32,
    pub attempts_spent: u32,
    pub revoked: bool,
}

impl RetryGrant {
    pub fn new(
        grant_ref: impl Into<String>,
        attempts_allowed: u32,
    ) -> Result<Self, OrchestrationError> {
        let grant_ref = grant_ref.into();
        if grant_ref.trim().is_empty() || attempts_allowed == 0 {
            return Err(OrchestrationError::InvalidRetryGrant);
        }
        Ok(Self {
            grant_ref,
            attempts_allowed,
            attempts_spent: 0,
            revoked: false,
        })
    }

    pub fn remaining(&self) -> u32 {
        self.attempts_allowed.saturating_sub(self.attempts_spent)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegStatus {
    Active,
    Detached,
    CancelRequested,
    CancellationAccepted,
    ProcessTerminated,
    Quiescent,
    Returned,
    Failed,
    LateResult,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnedArtifact {
    pub artifact_ref: String,
    pub subject_ref: String,
    pub subject_revision: String,
    pub producing_execution_ref: String,
    pub evidence_refs: BTreeSet<String>,
    pub semantic_difference: String,
}

/// One immutable execution attempt. A retry creates another value; it never
/// replaces the receipt for the attempt that failed or was cancelled.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionAttempt {
    pub execution_ref: String,
    pub delegation: DelegationContract,
    pub status: LegStatus,
    pub status_history: Vec<LegStatus>,
    pub artifacts: Vec<ReturnedArtifact>,
    pub late_artifacts: Vec<ReturnedArtifact>,
    pub failure_reason: Option<String>,
}

impl ReturnedArtifact {
    fn validate(&self) -> Result<(), OrchestrationError> {
        for (field, value) in [
            ("artifactRef", self.artifact_ref.as_str()),
            ("subjectRef", self.subject_ref.as_str()),
            ("subjectRevision", self.subject_revision.as_str()),
            (
                "producingExecutionRef",
                self.producing_execution_ref.as_str(),
            ),
            ("semanticDifference", self.semantic_difference.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(OrchestrationError::EmptyField(field));
            }
        }
        if self.evidence_refs.is_empty() {
            return Err(OrchestrationError::MissingEvidence(
                self.artifact_ref.clone(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegRecord {
    pub delegation: DelegationContract,
    pub execution_ref: String,
    pub status: LegStatus,
    pub status_history: Vec<LegStatus>,
    pub attempts: Vec<ExecutionAttempt>,
    pub artifacts: Vec<ReturnedArtifact>,
    pub late_artifacts: Vec<ReturnedArtifact>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarrierState {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BarrierReading {
    pub key: String,
    pub state: BarrierState,
    pub required_units: BTreeSet<WorkflowUnitRef>,
    pub returned_units: BTreeSet<WorkflowUnitRef>,
    pub missing_units: BTreeSet<WorkflowUnitRef>,
    pub failed_units: BTreeSet<WorkflowUnitRef>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WholeRunState {
    Incomplete,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndependentReviewer {
    pub execution_ref: String,
    pub disposition_run_ref: String,
    pub review_of: BTreeSet<WorkflowUnitRef>,
    pub independent_from_execution_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisRecord {
    pub synthesis_ref: String,
    pub barrier_key: String,
    pub reviewer_execution_ref: String,
    pub artifact_refs: BTreeSet<String>,
    pub evidence_refs: BTreeSet<String>,
    pub integrated_difference: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureFinding {
    BrittleWorkflowScripting,
    OrphanExecutionLeg,
    IncompleteBarrier,
    ConcatenationAsSynthesis,
    ProducerSelfReview,
}

/// Explicit owner evidence for an architecture finding that is outside the
/// native Run transition itself. The gate reports these receipts; it does not
/// pretend to discover a foreign script or orphan process by inference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerArchitectureObservationReceipt {
    pub observation_ref: String,
    pub owner_ref: String,
    pub finding: ArchitectureFinding,
    pub subject_ref: String,
    pub evidence_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureIntegrityReport {
    pub contract: String,
    pub findings: BTreeSet<ArchitectureFinding>,
    pub orphan_execution_refs: BTreeSet<String>,
    pub incomplete_barriers: BTreeSet<String>,
    pub owner_observation_refs: BTreeSet<String>,
}

impl ArchitectureIntegrityReport {
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationError {
    Run(RunContractError),
    EmptyField(&'static str),
    DuplicateExecution(String),
    UnknownUnit(String),
    UnknownBarrier(String),
    InvalidExecutionDisposition {
        execution: String,
        reason: String,
    },
    DependencyNotSatisfied(String),
    BarrierNotSatisfied(String),
    EmptyFork,
    NotIndependent {
        left: WorkflowUnitRef,
        right: WorkflowUnitRef,
    },
    SharedWriterConflict {
        subject: String,
        owner: WorkflowUnitRef,
    },
    InvalidTransition {
        unit: WorkflowUnitRef,
        status: LegStatus,
    },
    MissingLeg(String),
    InvalidArtifact(String),
    MissingEvidence(String),
    StaleLateResult {
        artifact: String,
        current_revision: String,
    },
    RetryExhausted(String),
    RetryRevoked(String),
    InvalidRetryGrant,
    DuplicateRetryGrant(String),
    InvalidArchitectureObservation,
    DuplicateArchitectureObservation(String),
    ReviewerAlreadyProducer(String),
    ReviewerNotIndependent(String),
    BarrierIncomplete(String),
    ConcatenationRejected(String),
    SynthesisAlreadyExists(String),
    InvalidSnapshot(String),
}

impl Display for OrchestrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory orchestration error: {self:?}")
    }
}

impl Error for OrchestrationError {}

impl From<RunContractError> for OrchestrationError {
    fn from(error: RunContractError) -> Self {
        Self::Run(error)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutableOrchestration {
    workflow: CompiledWorkflow,
    run: Run,
    authority: RunMutationAuthority,
    legs: BTreeMap<WorkflowUnitRef, LegRecord>,
    subject_revisions: BTreeMap<String, String>,
    active_writers: BTreeMap<String, WorkflowUnitRef>,
    retry_grants: BTreeMap<String, RetryGrant>,
    reviewers: BTreeMap<String, IndependentReviewer>,
    syntheses: BTreeMap<String, SynthesisRecord>,
    owner_observations: BTreeMap<String, OwnerArchitectureObservationReceipt>,
    concatenation_attempts: BTreeSet<String>,
    self_review_attempts: BTreeSet<String>,
}

impl ExecutableOrchestration {
    /// Compile the supplied graph into the existing RunMap with one native,
    /// atomic topology command. No scheduler or shell DAG is introduced.
    pub fn new(workflow: CompiledWorkflow, mut run: Run) -> Result<Self, OrchestrationError> {
        let authority = run.mutation_authority();
        let command = workflow.topology_command(run.revision());
        run.apply_topology_command(&authority, command)?;
        let mut subject_revisions = BTreeMap::new();
        for unit in workflow.units.values() {
            subject_revisions
                .entry(unit.subject_ref.to_string())
                .or_insert_with(|| unit.basis_revision.clone());
        }
        Ok(Self {
            workflow,
            run,
            authority,
            legs: BTreeMap::new(),
            subject_revisions,
            active_writers: BTreeMap::new(),
            retry_grants: BTreeMap::new(),
            reviewers: BTreeMap::new(),
            syntheses: BTreeMap::new(),
            owner_observations: BTreeMap::new(),
            concatenation_attempts: BTreeSet::new(),
            self_review_attempts: BTreeSet::new(),
        })
    }

    pub fn contract(&self) -> &'static str {
        ORCHESTRATION_CONTRACT
    }

    pub fn workflow(&self) -> &CompiledWorkflow {
        &self.workflow
    }

    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn leg(&self, unit: &WorkflowUnitRef) -> Option<&LegRecord> {
        self.legs.get(unit)
    }

    pub fn legs(&self) -> &BTreeMap<WorkflowUnitRef, LegRecord> {
        &self.legs
    }

    pub fn retry_grant(&self, grant_ref: &str) -> Option<&RetryGrant> {
        self.retry_grants.get(grant_ref)
    }

    pub fn delegation(&self, unit: &WorkflowUnitRef) -> Option<&DelegationContract> {
        self.legs.get(unit).map(|leg| &leg.delegation)
    }

    /// Start a genuinely independent fork. All pairs must be explicitly
    /// independent in the compiled source, and shared writable subjects are
    /// rejected before any child is admitted.
    pub fn fork(
        &mut self,
        parent_journey_ref: impl Into<String>,
        units: &[WorkflowUnitRef],
        launches: BTreeMap<WorkflowUnitRef, ExecutionLaunch>,
    ) -> Result<Vec<DelegationContract>, OrchestrationError> {
        if units.len() < 2 {
            return Err(OrchestrationError::EmptyFork);
        }
        self.validate_fork(units, &launches)?;
        let parent_journey_ref = parent_journey_ref.into();
        let mut staged = self.clone();
        let mut delegations = Vec::with_capacity(units.len());
        for unit in units {
            let launch = launches
                .get(unit)
                .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?
                .clone();
            delegations.push(staged.start_unit(parent_journey_ref.clone(), unit, launch)?);
        }
        *self = staged;
        Ok(delegations)
    }

    /// Start ordinary dependent/serial work. It uses the same native launch
    /// path as a fork, but does not require an independence relation.
    pub fn start_serial(
        &mut self,
        parent_journey_ref: impl Into<String>,
        unit: &WorkflowUnitRef,
        launch: ExecutionLaunch,
    ) -> Result<DelegationContract, OrchestrationError> {
        if self.legs.contains_key(unit) {
            return Err(OrchestrationError::DuplicateExecution(unit.to_string()));
        }
        self.validate_launch(unit, &launch)?;
        let mut staged = self.clone();
        let delegation = staged.start_unit(parent_journey_ref.into(), unit, launch)?;
        *self = staged;
        Ok(delegation)
    }

    fn validate_fork(
        &self,
        units: &[WorkflowUnitRef],
        launches: &BTreeMap<WorkflowUnitRef, ExecutionLaunch>,
    ) -> Result<(), OrchestrationError> {
        let mut seen = BTreeSet::new();
        for unit in units {
            if !seen.insert(unit.clone()) {
                return Err(OrchestrationError::DuplicateExecution(unit.to_string()));
            }
            if self.legs.contains_key(unit) {
                return Err(OrchestrationError::DuplicateExecution(unit.to_string()));
            }
            self.validate_launch(
                unit,
                launches
                    .get(unit)
                    .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?,
            )?;
            self.ensure_dependencies_and_barriers_ready(unit)?;
            for other in &seen {
                if other == unit {
                    continue;
                }
                if !self.explicitly_independent(unit, other) {
                    return Err(OrchestrationError::NotIndependent {
                        left: other.clone(),
                        right: unit.clone(),
                    });
                }
            }
            let compiled = self.unit(unit)?;
            if is_writer(compiled) {
                if let Some(owner) = self.active_writers.get(&compiled.subject_ref.to_string()) {
                    return Err(OrchestrationError::SharedWriterConflict {
                        subject: compiled.subject_ref.to_string(),
                        owner: owner.clone(),
                    });
                }
                for prior in &seen {
                    if prior == unit {
                        continue;
                    }
                    let prior_unit = self.unit(prior)?;
                    if is_writer(prior_unit) && prior_unit.subject_ref == compiled.subject_ref {
                        return Err(OrchestrationError::SharedWriterConflict {
                            subject: compiled.subject_ref.to_string(),
                            owner: prior.clone(),
                        });
                    }
                }
            }
        }
        self.preflight_retry_grants(units, launches)?;
        Ok(())
    }

    fn preflight_retry_grants(
        &self,
        units: &[WorkflowUnitRef],
        launches: &BTreeMap<WorkflowUnitRef, ExecutionLaunch>,
    ) -> Result<(), OrchestrationError> {
        let mut planned_spend = BTreeMap::<String, u32>::new();
        for unit in units {
            let Some(grant) = launches
                .get(unit)
                .and_then(|launch| launch.retry_grant.as_ref())
            else {
                continue;
            };
            let (attempts_allowed, attempts_spent, revoked) = self
                .retry_grants
                .get(&grant.grant_ref)
                .map(|current| {
                    (
                        current.attempts_allowed,
                        current.attempts_spent,
                        current.revoked,
                    )
                })
                .unwrap_or((grant.attempts_allowed, grant.attempts_spent, grant.revoked));
            if revoked {
                return Err(OrchestrationError::RetryRevoked(grant.grant_ref.clone()));
            }
            let spend = planned_spend.entry(grant.grant_ref.clone()).or_insert(0);
            if attempts_spent.saturating_add(*spend) >= attempts_allowed {
                return Err(OrchestrationError::RetryExhausted(grant.grant_ref.clone()));
            }
            *spend += 1;
        }
        Ok(())
    }

    fn validate_launch(
        &self,
        unit: &WorkflowUnitRef,
        launch: &ExecutionLaunch,
    ) -> Result<(), OrchestrationError> {
        self.unit(unit)?;
        if launch.execution_ref.trim().is_empty() {
            return Err(OrchestrationError::EmptyField("executionRef"));
        }
        if self.legs.values().any(|leg| {
            leg.execution_ref == launch.execution_ref
                || leg
                    .attempts
                    .iter()
                    .any(|attempt| attempt.execution_ref == launch.execution_ref)
        }) || self.reviewers.contains_key(&launch.execution_ref)
        {
            return Err(OrchestrationError::DuplicateExecution(
                launch.execution_ref.clone(),
            ));
        }
        if launch.disposition.demand.run_ref != self.run.reference().to_string() {
            return Err(OrchestrationError::InvalidExecutionDisposition {
                execution: launch.execution_ref.clone(),
                reason: "Execution Intelligence disposition names another Run".into(),
            });
        }
        if launch.disposition.demand.workflow_unit_ref.as_deref() != Some(unit.to_string().as_str())
        {
            return Err(OrchestrationError::InvalidExecutionDisposition {
                execution: launch.execution_ref.clone(),
                reason: "Execution Intelligence disposition names another workflow unit".into(),
            });
        }
        Ok(())
    }

    fn start_unit(
        &mut self,
        parent_journey_ref: String,
        unit: &WorkflowUnitRef,
        launch: ExecutionLaunch,
    ) -> Result<DelegationContract, OrchestrationError> {
        let compiled = self.unit(unit)?.clone();
        self.ensure_dependencies_and_barriers_ready(unit)?;
        if is_writer(&compiled) {
            let subject = compiled.subject_ref.to_string();
            if let Some(owner) = self.active_writers.get(&subject) {
                return Err(OrchestrationError::SharedWriterConflict {
                    subject,
                    owner: owner.clone(),
                });
            }
        }
        self.reserve_retry_grant(&launch)?;
        self.set_unit_state(unit, NodeState::Active)?;
        if is_writer(&compiled) {
            self.active_writers
                .insert(compiled.subject_ref.to_string(), unit.clone());
        }
        let mut delegation = delegation_for(
            &self.run,
            &parent_journey_ref,
            unit,
            &compiled,
            launch.retry_grant.as_ref().and_then(|grant| {
                self.retry_grants
                    .get(&grant.grant_ref)
                    .cloned()
                    .or_else(|| Some(grant.clone()))
            }),
        );
        delegation.basis_revision = self
            .subject_revisions
            .get(&delegation.subject_ref)
            .cloned()
            .ok_or(OrchestrationError::EmptyField("subjectRevision"))?;
        let execution_ref = launch.execution_ref;
        let attempt = ExecutionAttempt {
            execution_ref: execution_ref.clone(),
            delegation: delegation.clone(),
            status: LegStatus::Active,
            status_history: vec![LegStatus::Active],
            artifacts: Vec::new(),
            late_artifacts: Vec::new(),
            failure_reason: None,
        };
        let mut attempts = self
            .legs
            .get(unit)
            .map(|leg| leg.attempts.clone())
            .unwrap_or_default();
        attempts.push(attempt);
        self.legs.insert(
            unit.clone(),
            LegRecord {
                delegation: delegation.clone(),
                execution_ref,
                status: LegStatus::Active,
                status_history: vec![LegStatus::Active],
                attempts,
                artifacts: Vec::new(),
                late_artifacts: Vec::new(),
                failure_reason: None,
            },
        );
        Ok(delegation)
    }

    fn reserve_retry_grant(&mut self, launch: &ExecutionLaunch) -> Result<(), OrchestrationError> {
        let Some(incoming) = &launch.retry_grant else {
            return Ok(());
        };
        if self.retry_grants.contains_key(&incoming.grant_ref) {
            let grant = self.retry_grants.get_mut(&incoming.grant_ref).unwrap();
            if grant.revoked {
                return Err(OrchestrationError::RetryRevoked(grant.grant_ref.clone()));
            }
            if grant.remaining() == 0 {
                return Err(OrchestrationError::RetryExhausted(grant.grant_ref.clone()));
            }
            grant.attempts_spent += 1;
            return Ok(());
        }
        let mut grant = incoming.clone();
        if grant.revoked {
            return Err(OrchestrationError::RetryRevoked(grant.grant_ref));
        }
        if grant.remaining() == 0 {
            return Err(OrchestrationError::RetryExhausted(grant.grant_ref));
        }
        grant.attempts_spent += 1;
        self.retry_grants.insert(grant.grant_ref.clone(), grant);
        Ok(())
    }

    fn ensure_dependencies_and_barriers_ready(
        &self,
        unit: &WorkflowUnitRef,
    ) -> Result<(), OrchestrationError> {
        let compiled = self.unit(unit)?;
        for dependency in &compiled.dependencies {
            if !self
                .legs
                .get(dependency)
                .is_some_and(|leg| leg.status == LegStatus::Returned)
            {
                return Err(OrchestrationError::DependencyNotSatisfied(
                    dependency.to_string(),
                ));
            }
        }
        for barrier in &self.workflow.barriers {
            if barrier.releases.contains(unit)
                && self.barrier_reading(&barrier.key)?.state != BarrierState::Complete
            {
                return Err(OrchestrationError::BarrierNotSatisfied(barrier.key.clone()));
            }
        }
        Ok(())
    }

    fn explicitly_independent(&self, left: &WorkflowUnitRef, right: &WorkflowUnitRef) -> bool {
        let Some(left) = self
            .workflow
            .units
            .values()
            .find(|unit| &unit.reference == left)
        else {
            return false;
        };
        let Some(right) = self
            .workflow
            .units
            .values()
            .find(|unit| &unit.reference == right)
        else {
            return false;
        };
        left.independence_from.contains(&right.reference)
            || right.independence_from.contains(&left.reference)
    }

    fn unit(
        &self,
        reference: &WorkflowUnitRef,
    ) -> Result<&CompiledWorkflowUnit, OrchestrationError> {
        self.workflow
            .units
            .values()
            .find(|unit| &unit.reference == reference)
            .ok_or_else(|| OrchestrationError::UnknownUnit(reference.to_string()))
    }

    fn node_for_unit(&self, unit: &WorkflowUnitRef) -> Result<NodeId, OrchestrationError> {
        let reference = unit.to_string();
        self.run
            .map()
            .nodes()
            .values()
            .find(|node| {
                node.semantic_ref
                    .as_ref()
                    .is_some_and(|semantic| semantic.to_string() == reference)
            })
            .map(|node| node.id.clone())
            .ok_or(OrchestrationError::UnknownUnit(reference))
    }

    fn set_unit_state(
        &mut self,
        unit: &WorkflowUnitRef,
        state: NodeState,
    ) -> Result<(), OrchestrationError> {
        let node_id = self.node_for_unit(unit)?;
        let command = RunTopologyCommand {
            command_id: format!(
                "orchestration-state:{unit}:{}:{state:?}",
                self.run.revision().get()
            ),
            expected_revision: self.run.revision(),
            mutation: TopologyMutation::SetNodeState { node_id, state },
        };
        let outcome = self.run.apply_topology_command(&self.authority, command)?;
        if !matches!(outcome, CommandOutcome::Applied { .. }) {
            return Err(OrchestrationError::Run(RunContractError::InvalidCommandId));
        }
        Ok(())
    }

    pub fn detach(&mut self, unit: &WorkflowUnitRef) -> Result<(), OrchestrationError> {
        self.transition(unit, LegStatus::Detached, &[LegStatus::Active])
    }

    pub fn request_cancellation(
        &mut self,
        unit: &WorkflowUnitRef,
    ) -> Result<(), OrchestrationError> {
        self.transition(
            unit,
            LegStatus::CancelRequested,
            &[LegStatus::Active, LegStatus::Detached],
        )
    }

    pub fn accept_cancellation(
        &mut self,
        unit: &WorkflowUnitRef,
    ) -> Result<(), OrchestrationError> {
        self.transition(
            unit,
            LegStatus::CancellationAccepted,
            &[LegStatus::CancelRequested],
        )
    }

    pub fn record_process_termination(
        &mut self,
        unit: &WorkflowUnitRef,
    ) -> Result<(), OrchestrationError> {
        self.transition(
            unit,
            LegStatus::ProcessTerminated,
            &[LegStatus::CancellationAccepted],
        )
    }

    pub fn mark_quiescent(&mut self, unit: &WorkflowUnitRef) -> Result<(), OrchestrationError> {
        self.transition(
            unit,
            LegStatus::Quiescent,
            &[
                LegStatus::ProcessTerminated,
                LegStatus::CancellationAccepted,
                LegStatus::LateResult,
            ],
        )
    }

    pub fn fail(
        &mut self,
        unit: &WorkflowUnitRef,
        reason: impl Into<String>,
    ) -> Result<(), OrchestrationError> {
        let leg = self.leg_mut(unit)?;
        if !matches!(leg.status, LegStatus::Active | LegStatus::Detached) {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status: leg.status,
            });
        }
        set_current_status(leg, LegStatus::Failed);
        let reason = reason.into();
        leg.failure_reason = Some(reason.clone());
        leg.attempts.last_mut().unwrap().failure_reason = Some(reason);
        self.release_writer(unit);
        self.set_unit_state(unit, NodeState::Abandoned)
    }

    fn transition(
        &mut self,
        unit: &WorkflowUnitRef,
        next: LegStatus,
        allowed: &[LegStatus],
    ) -> Result<(), OrchestrationError> {
        let status = self
            .leg(unit)
            .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?
            .status;
        if !allowed.contains(&status) {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status,
            });
        }
        let release_writer = matches!(next, LegStatus::ProcessTerminated | LegStatus::Quiescent);
        let leg = self.leg_mut(unit)?;
        set_current_status(leg, next);
        if release_writer {
            self.release_writer(unit);
        }
        Ok(())
    }

    fn leg_mut(&mut self, unit: &WorkflowUnitRef) -> Result<&mut LegRecord, OrchestrationError> {
        self.legs
            .get_mut(unit)
            .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))
    }

    fn release_writer(&mut self, unit: &WorkflowUnitRef) {
        self.active_writers.retain(|_, owner| owner != unit);
    }

    pub fn return_artifact(
        &mut self,
        unit: &WorkflowUnitRef,
        artifact: ReturnedArtifact,
    ) -> Result<(), OrchestrationError> {
        artifact.validate()?;
        let expected_execution = self
            .leg(unit)
            .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?
            .execution_ref
            .clone();
        let expected_subject = self.unit(unit)?.subject_ref.to_string();
        let expected_basis = self
            .leg(unit)
            .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?
            .delegation
            .basis_revision
            .clone();
        if artifact.producing_execution_ref != expected_execution {
            return self.record_historical_late_artifact(unit, artifact);
        }
        let leg = self.leg_mut(unit)?;
        if !matches!(leg.status, LegStatus::Active | LegStatus::Detached) {
            return self.return_late_artifact(
                unit,
                artifact,
                expected_execution,
                expected_subject,
                expected_basis,
            );
        }
        if artifact.producing_execution_ref != expected_execution
            || artifact.subject_ref != expected_subject
            || artifact.subject_revision != expected_basis
        {
            return Err(OrchestrationError::InvalidArtifact(artifact.artifact_ref));
        }
        leg.artifacts.push(artifact.clone());
        set_current_status(leg, LegStatus::Returned);
        leg.attempts.last_mut().unwrap().artifacts.push(artifact);
        self.release_writer(unit);
        self.set_unit_state(unit, NodeState::Returned)
    }

    /// A result from an earlier attempt remains attached to that attempt. It
    /// cannot replace the current attempt, satisfy a barrier or release its writer.
    fn record_historical_late_artifact(
        &mut self,
        unit: &WorkflowUnitRef,
        artifact: ReturnedArtifact,
    ) -> Result<(), OrchestrationError> {
        let leg = self.leg_mut(unit)?;
        let attempt = leg
            .attempts
            .iter_mut()
            .find(|attempt| attempt.execution_ref == artifact.producing_execution_ref)
            .ok_or_else(|| OrchestrationError::InvalidArtifact(artifact.artifact_ref.clone()))?;
        if attempt.delegation.subject_ref != artifact.subject_ref
            || attempt.delegation.basis_revision != artifact.subject_revision
        {
            return Err(OrchestrationError::InvalidArtifact(artifact.artifact_ref));
        }
        if let Some(previous) = attempt
            .late_artifacts
            .iter()
            .find(|previous| previous.artifact_ref == artifact.artifact_ref)
        {
            return if previous == &artifact {
                Ok(())
            } else {
                Err(OrchestrationError::InvalidArtifact(artifact.artifact_ref))
            };
        }
        attempt.late_artifacts.push(artifact);
        Ok(())
    }

    fn return_late_artifact(
        &mut self,
        unit: &WorkflowUnitRef,
        artifact: ReturnedArtifact,
        expected_execution: String,
        expected_subject: String,
        expected_basis: String,
    ) -> Result<(), OrchestrationError> {
        let current_revision = self
            .subject_revisions
            .get(&artifact.subject_ref)
            .cloned()
            .unwrap_or_default();
        let leg = self.leg_mut(unit)?;
        if !matches!(
            leg.status,
            LegStatus::Quiescent | LegStatus::ProcessTerminated | LegStatus::CancellationAccepted
        ) {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status: leg.status,
            });
        }
        if artifact.producing_execution_ref != expected_execution
            || artifact.subject_ref != expected_subject
            || artifact.subject_revision != expected_basis
        {
            return Err(OrchestrationError::InvalidArtifact(artifact.artifact_ref));
        }
        leg.late_artifacts.push(artifact.clone());
        set_current_status(leg, LegStatus::LateResult);
        leg.attempts
            .last_mut()
            .unwrap()
            .late_artifacts
            .push(artifact);
        if current_revision != leg.delegation.basis_revision {
            return Ok(());
        }
        Ok(())
    }

    /// Explicitly incorporate a late result only after the current subject
    /// revision has been checked by the owner. A stale result remains evidence.
    pub fn incorporate_late_result(
        &mut self,
        unit: &WorkflowUnitRef,
    ) -> Result<(), OrchestrationError> {
        let current = self
            .subject_revisions
            .get(&self.unit(unit)?.subject_ref.to_string())
            .cloned()
            .unwrap_or_default();
        let leg = self.leg_mut(unit)?;
        let Some(artifact) = leg.late_artifacts.last().cloned() else {
            return Err(OrchestrationError::MissingEvidence(unit.to_string()));
        };
        if artifact.subject_revision != current {
            return Err(OrchestrationError::StaleLateResult {
                artifact: artifact.artifact_ref,
                current_revision: current,
            });
        }
        if !matches!(
            leg.status,
            LegStatus::LateResult | LegStatus::Quiescent | LegStatus::ProcessTerminated
        ) || !leg
            .status_history
            .iter()
            .any(|status| matches!(status, LegStatus::Quiescent | LegStatus::ProcessTerminated))
        {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status: leg.status,
            });
        }
        leg.artifacts.push(artifact.clone());
        set_current_status(leg, LegStatus::Returned);
        leg.attempts.last_mut().unwrap().artifacts.push(artifact);
        self.release_writer(unit);
        self.set_unit_state(unit, NodeState::Returned)
    }

    pub fn advance_subject(&mut self, subject_ref: impl Into<String>, revision: impl Into<String>) {
        self.subject_revisions
            .insert(subject_ref.into(), revision.into());
    }

    pub fn open_retry_grant(&mut self, grant: RetryGrant) -> Result<(), OrchestrationError> {
        if grant.attempts_spent > grant.attempts_allowed {
            return Err(OrchestrationError::InvalidRetryGrant);
        }
        if self.retry_grants.contains_key(&grant.grant_ref) {
            return Err(OrchestrationError::DuplicateRetryGrant(grant.grant_ref));
        }
        self.retry_grants.insert(grant.grant_ref.clone(), grant);
        Ok(())
    }

    pub fn revoke_retry_grant(&mut self, grant_ref: &str) -> Result<(), OrchestrationError> {
        let grant = self
            .retry_grants
            .get_mut(grant_ref)
            .ok_or_else(|| OrchestrationError::RetryExhausted(grant_ref.into()))?;
        grant.revoked = true;
        Ok(())
    }

    pub fn retry(
        &mut self,
        parent_journey_ref: impl Into<String>,
        unit: &WorkflowUnitRef,
        grant_ref: &str,
        launch: ExecutionLaunch,
    ) -> Result<DelegationContract, OrchestrationError> {
        if launch
            .retry_grant
            .as_ref()
            .is_none_or(|grant| grant.grant_ref != grant_ref)
        {
            return Err(OrchestrationError::RetryExhausted(grant_ref.into()));
        }
        let status = self
            .leg(unit)
            .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?
            .status;
        if !matches!(
            status,
            LegStatus::Failed | LegStatus::LateResult | LegStatus::Quiescent
        ) {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status,
            });
        }
        if status == LegStatus::LateResult
            && !self.leg(unit).is_some_and(|leg| {
                leg.status_history.iter().any(|status| {
                    matches!(status, LegStatus::Quiescent | LegStatus::ProcessTerminated)
                })
            })
        {
            return Err(OrchestrationError::InvalidTransition {
                unit: unit.clone(),
                status,
            });
        }
        self.validate_launch(unit, &launch)?;
        let mut staged = self.clone();
        staged.release_writer(unit);
        let delegation = staged.start_unit(parent_journey_ref.into(), unit, launch)?;
        *self = staged;
        Ok(delegation)
    }

    pub fn barrier_reading(&self, key: &str) -> Result<BarrierReading, OrchestrationError> {
        let barrier = self
            .workflow
            .barriers
            .iter()
            .find(|barrier| barrier.key == key)
            .ok_or_else(|| OrchestrationError::UnknownBarrier(key.into()))?;
        let mut returned_units = BTreeSet::new();
        let mut missing_units = BTreeSet::new();
        let mut failed_units = BTreeSet::new();
        for unit in &barrier.waits_for {
            match self.legs.get(unit).map(|leg| leg.status) {
                Some(LegStatus::Returned) => {
                    returned_units.insert(unit.clone());
                }
                Some(
                    LegStatus::Failed
                    | LegStatus::CancellationAccepted
                    | LegStatus::ProcessTerminated
                    | LegStatus::Quiescent
                    | LegStatus::LateResult,
                ) => {
                    failed_units.insert(unit.clone());
                }
                _ => {
                    missing_units.insert(unit.clone());
                }
            }
        }
        let state = if !failed_units.is_empty() {
            BarrierState::Failed
        } else if missing_units.is_empty() {
            BarrierState::Complete
        } else {
            BarrierState::Pending
        };
        Ok(BarrierReading {
            key: key.into(),
            state,
            required_units: barrier.waits_for.clone(),
            returned_units,
            missing_units,
            failed_units,
        })
    }

    pub fn whole_run_state(&self) -> WholeRunState {
        let mut failed = false;
        let mut complete = true;
        for unit in self.workflow.units.values() {
            match self.legs.get(&unit.reference).map(|leg| leg.status) {
                Some(LegStatus::Returned) => {}
                Some(
                    LegStatus::Failed
                    | LegStatus::CancellationAccepted
                    | LegStatus::ProcessTerminated
                    | LegStatus::Quiescent
                    | LegStatus::LateResult,
                ) => failed = true,
                _ => complete = false,
            }
        }
        if failed {
            WholeRunState::Failed
        } else if complete {
            WholeRunState::Complete
        } else {
            WholeRunState::Incomplete
        }
    }

    pub fn register_independent_reviewer(
        &mut self,
        execution_ref: impl Into<String>,
        disposition: &ExecutionDisposition,
        review_of: BTreeSet<WorkflowUnitRef>,
    ) -> Result<IndependentReviewer, OrchestrationError> {
        let execution_ref = execution_ref.into();
        if self.legs.values().any(|leg| {
            leg.attempts
                .iter()
                .any(|attempt| attempt.execution_ref == execution_ref)
        }) {
            self.self_review_attempts.insert(execution_ref.clone());
            return Err(OrchestrationError::ReviewerAlreadyProducer(execution_ref));
        }
        if disposition.demand.run_ref != self.run.reference().to_string()
            || review_of.is_empty()
            || !review_of.iter().all(|unit| self.unit(unit).is_ok())
        {
            return Err(OrchestrationError::ReviewerNotIndependent(execution_ref));
        }
        let producer_execution_refs = review_of
            .iter()
            .filter_map(|unit| self.legs.get(unit).map(|leg| leg.execution_ref.clone()))
            .collect::<BTreeSet<_>>();
        if producer_execution_refs.len() != review_of.len()
            || !producer_execution_refs.is_subset(&disposition.demand.independence_from)
        {
            return Err(OrchestrationError::ReviewerNotIndependent(execution_ref));
        }
        let reviewer = IndependentReviewer {
            execution_ref: execution_ref.clone(),
            disposition_run_ref: disposition.demand.run_ref.clone(),
            review_of,
            independent_from_execution_refs: producer_execution_refs,
        };
        self.reviewers.insert(execution_ref, reviewer.clone());
        Ok(reviewer)
    }

    pub fn synthesize(
        &mut self,
        synthesis_ref: impl Into<String>,
        barrier_key: &str,
        reviewer_execution_ref: &str,
        artifact_refs: BTreeSet<String>,
        evidence_refs: BTreeSet<String>,
        integrated_difference: impl Into<String>,
    ) -> Result<SynthesisRecord, OrchestrationError> {
        let synthesis_ref = synthesis_ref.into();
        if self.syntheses.contains_key(&synthesis_ref) {
            return Err(OrchestrationError::SynthesisAlreadyExists(synthesis_ref));
        }
        let barrier = self.barrier_reading(barrier_key)?;
        if barrier.state != BarrierState::Complete {
            return Err(OrchestrationError::BarrierIncomplete(barrier_key.into()));
        }
        let reviewer = self.reviewers.get(reviewer_execution_ref).ok_or_else(|| {
            OrchestrationError::ReviewerNotIndependent(reviewer_execution_ref.into())
        })?;
        if !barrier.required_units.is_subset(&reviewer.review_of) {
            return Err(OrchestrationError::ReviewerNotIndependent(
                reviewer_execution_ref.into(),
            ));
        }
        let mut available_artifacts = BTreeMap::new();
        let mut producer_refs = BTreeSet::new();
        let mut available_evidence = BTreeSet::new();
        for unit in &barrier.required_units {
            let leg = self
                .leg(unit)
                .ok_or_else(|| OrchestrationError::MissingLeg(unit.to_string()))?;
            for artifact in &leg.artifacts {
                producer_refs.insert(artifact.producing_execution_ref.clone());
                available_evidence.extend(artifact.evidence_refs.clone());
                available_artifacts.insert(artifact.artifact_ref.clone(), artifact);
            }
        }
        if producer_refs.contains(reviewer_execution_ref) {
            self.self_review_attempts
                .insert(reviewer_execution_ref.into());
            return Err(OrchestrationError::ReviewerAlreadyProducer(
                reviewer_execution_ref.into(),
            ));
        }
        if artifact_refs.is_empty()
            || !artifact_refs.is_subset(&available_artifacts.keys().cloned().collect())
            || evidence_refs.is_empty()
            || !evidence_refs.is_subset(&available_evidence)
        {
            self.concatenation_attempts.insert(barrier_key.into());
            return Err(OrchestrationError::ConcatenationRejected(
                barrier_key.into(),
            ));
        }
        let integrated_difference = integrated_difference.into();
        if integrated_difference.trim().is_empty() {
            self.concatenation_attempts.insert(barrier_key.into());
            return Err(OrchestrationError::ConcatenationRejected(
                barrier_key.into(),
            ));
        }
        let record = SynthesisRecord {
            synthesis_ref: synthesis_ref.clone(),
            barrier_key: barrier_key.into(),
            reviewer_execution_ref: reviewer_execution_ref.into(),
            artifact_refs,
            evidence_refs,
            integrated_difference,
        };
        self.syntheses.insert(synthesis_ref, record.clone());
        Ok(record)
    }

    pub fn admit_architecture_observation(
        &mut self,
        receipt: OwnerArchitectureObservationReceipt,
    ) -> Result<(), OrchestrationError> {
        if receipt.observation_ref.trim().is_empty()
            || receipt.owner_ref.trim().is_empty()
            || receipt.subject_ref.trim().is_empty()
            || receipt.evidence_refs.is_empty()
        {
            return Err(OrchestrationError::InvalidArchitectureObservation);
        }
        if self
            .owner_observations
            .contains_key(&receipt.observation_ref)
        {
            return Err(OrchestrationError::DuplicateArchitectureObservation(
                receipt.observation_ref,
            ));
        }
        self.owner_observations
            .insert(receipt.observation_ref.clone(), receipt);
        Ok(())
    }

    pub fn attempt_concatenated_synthesis(
        &mut self,
        barrier_key: impl Into<String>,
    ) -> Result<(), OrchestrationError> {
        let barrier_key = barrier_key.into();
        self.concatenation_attempts.insert(barrier_key.clone());
        Err(OrchestrationError::ConcatenationRejected(barrier_key))
    }

    pub fn architecture_integrity(&self) -> ArchitectureIntegrityReport {
        let incomplete_barriers = self
            .workflow
            .barriers
            .iter()
            .filter_map(|barrier| {
                self.barrier_reading(&barrier.key)
                    .ok()
                    .filter(|reading| reading.state != BarrierState::Complete)
                    .map(|_| barrier.key.clone())
            })
            .collect::<BTreeSet<_>>();
        let mut findings = self
            .owner_observations
            .values()
            .map(|receipt| receipt.finding.clone())
            .collect::<BTreeSet<_>>();
        let orphan_execution_refs = self
            .owner_observations
            .values()
            .filter(|receipt| receipt.finding == ArchitectureFinding::OrphanExecutionLeg)
            .map(|receipt| receipt.subject_ref.clone())
            .collect::<BTreeSet<_>>();
        if !incomplete_barriers.is_empty() {
            findings.insert(ArchitectureFinding::IncompleteBarrier);
        }
        if !self.concatenation_attempts.is_empty() {
            findings.insert(ArchitectureFinding::ConcatenationAsSynthesis);
        }
        if !self.self_review_attempts.is_empty() {
            findings.insert(ArchitectureFinding::ProducerSelfReview);
        }
        ArchitectureIntegrityReport {
            contract: "AG-006".into(),
            findings,
            orphan_execution_refs,
            incomplete_barriers,
            owner_observation_refs: self.owner_observations.keys().cloned().collect(),
        }
    }
}

fn set_current_status(leg: &mut LegRecord, status: LegStatus) {
    leg.status = status;
    leg.status_history.push(status);
    if let Some(attempt) = leg.attempts.last_mut() {
        attempt.status = status;
        attempt.status_history.push(status);
    }
}

fn delegation_for(
    run: &Run,
    parent_journey_ref: &str,
    unit: &WorkflowUnitRef,
    compiled: &CompiledWorkflowUnit,
    retry_grant: Option<RetryGrant>,
) -> DelegationContract {
    DelegationContract {
        parent_journey_ref: parent_journey_ref.into(),
        parent_run_ref: run.reference().to_string(),
        execution_unit_ref: unit.clone(),
        concern: compiled.developmental_concern.clone(),
        subject_ref: compiled.subject_ref.to_string(),
        basis_revision: compiled.basis_revision.clone(),
        agent_requirements: compiled.agent_requirements.clone(),
        permitted_effects: compiled.permitted_effects.clone(),
        verification_obligations: compiled.verification_obligations.clone(),
        return_address: compiled.return_address.clone(),
        stop_conditions: compiled.stop_conditions.clone(),
        retry_grant,
    }
}

fn is_writer(unit: &CompiledWorkflowUnit) -> bool {
    unit.permitted_effects.iter().any(|effect| {
        let effect = effect.to_ascii_lowercase();
        effect.starts_with("write:") || effect.starts_with("mutate:") || effect.contains("write ")
    })
}
