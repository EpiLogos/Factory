use super::{required, NativeVakPerformance, VakOrchestrationError, VakPerformanceSnapshot};
use crate::core::run::{
    RunRef, RunThoughtConsumptionCommand, RunThoughtId, RunThoughtLifecycle, RunThoughtOutcome,
    ThoughtConsumption, ThoughtConsumptionInput, ThoughtConsumptionSources, ThoughtFieldError,
    ThoughtSource, ThoughtSourceObservation, ThoughtUse,
};
use crate::journey::JourneyReturn;
use crate::journey_praxis::{
    JourneyMethodProofCorrelation, JourneyPraxisReturn, AIKIT_METHOD_SCHEMA,
};
use crate::orchestration::ExecutableOrchestration;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakThoughtConsumptionRequest {
    pub command_id: String,
    pub consumption_id: RunThoughtId,
    pub consumer_ref: String,
    pub working_field: ThoughtSource,
    pub inputs: Vec<ThoughtConsumptionInput>,
    #[serde(default)]
    pub human_response: Vec<ThoughtSource>,
    pub assessment: ThoughtSource,
    pub uses: Vec<ThoughtUse>,
    pub retention_policy: ThoughtSource,
    pub resulting_lifecycle: RunThoughtLifecycle,
}

impl VakPerformanceSnapshot {
    /// Actual artifacts returned by native attempts, including late results that
    /// remain evidence without being silently promoted into current output.
    pub fn thought_evidence_sources(&self) -> BTreeSet<ThoughtSource> {
        let mut sources = BTreeSet::new();
        for attempt in &self.attempts {
            for reference in attempt
                .artifact_refs
                .iter()
                .chain(attempt.late_artifact_refs.iter())
            {
                sources.insert(ThoughtSource {
                    owner: "factory".into(),
                    reference: reference.clone(),
                    revision: attempt.execution_ref.clone(),
                });
            }
        }
        sources
    }

    /// Factory Return over the existing Journey owner. Recognition deliberately
    /// remains absent until its native owner has made that separate determination.
    pub fn journey_return(
        &self,
        return_ref: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<JourneyReturn, VakOrchestrationError> {
        let return_ref = return_ref.into();
        let summary = summary.into();
        required(&return_ref, "returnRef")?;
        required(&summary, "returnSummary")?;
        let run_ref = self
            .run_ref
            .parse::<RunRef>()
            .map_err(|_| VakOrchestrationError::InvalidRunRef)?;
        let evidence_refs = self.evidence_refs();
        if evidence_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPerformanceEvidence);
        }
        let mut basis_refs = BTreeSet::from([
            self.performance_ref.clone(),
            self.actor_ref.clone(),
            self.whole_ref.clone(),
            self.subject_ref.clone(),
            self.ql_binding_ref.clone(),
            format!("ql-revision:{}", self.ql_binding_revision),
            self.ai_kit_resolve_path_ref.clone(),
            self.context_resolution_ref.clone(),
            self.workflow_source_ref.clone(),
            format!("workflow-revision:{}", self.workflow_source_revision),
            format!("workflow-digest:{}", self.workflow_source_digest),
        ]);
        basis_refs.extend(self.source_refs.clone());
        Ok(JourneyReturn {
            return_ref,
            run_refs: vec![run_ref],
            basis_refs: basis_refs.into_iter().collect(),
            evidence_refs: evidence_refs.into_iter().collect(),
            recognition_ref: None,
            summary,
        })
    }

    /// Correlate this exact performed occasion to an AIKit Method-classified Skill
    /// through the existing Journey praxis contract. It does not recognise or
    /// register the Skill; the native AIKit proof/Recognition path remains owner.
    pub fn journey_praxis_return(
        &self,
        method_ref: impl Into<String>,
        method_revision: impl Into<String>,
        activity_refs: Vec<String>,
        return_refs: Vec<String>,
        proof: Option<JourneyMethodProofCorrelation>,
    ) -> Result<JourneyPraxisReturn, VakOrchestrationError> {
        let method_ref = method_ref.into();
        let method_revision = method_revision.into();
        required(&method_ref, "methodRef")?;
        required(&method_revision, "methodRevision")?;
        let run_ref = self
            .run_ref
            .parse::<RunRef>()
            .map_err(|_| VakOrchestrationError::InvalidRunRef)?;
        let evidence_refs = self.evidence_refs();
        if evidence_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPerformanceEvidence);
        }
        let mut body_condition_refs = BTreeSet::from([
            self.performance_ref.clone(),
            self.actor_ref.clone(),
            self.whole_ref.clone(),
            self.subject_ref.clone(),
            self.ql_binding_ref.clone(),
            self.ai_kit_resolve_path_ref.clone(),
            self.context_resolution_ref.clone(),
            self.workflow_source_ref.clone(),
        ]);
        body_condition_refs.extend(self.source_refs.clone());
        for attempt in &self.attempts {
            body_condition_refs.insert(attempt.model_ref.clone());
            body_condition_refs.insert(attempt.provider_ref.clone());
            body_condition_refs.insert(attempt.whole_ref.clone());
            body_condition_refs.extend(attempt.source_refs.clone());
        }
        Ok(JourneyPraxisReturn {
            run_ref,
            method_contract: AIKIT_METHOD_SCHEMA.into(),
            method_ref,
            method_revision,
            context_resolution_ref: self.context_resolution_ref.clone(),
            body_condition_refs: body_condition_refs.into_iter().collect(),
            activity_refs,
            evidence_refs: evidence_refs.into_iter().collect(),
            return_refs,
            proof,
        })
    }
}

struct VakPerformanceSources<'a, P> {
    actual: BTreeSet<ThoughtSource>,
    receipt: ThoughtSource,
    downstream: &'a P,
}

impl<P: ThoughtConsumptionSources> ThoughtConsumptionSources for VakPerformanceSources<'_, P> {
    fn observe(
        &self,
        source: &ThoughtSource,
    ) -> Result<ThoughtSourceObservation, ThoughtFieldError> {
        if self.actual.contains(source) {
            return Ok(ThoughtSourceObservation::Current {
                source: source.clone(),
                receipt: self.receipt.clone(),
            });
        }
        self.downstream.observe(source)
    }
}

impl NativeVakPerformance {
    /// Consume selected Run cognition against evidence from this actual performed
    /// occasion. Central NOW/T/T′ source observations and useful outputs are still
    /// checked by their native owners; only exact Factory artifacts are observed
    /// here from the performance itself.
    pub fn consume_thoughts<P: ThoughtConsumptionSources>(
        &self,
        orchestration: &mut ExecutableOrchestration,
        request: VakThoughtConsumptionRequest,
        sources: &P,
    ) -> Result<RunThoughtOutcome, VakOrchestrationError> {
        required(&request.command_id, "thoughtConsumptionCommandId")?;
        required(&request.consumer_ref, "thoughtConsumerRef")?;
        let snapshot = self.snapshot(orchestration)?;
        if !snapshot.settled() {
            return Err(VakOrchestrationError::PerformanceNotSettled);
        }
        let actual = snapshot.thought_evidence_sources();
        if actual.is_empty() {
            return Err(VakOrchestrationError::MissingPerformanceEvidence);
        }
        let actual_evidence = actual.iter().cloned().collect::<Vec<_>>();
        let observer = VakPerformanceSources {
            actual,
            receipt: ThoughtSource {
                owner: "factory".into(),
                reference: snapshot.performance_ref.clone(),
                revision: format!("run-revision:{}", snapshot.run_revision),
            },
            downstream: sources,
        };
        let consumption = ThoughtConsumption {
            consumption_id: request.consumption_id,
            run_ref: orchestration.run().reference().clone(),
            consumer_ref: request.consumer_ref,
            working_field: request.working_field,
            inputs: request.inputs,
            actual_evidence,
            human_response: request.human_response,
            assessment: request.assessment,
            uses: request.uses,
            retention_policy: request.retention_policy,
            resulting_lifecycle: request.resulting_lifecycle,
        };
        let command = RunThoughtConsumptionCommand {
            command_id: request.command_id,
            expected_revision: orchestration.run().revision(),
            consumption,
        };
        Ok(orchestration.consume_run_thoughts(command, &observer)?)
    }
}
