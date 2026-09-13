use super::{required, VakChainMaterial, VakConductPlan, VakOrchestrationError};
use crate::attempt_runtime::{AttemptStart, AttemptTrackingFact, SituatedExecutionDisposition};
use crate::core::run::WorkflowUnitRef;
use crate::orchestration::RetryGrant;
use crate::workflow::CompiledWorkflow;
use std::collections::BTreeSet;

pub const VAK_SCOPE_TRACKING_KIND: &str = "factory.vak-scope/v1";
pub const VAK_CHAIN_INPUT_TRACKING_KIND: &str = "factory.vak-chain-input/v1";

/// Project one already-validated C′/AIKit unit scope into the existing attempt
/// tracking ledger. The ledger remains Factory's native heterogeneous correlation
/// owner; this creates no Vāk store and grants no execution authority.
pub fn vak_scope_tracking(
    plan: &VakConductPlan,
    unit_ref: &WorkflowUnitRef,
) -> Result<AttemptTrackingFact, VakOrchestrationError> {
    let scope = plan
        .units
        .iter()
        .find(|scope| &scope.unit_ref == unit_ref)
        .ok_or_else(|| VakOrchestrationError::UnknownPerformanceUnit(unit_ref.to_string()))?;
    required(&plan.performance_ref, "performanceRef")?;
    let mut evidence_refs = BTreeSet::from([
        plan.performance_ref.clone(),
        plan.binding.actor_ref.clone(),
        scope.whole_ref.clone(),
        plan.binding.ql_binding_ref.clone(),
        scope.ai_kit_resolve_path_ref.clone(),
        scope.context_resolution_ref.clone(),
        unit_ref.to_string(),
    ]);
    evidence_refs.extend(scope.source_refs.clone());
    Ok(AttemptTrackingFact {
        fact_ref: format!("{}:{}:scope", plan.performance_ref, unit_ref),
        kind: VAK_SCOPE_TRACKING_KIND.into(),
        owner_ref: "factory".into(),
        subject_ref: scope.subject_ref.clone(),
        source_revision: plan.binding.ql_binding_revision.clone(),
        evidence_refs,
    })
}

/// The native owner handoff already serializes `SituatedExecutionDisposition` as
/// the bounded attempt context. Carry the persisted fact identities and their
/// exact source/evidence refs into that existing context so execution consumes
/// the same lineage that Factory retains. This does not replace the tracking facts.
pub fn carry_vak_tracking_into_disposition(
    disposition: &mut SituatedExecutionDisposition,
    tracking: &[AttemptTrackingFact],
) -> Result<(), VakOrchestrationError> {
    for fact in tracking {
        required(&fact.fact_ref, "trackingFactRef")?;
        required(&fact.owner_ref, "trackingOwnerRef")?;
        required(&fact.subject_ref, "trackingSubjectRef")?;
        required(&fact.source_revision, "trackingSourceRevision")?;
        disposition.context_refs.insert(fact.fact_ref.clone());
        disposition.context_refs.insert(fact.subject_ref.clone());
        disposition
            .context_refs
            .insert(format!("source-revision:{}", fact.source_revision));
        disposition.context_refs.extend(fact.evidence_refs.clone());
    }
    Ok(())
}

/// Prepare the existing durable Factory attempt carrier from one validated Vāk
/// unit. The same tracking facts are persisted on the attempt and projected into
/// the situated disposition that the native owner handoff already serializes.
/// This is the production join from C′/Resolve lineage to commissioned execution,
/// not a new dispatch path.
#[allow(clippy::too_many_arguments)]
pub fn vak_attempt_start(
    workflow: &CompiledWorkflow,
    plan: &VakConductPlan,
    unit_ref: &WorkflowUnitRef,
    attempt_ref: impl Into<String>,
    task_ref: impl Into<String>,
    mut disposition: SituatedExecutionDisposition,
    retry_grant: Option<RetryGrant>,
    chain_material: Option<&VakChainMaterial>,
) -> Result<AttemptStart, VakOrchestrationError> {
    plan.validate(workflow)?;
    let attempt_ref = attempt_ref.into();
    let task_ref = task_ref.into();
    required(&attempt_ref, "attemptRef")?;
    required(&task_ref, "taskRef")?;
    let mut tracking = vec![vak_scope_tracking(plan, unit_ref)?];
    if let Some(material) = chain_material {
        if &material.successor_unit_ref != unit_ref {
            return Err(VakOrchestrationError::InvalidPredecessorMaterial(
                material.predecessor_unit_ref.to_string(),
            ));
        }
        tracking.push(material.tracking_fact(&plan.performance_ref)?);
    }
    carry_vak_tracking_into_disposition(&mut disposition, &tracking)?;
    Ok(AttemptStart {
        attempt_ref,
        task_ref,
        workflow_unit_ref: unit_ref.clone(),
        disposition,
        retry_grant,
        tracking,
    })
}

impl VakChainMaterial {
    /// Preserve the selected predecessor result as an input fact on the successor
    /// attempt. Only material admitted by `continue_chain` can produce this fact.
    pub fn tracking_fact(
        &self,
        performance_ref: &str,
    ) -> Result<AttemptTrackingFact, VakOrchestrationError> {
        required(performance_ref, "performanceRef")?;
        let mut evidence_refs = self.evidence_refs.clone();
        evidence_refs.extend(self.artifact_refs.clone());
        evidence_refs.insert(self.predecessor_execution_ref.clone());
        evidence_refs.insert(self.predecessor_unit_ref.to_string());
        evidence_refs.insert(self.successor_unit_ref.to_string());
        Ok(AttemptTrackingFact {
            fact_ref: format!(
                "{}:{}:from:{}",
                performance_ref, self.successor_unit_ref, self.predecessor_execution_ref
            ),
            kind: VAK_CHAIN_INPUT_TRACKING_KIND.into(),
            owner_ref: "factory".into(),
            subject_ref: self.subject_ref.clone(),
            source_revision: self.subject_revision.clone(),
            evidence_refs,
        })
    }
}
