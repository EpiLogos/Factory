use super::{required, VakChainMaterial, VakConductPlan, VakOrchestrationError};
use crate::attempt_runtime::AttemptTrackingFact;
use crate::core::run::WorkflowUnitRef;
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
