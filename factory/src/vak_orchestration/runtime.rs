use super::{required, ThreadForm, VakConductPlan, VakOrchestrationError, VAK_ORCHESTRATION_CONTRACT};
use crate::core::run::WorkflowUnitRef;
use crate::execution_intelligence::ExecutionDisposition;
use crate::orchestration::{
    BarrierState, ExecutableOrchestration, ExecutionLaunch, IndependentReviewer, LegStatus,
    RetryGrant, SynthesisRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VakAttemptReading {
    pub unit_ref: WorkflowUnitRef,
    pub execution_ref: String,
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
    pub actor_ref: String,
    pub subject_ref: String,
    pub whole_ref: String,
    pub ql_binding_ref: String,
    pub ql_binding_revision: String,
    pub ai_kit_resolve_path_ref: String,
    pub context_resolution_ref: String,
    pub frame: String,
    pub thread: String,
    pub sequence: String,
    pub direction: String,
    pub musical_role: String,
    pub attempts: Vec<VakAttemptReading>,
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
    started: BTreeMap<WorkflowUnitRef, String>,
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
        let mut started = BTreeMap::new();
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
                    started.insert(unit, launch.execution_ref);
                }
            }
            _ => {
                if launches.len() != 1 || !launches.contains_key(&first) {
                    return Err(VakOrchestrationError::LaunchSetMismatch);
                }
                let launch = launches.get(&first).cloned().expect("validated launch");
                orchestration.start_serial(parent_journey_ref, &first, launch.clone())?;
                started.insert(first, launch.execution_ref);
            }
        }
        Ok(Self { plan, started })
    }

    pub fn plan(&self) -> &VakConductPlan {
        &self.plan
    }

    pub fn continue_sequence(
        &mut self,
        orchestration: &mut ExecutableOrchestration,
        parent_journey_ref: &str,
        launch: ExecutionLaunch,
    ) -> Result<WorkflowUnitRef, VakOrchestrationError> {
        let next_index = self.started.len();
        if next_index >= self.plan.units.len() {
            return Err(VakOrchestrationError::SequenceComplete);
        }
        match self.plan.binding.thread_form() {
            ThreadForm::Chain => {
                let previous = &self.plan.units[next_index - 1].unit_ref;
                if orchestration.leg(previous).map(|leg| leg.status) != Some(LegStatus::Returned) {
                    return Err(VakOrchestrationError::PredecessorNotReturned(
                        previous.to_string(),
                    ));
                }
            }
            ThreadForm::Nested => {
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
            }
            _ => return Err(VakOrchestrationError::WrongContinuationForm),
        }
        self.start_next(orchestration, parent_journey_ref, next_index, launch)
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
        orchestration.start_serial(parent_journey_ref, &next, launch.clone())?;
        self.started.insert(next.clone(), launch.execution_ref);
        Ok(next)
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
        let unit = &self.plan.units[0].unit_ref;
        orchestration.retry(parent_journey_ref, unit, grant_ref, launch.clone())?;
        self.started.insert(unit.clone(), launch.execution_ref);
        Ok(())
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
            let mut artifact_refs = BTreeSet::new();
            let mut late_artifact_refs = BTreeSet::new();
            let mut evidence_refs = BTreeSet::new();
            for artifact in &leg.artifacts {
                artifact_refs.insert(artifact.artifact_ref.clone());
                evidence_refs.extend(artifact.evidence_refs.clone());
            }
            for artifact in &leg.late_artifacts {
                late_artifact_refs.insert(artifact.artifact_ref.clone());
                evidence_refs.extend(artifact.evidence_refs.clone());
            }
            attempts.push(VakAttemptReading {
                unit_ref: scope.unit_ref.clone(),
                execution_ref: leg.execution_ref.clone(),
                status: leg.status,
                status_history: leg.status_history.clone(),
                artifact_refs,
                late_artifact_refs,
                evidence_refs,
                failure_reason: leg.failure_reason.clone(),
            });
        }
        Ok(VakPerformanceSnapshot {
            contract: VAK_ORCHESTRATION_CONTRACT.into(),
            performance_ref: self.plan.performance_ref.clone(),
            run_ref: orchestration.run().reference().to_string(),
            actor_ref: self.plan.binding.actor_ref.clone(),
            subject_ref: self.plan.binding.subject_ref.clone(),
            whole_ref: self.plan.binding.whole_ref.clone(),
            ql_binding_ref: self.plan.binding.ql_binding_ref.clone(),
            ql_binding_revision: self.plan.binding.ql_binding_revision.clone(),
            ai_kit_resolve_path_ref: self.plan.binding.ai_kit_resolve_path_ref.clone(),
            context_resolution_ref: self.plan.binding.context_resolution_ref.clone(),
            frame: self.plan.binding.frame.clone(),
            thread: self.plan.binding.thread.clone(),
            sequence: self.plan.binding.sequence.clone(),
            direction: self.plan.binding.direction.clone(),
            musical_role: self.plan.binding.thread_form().musical_role().into(),
            attempts,
        })
    }
}

pub fn sustained_retry_grant(
    grant_ref: impl Into<String>,
    attempts_allowed: u32,
) -> Result<RetryGrant, VakOrchestrationError> {
    Ok(RetryGrant::new(grant_ref, attempts_allowed)?)
}
