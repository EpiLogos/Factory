use super::{
    CPrimeExecutionBinding, VakConductPlan, VakOrchestrationError, VakPerformanceSnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZStage {
    Composed,
    Performing,
    Recorded,
    Reheard,
    Recomposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VakZCycle {
    pub cycle_ref: String,
    pub performance_ref: String,
    pub stage: ZStage,
    pub record_evidence_refs: BTreeSet<String>,
    pub rehear_evidence_refs: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_ql_binding_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_ql_binding_revision: Option<String>,
}
impl VakZCycle {
    pub fn compose(
        cycle_ref: impl Into<String>,
        plan: &VakConductPlan,
    ) -> Result<Self, VakOrchestrationError> {
        let cycle_ref = cycle_ref.into();
        super::required(&cycle_ref, "cycleRef")?;
        super::required(&plan.performance_ref, "performanceRef")?;
        Ok(Self {
            cycle_ref,
            performance_ref: plan.performance_ref.clone(),
            stage: ZStage::Composed,
            record_evidence_refs: BTreeSet::new(),
            rehear_evidence_refs: BTreeSet::new(),
            next_ql_binding_ref: None,
            next_ql_binding_revision: None,
        })
    }

    pub fn performing(
        &mut self,
        snapshot: &VakPerformanceSnapshot,
    ) -> Result<(), VakOrchestrationError> {
        if self.stage != ZStage::Composed
            || !snapshot.has_actual_execution()
            || snapshot.performance_ref != self.performance_ref
        {
            return Err(VakOrchestrationError::InvalidZTransition);
        }
        self.stage = ZStage::Performing;
        Ok(())
    }

    pub fn record(
        &mut self,
        snapshot: &VakPerformanceSnapshot,
    ) -> Result<(), VakOrchestrationError> {
        if self.stage != ZStage::Performing
            || !snapshot.settled()
            || snapshot.performance_ref != self.performance_ref
        {
            return Err(VakOrchestrationError::InvalidZTransition);
        }
        self.record_evidence_refs = snapshot.evidence_refs();
        if self.record_evidence_refs.is_empty() {
            return Err(VakOrchestrationError::MissingPerformanceEvidence);
        }
        self.stage = ZStage::Recorded;
        Ok(())
    }

    pub fn rehear(&mut self, evidence_refs: BTreeSet<String>) -> Result<(), VakOrchestrationError> {
        if self.stage != ZStage::Recorded
            || evidence_refs.is_empty()
            || !evidence_refs.is_subset(&self.record_evidence_refs)
        {
            return Err(VakOrchestrationError::InvalidZTransition);
        }
        self.rehear_evidence_refs = evidence_refs;
        self.stage = ZStage::Reheard;
        Ok(())
    }

    pub fn recompose(
        &mut self,
        current: &CPrimeExecutionBinding,
        next: &CPrimeExecutionBinding,
    ) -> Result<(), VakOrchestrationError> {
        if self.stage != ZStage::Reheard {
            return Err(VakOrchestrationError::InvalidZTransition);
        }
        next.validate()?;
        if current.ql_binding_ref == next.ql_binding_ref
            && current.ql_binding_revision == next.ql_binding_revision
        {
            return Err(VakOrchestrationError::RecompositionDidNotChangeBinding);
        }
        self.next_ql_binding_ref = Some(next.ql_binding_ref.clone());
        self.next_ql_binding_revision = Some(next.ql_binding_revision.clone());
        self.stage = ZStage::Recomposed;
        Ok(())
    }
}
