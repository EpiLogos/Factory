use super::{
    required, ThreadForm, VakConductPlan, VakOrchestrationError, VAK_ORCHESTRATION_CONTRACT,
};
use crate::core::run::WorkflowUnitRef;
use crate::execution_intelligence::ExecutionDisposition;
use crate::orchestration::{
    BarrierState, ExecutableOrchestration, ExecutionLaunch, IndependentReviewer, LegStatus,
    RetryGrant, SynthesisRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakLaunchReading {
    pub model_ref: String,
    pub provider_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakChainMaterial {
    pub predecessor_unit_ref: WorkflowUnitRef,
    pub predecessor_execution_ref: String,
    pub successor_unit_ref: WorkflowUnitRef,
    pub subject_ref: String,
    pub subject_revision: String,
    pub receiving_context_ref: String,
    pub artifact_refs: BTreeSet<String>,
    pub evidence_refs: BTreeSet<String>,
    pub semantic_differences: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakSustainedStopObservation {
    pub stop_condition_ref: String,
    pub native_stop_conditions: String,
    pub owner_ref: String,
    pub source_revision: String,
    pub evidence_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VakAttemptReading {
    pub unit_ref: WorkflowUnitRef,
    pub attempt_index: usize,
    pub current: bool,
    pub execution_ref: String,
    pub actor_ref: String,
    pub whole_ref: String,
    pub subject_ref: String,
    pub ql_binding_ref: String,
    pub ql_binding_revision: String,
    pub ai_kit_resolve_path_ref: String,
    pub context_resolution_ref: String,
    pub source_refs: BTreeSet<String>,
    pub model_ref: String,
    pub provider_ref: String,
    pub status: LegStatus,
    pub status_history: Vec<LegStatus>,
    pub artifact_refs: BTreeSet<String>,
    pub late_artifact_refs: BTreeSet<String>,
    pub evidence_refs: BTreeSet<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VakPerformanceSnapshot {
    pub contract: String,
    pub performance_ref: String,
    pub run_ref: String,
    pub run_revision: u64,
    pub workflow_source_ref: String,
    pub workflow_source_revision: String,
    pub workflow_source_digest: String,
    pub actor_ref: String,
    pub subject_ref: String,
    pub whole_ref: String,
    pub ql_binding_ref: String,
    pub ql_binding_revision: String,
    pub ai_kit_resolve_path_ref: String,
    pub context_resolution_ref: String,
    pub source_refs: BTreeSet<String>,
    pub frame: String,
    pub thread: String,
    pub sequence: String,
    pub direction: String,
    pub musical_role: String,
    pub attempts: Vec<VakAttemptReading>,
    pub chain_inputs: Vec<VakChainMaterial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustained_stop: Option<VakSustainedStopObservation>,
}
impl VakPerformanceSnapshot {
    pub fn evidence_refs(&self) -> BTreeSet<String> {
        self.attempts
            .iter()
            .flat_map(|attempt| attempt.evidence_refs.iter().cloned())
            .collect()
    }

    pub fn has_actual_execution(&self) -> bool {
        !self.attempts.is_empty()
    }

    pub fn settled(&self) -> bool {
        self.attempts.iter().all(|attempt| {
            matches!(
                attempt.status,
                LegStatus::Returned
                    | LegStatus::Failed
                    | LegStatus::CancellationAccepted
                    | LegStatus::ProcessTerminated
                    | LegStatus::Quiescent
                    | LegStatus::LateResult
            )
        })
    }
}

/// Correlation around one existing native orchestration. It has no independent
/// mutation authority: every state change delegates to ExecutableOrchestration.
pub struct NativeVakPerformance {
    plan: VakConductPlan,
    started_units: BTreeSet<WorkflowUnitRef>,
    launches: BTreeMap<String, VakLaunchReading>,
    chain_inputs: BTreeMap<WorkflowUnitRef, VakChainMaterial>,
    sustained_stop: Option<VakSustainedStopObservation>,
}
impl NativeVakPerformance {
    pub fn start(
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        plan: VakConductPlan,
        launches: BTreeMap<WorkflowUnitRef, ExecutionLaunch>,
    ) -> Result<Self, VakOrchestrationError> {
        plan.validate(orchestration.workflow())?;
        required(parent_journey_ref, "parentJourneyRef")?;
        let first = plan.units[0].unit_ref.clone();
        let mut started_units = BTreeSet::new();
        let mut launch_readings = BTreeMap::new();
        match plan.binding.thread_form() {
            ThreadForm::Parallel | ThreadForm::Fusion => {
                let expected = plan
                    .units
                    .iter()
                    .map(|scope| scope.unit_ref.clone())
                    .collect::<BTreeSet<_>>();
                if launches.keys().cloned().collect::<BTreeSet<_>>() != expected {
                    return Err(VakOrchestrationError::LaunchSetMismatch);
                }
                let units = plan
                    .units
                    .iter()
                    .map(|scope| scope.unit_ref.clone())
                    .collect::<Vec<_>>();
                orchestration.fork(parent_journey_ref, &units, launches.clone())?;
                for (unit, launch) in launches {
                    started_units.insert(unit);
                    launch_readings.insert(launch.execution_ref.clone(), launch_reading(&launch));
                }
            }
            _ => {
                if launches.len() != 1 || !launches.contains_key(&first) {
                    return Err(VakOrchestrationError::LaunchSetMismatch);
                }
                let launch = launches.get(&first).cloned().expect("validated launch");
                orchestration.start_serial(parent_journey_ref, &first, launch.clone())?;
                started_units.insert(first);
                launch_readings.insert(launch.execution_ref.clone(), launch_reading(&launch));
            }
        }
        Ok(Self {
            plan,
            started_units,
            launches: launch_readings,
            chain_inputs: BTreeMap::new(),
            sustained_stop: None,
        })
    }

    pub fn plan(&self) -> &VakConductPlan {
        &self.plan
    }

    pub fn continue_chain(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        selected_artifact_refs: BTreeSet<String>,
        launch: ExecutionLaunch,
    ) -> Result<VakChainMaterial, VakOrchestrationError> {
        if self.plan.binding.thread_form() != ThreadForm::Chain {
            return Err(VakOrchestrationError::WrongContinuationForm);
        }
        let next_index = self.next_index()?;
        let previous = &self.plan.units[next_index - 1].unit_ref;
        let successor = self.plan.units[next_index].unit_ref.clone();
        let receiving_context_ref = self
            .plan
            .chain_input(previous, &successor)
            .ok_or_else(|| VakOrchestrationError::MissingChainInput {
                predecessor: previous.to_string(),
                successor: successor.to_string(),
            })?
            .receiving_context_ref
            .clone();
        let leg = orchestration
            .leg(previous)
            .ok_or_else(|| VakOrchestrationError::PredecessorNotReturned(previous.to_string()))?;
        if leg.status != LegStatus::Returned {
            return Err(VakOrchestrationError::PredecessorNotReturned(
                previous.to_string(),
            ));
        }
        if selected_artifact_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPredecessorMaterial(
                previous.to_string(),
            ));
        }
        let available = leg
            .artifacts
            .iter()
            .map(|artifact| artifact.artifact_ref.clone())
            .collect::<BTreeSet<_>>();
        if !selected_artifact_refs.is_subset(&available) {
            return Err(VakOrchestrationError::InvalidPredecessorMaterial(
                previous.to_string(),
            ));
        }
        let mut evidence_refs = BTreeSet::new();
        let mut semantic_differences = BTreeSet::new();
        for artifact in leg
            .artifacts
            .iter()
            .filter(|artifact| selected_artifact_refs.contains(&artifact.artifact_ref))
        {
            if artifact.producing_execution_ref != leg.execution_ref {
                return Err(VakOrchestrationError::InvalidPredecessorMaterial(
                    previous.to_string(),
                ));
            }
            evidence_refs.extend(artifact.evidence_refs.clone());
            semantic_differences.insert(artifact.semantic_difference.clone());
        }
        if evidence_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPredecessorMaterial(
                previous.to_string(),
            ));
        }
        let material = VakChainMaterial {
            predecessor_unit_ref: previous.clone(),
            predecessor_execution_ref: leg.execution_ref.clone(),
            successor_unit_ref: successor.clone(),
            subject_ref: leg.delegation.subject_ref.clone(),
            subject_revision: leg.delegation.basis_revision.clone(),
            receiving_context_ref,
            artifact_refs: selected_artifact_refs,
            evidence_refs,
            semantic_differences,
        };
        self.start_next(orchestration, parent_journey_ref, next_index, launch)?;
        self.chain_inputs.insert(successor, material.clone());
        Ok(material)
    }

    pub fn continue_nested(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        launch: ExecutionLaunch,
    ) -> Result<WorkflowUnitRef, VakOrchestrationError> {
        if self.plan.binding.thread_form() != ThreadForm::Nested {
            return Err(VakOrchestrationError::WrongContinuationForm);
        }
        let next_index = self.next_index()?;
        let next = &self.plan.units[next_index].unit_ref;
        let parent = orchestration
            .workflow()
            .nesting
            .iter()
            .find(|edge| edge.child == *next)
            .map(|edge| edge.parent.clone())
            .ok_or(VakOrchestrationError::InvalidNestedTopology)?;
        if orchestration.leg(&parent).is_none() {
            return Err(VakOrchestrationError::NestedParentNotStarted(
                parent.to_string(),
            ));
        }
        self.start_next(orchestration, parent_journey_ref, next_index, launch)
    }

    fn next_index(&self) -> Result<usize, VakOrchestrationError> {
        let next_index = self.started_units.len();
        if next_index >= self.plan.units.len() {
            Err(VakOrchestrationError::SequenceComplete)
        } else {
            Ok(next_index)
        }
    }

    fn start_next(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        index: usize,
        launch: ExecutionLaunch,
    ) -> Result<WorkflowUnitRef, VakOrchestrationError> {
        let next = self.plan.units[index].unit_ref.clone();
        let next_ref = next.to_string();
        if launch.disposition.demand.workflow_unit_ref.as_deref() != Some(next_ref.as_str()) {
            return Err(VakOrchestrationError::LaunchUnitMismatch);
        }
        let execution_ref = launch.execution_ref.clone();
        let reading = launch_reading(&launch);
        orchestration.start_serial(parent_journey_ref, &next, launch)?;
        self.started_units.insert(next.clone());
        self.launches.insert(execution_ref, reading);
        Ok(next)
    }

    pub fn retry_unit(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        unit: &WorkflowUnitRef,
        grant_ref: &str,
        launch: ExecutionLaunch,
    ) -> Result<(), VakOrchestrationError> {
        if !self.plan.units.iter().any(|scope| &scope.unit_ref == unit) {
            return Err(VakOrchestrationError::UnknownPerformanceUnit(
                unit.to_string(),
            ));
        }
        if self.plan.binding.thread_form() == ThreadForm::Sustained && self.sustained_stop.is_some()
        {
            return Err(VakOrchestrationError::SustainedStopAlreadySatisfied);
        }
        let unit_ref = unit.to_string();
        if launch.disposition.demand.workflow_unit_ref.as_deref() != Some(unit_ref.as_str()) {
            return Err(VakOrchestrationError::LaunchUnitMismatch);
        }
        let execution_ref = launch.execution_ref.clone();
        let reading = launch_reading(&launch);
        orchestration.retry(parent_journey_ref, unit, grant_ref, launch)?;
        self.launches.insert(execution_ref, reading);
        Ok(())
    }

    pub fn resume_sustained(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        grant_ref: &str,
        launch: ExecutionLaunch,
    ) -> Result<(), VakOrchestrationError> {
        if self.plan.binding.thread_form() != ThreadForm::Sustained {
            return Err(VakOrchestrationError::WrongContinuationForm);
        }
        if self.sustained_stop.is_some() {
            return Err(VakOrchestrationError::SustainedStopAlreadySatisfied);
        }
        let unit = self.plan.units[0].unit_ref.clone();
        self.retry_unit(orchestration, parent_journey_ref, &unit, grant_ref, launch)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stop_sustained(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        stop_condition_ref: &str,
        owner_ref: &str,
        source_revision: &str,
        satisfied: bool,
        evidence_refs: BTreeSet<String>,
    ) -> Result<VakSustainedStopObservation, VakOrchestrationError> {
        if self.plan.binding.thread_form() != ThreadForm::Sustained {
            return Err(VakOrchestrationError::WrongContinuationForm);
        }
        if self.sustained_stop.is_some() {
            return Err(VakOrchestrationError::SustainedStopAlreadySatisfied);
        }
        if !satisfied {
            return Err(VakOrchestrationError::SustainedStopNotSatisfied);
        }
        let expected = self
            .plan
            .stop_condition_ref
            .as_deref()
            .ok_or(VakOrchestrationError::InvalidField("stopConditionRef"))?;
        if stop_condition_ref != expected {
            return Err(VakOrchestrationError::SustainedStopNotSatisfied);
        }
        required(stop_condition_ref, "stopConditionRef")?;
        required(owner_ref, "stopOwnerRef")?;
        required(source_revision, "stopSourceRevision")?;
        if evidence_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPerformanceEvidence);
        }
        let unit = &self.plan.units[0].unit_ref;
        let leg = orchestration
            .leg(unit)
            .ok_or(VakOrchestrationError::SustainedStopNotApplicable)?;
        if !matches!(leg.status, LegStatus::Active | LegStatus::Detached) {
            return Err(VakOrchestrationError::SustainedStopNotApplicable);
        }
        let native_stop_conditions = leg.delegation.stop_conditions.clone();
        required(&native_stop_conditions, "nativeStopConditions")?;
        orchestration.request_cancellation(unit)?;
        orchestration.accept_cancellation(unit)?;
        orchestration.record_process_termination(unit)?;
        orchestration.mark_quiescent(unit)?;
        let observation = VakSustainedStopObservation {
            stop_condition_ref: stop_condition_ref.into(),
            native_stop_conditions,
            owner_ref: owner_ref.into(),
            source_revision: source_revision.into(),
            evidence_refs,
        };
        self.sustained_stop = Some(observation.clone());
        Ok(observation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn synthesize_fusion(
        &self,
        orchestration: &mut ExecutableOrchestration,
        reviewer_execution_ref: impl Into<String>,
        reviewer_disposition: &ExecutionDisposition,
        synthesis_ref: impl Into<String>,
        artifact_refs: BTreeSet<String>,
        evidence_refs: BTreeSet<String>,
        integrated_difference: impl Into<String>,
    ) -> Result<(IndependentReviewer, SynthesisRecord), VakOrchestrationError> {
        if self.plan.binding.thread_form() != ThreadForm::Fusion {
            return Err(VakOrchestrationError::WrongFusionForm);
        }
        let barrier = self
            .plan
            .fusion_barrier_ref
            .as_deref()
            .ok_or(VakOrchestrationError::InvalidField("fusionBarrierRef"))?;
        if orchestration.barrier_reading(barrier)?.state != BarrierState::Complete {
            return Err(VakOrchestrationError::FusionNotReady);
        }
        let reviewer_execution_ref = reviewer_execution_ref.into();
        let review_of = self
            .plan
            .units
            .iter()
            .map(|scope| scope.unit_ref.clone())
            .collect();
        let reviewer = orchestration.register_independent_reviewer(
            reviewer_execution_ref.clone(),
            reviewer_disposition,
            review_of,
        )?;
        let synthesis = orchestration.synthesize(
            synthesis_ref,
            barrier,
            &reviewer_execution_ref,
            artifact_refs,
            evidence_refs,
            integrated_difference,
        )?;
        Ok((reviewer, synthesis))
    }

    pub fn snapshot(
        &self,
        orchestration: &ExecutableOrchestration,
    ) -> Result<VakPerformanceSnapshot, VakOrchestrationError> {
        let mut attempts = Vec::new();
        for scope in &self.plan.units {
            let Some(leg) = orchestration.leg(&scope.unit_ref) else {
                continue;
            };
            for (attempt_index, attempt) in leg.attempts.iter().enumerate() {
                let launch = self.launches.get(&attempt.execution_ref).ok_or_else(|| {
                    VakOrchestrationError::MissingLaunchEvidence(attempt.execution_ref.clone())
                })?;
                let mut artifact_refs = BTreeSet::new();
                let mut late_artifact_refs = BTreeSet::new();
                let mut evidence_refs = BTreeSet::new();
                for artifact in &attempt.artifacts {
                    artifact_refs.insert(artifact.artifact_ref.clone());
                    evidence_refs.extend(artifact.evidence_refs.clone());
                }
                for artifact in &attempt.late_artifacts {
                    late_artifact_refs.insert(artifact.artifact_ref.clone());
                    evidence_refs.extend(artifact.evidence_refs.clone());
                }
                attempts.push(VakAttemptReading {
                    unit_ref: scope.unit_ref.clone(),
                    attempt_index,
                    current: attempt_index + 1 == leg.attempts.len(),
                    execution_ref: attempt.execution_ref.clone(),
                    actor_ref: self.plan.binding.actor_ref.clone(),
                    whole_ref: scope.whole_ref.clone(),
                    subject_ref: attempt.delegation.subject_ref.clone(),
                    ql_binding_ref: self.plan.binding.ql_binding_ref.clone(),
                    ql_binding_revision: self.plan.binding.ql_binding_revision.clone(),
                    ai_kit_resolve_path_ref: scope.ai_kit_resolve_path_ref.clone(),
                    context_resolution_ref: scope.context_resolution_ref.clone(),
                    source_refs: scope.source_refs.clone(),
                    model_ref: launch.model_ref.clone(),
                    provider_ref: launch.provider_ref.clone(),
                    status: attempt.status,
                    status_history: attempt.status_history.clone(),
                    artifact_refs,
                    late_artifact_refs,
                    evidence_refs,
                    failure_reason: attempt.failure_reason.clone(),
                });
            }
        }
        Ok(VakPerformanceSnapshot {
            contract: VAK_ORCHESTRATION_CONTRACT.into(),
            performance_ref: self.plan.performance_ref.clone(),
            run_ref: orchestration.run().reference().to_string(),
            run_revision: orchestration.run().revision().get(),
            workflow_source_ref: orchestration.workflow().source.reference.to_string(),
            workflow_source_revision: orchestration.workflow().source.revision.clone(),
            workflow_source_digest: orchestration.workflow().source.digest.clone(),
            actor_ref: self.plan.binding.actor_ref.clone(),
            subject_ref: self.plan.binding.subject_ref.clone(),
            whole_ref: self.plan.binding.whole_ref.clone(),
            ql_binding_ref: self.plan.binding.ql_binding_ref.clone(),
            ql_binding_revision: self.plan.binding.ql_binding_revision.clone(),
            ai_kit_resolve_path_ref: self.plan.binding.ai_kit_resolve_path_ref.clone(),
            context_resolution_ref: self.plan.binding.context_resolution_ref.clone(),
            source_refs: self.plan.binding.source_refs.clone(),
            frame: self.plan.binding.frame.clone(),
            thread: self.plan.binding.thread.clone(),
            sequence: self.plan.binding.sequence.clone(),
            direction: self.plan.binding.direction.clone(),
            musical_role: self.plan.binding.thread_form().musical_role().into(),
            attempts,
            chain_inputs: self
                .plan
                .units
                .iter()
                .filter_map(|scope| self.chain_inputs.get(&scope.unit_ref).cloned())
                .collect(),
            sustained_stop: self.sustained_stop.clone(),
        })
    }
}

fn launch_reading(launch: &ExecutionLaunch) -> VakLaunchReading {
    VakLaunchReading {
        model_ref: launch.disposition.selection.model_ref.clone(),
        provider_ref: launch.disposition.selection.provider_ref.clone(),
    }
}

pub fn sustained_retry_grant(
    grant_ref: impl Into<String>,
    attempts_allowed: u32,
) -> Result<RetryGrant, VakOrchestrationError> {
    Ok(RetryGrant::new(grant_ref, attempts_allowed)?)
}
