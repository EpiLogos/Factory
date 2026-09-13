use super::{
    required, CPrimeExecutionBinding, ThreadForm, VakOrchestrationError, VAK_ORCHESTRATION_CONTRACT,
};
use crate::core::run::WorkflowUnitRef;
use crate::workflow::{CompiledWorkflow, CompiledWorkflowUnit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakUnitScope {
    pub unit_ref: WorkflowUnitRef,
    pub whole_ref: String,
    pub subject_ref: String,
    pub ai_kit_resolve_path_ref: String,
    pub context_resolution_ref: String,
    pub source_refs: BTreeSet<String>,
}
impl VakUnitScope {
    fn validate(
        &self,
        workflow: &CompiledWorkflow,
        binding: &CPrimeExecutionBinding,
    ) -> Result<(), VakOrchestrationError> {
        for (value, field) in [
            (&self.whole_ref, "unit.wholeRef"),
            (&self.subject_ref, "unit.subjectRef"),
            (&self.ai_kit_resolve_path_ref, "unit.aiKitResolvePathRef"),
            (&self.context_resolution_ref, "unit.contextResolutionRef"),
        ] {
            required(value, field)?;
        }
        if self.source_refs.is_empty() {
            return Err(VakOrchestrationError::InvalidField("unit.sourceRefs"));
        }
        let native = unit(workflow, &self.unit_ref)?;
        if native.subject_ref.to_string() != self.subject_ref {
            return Err(VakOrchestrationError::SubjectMismatch(
                self.unit_ref.to_string(),
            ));
        }
        if !self.source_refs.is_subset(&binding.source_refs) {
            return Err(VakOrchestrationError::SourceScopeWidening(
                self.unit_ref.to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakChainInputBinding {
    pub predecessor_unit_ref: WorkflowUnitRef,
    pub successor_unit_ref: WorkflowUnitRef,
    pub receiving_context_ref: String,
}
impl VakChainInputBinding {
    fn validate(&self) -> Result<(), VakOrchestrationError> {
        required(
            &self.receiving_context_ref,
            "chainInput.receivingContextRef",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VakConductPlan {
    pub contract: String,
    pub performance_ref: String,
    pub binding: CPrimeExecutionBinding,
    pub units: Vec<VakUnitScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain_inputs: Vec<VakChainInputBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_condition_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion_barrier_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_performance_ref: Option<String>,
}
impl VakConductPlan {
    pub fn validate(&self, workflow: &CompiledWorkflow) -> Result<(), VakOrchestrationError> {
        if self.contract != VAK_ORCHESTRATION_CONTRACT {
            return Err(VakOrchestrationError::WrongContract(
                "Factory Vāk orchestration",
            ));
        }
        required(&self.performance_ref, "performanceRef")?;
        self.binding.validate()?;
        if self.units.is_empty() || self.units.len() > 4096 {
            return Err(VakOrchestrationError::InvalidUnitShape);
        }
        let mut ids = BTreeSet::new();
        for scope in &self.units {
            if !ids.insert(scope.unit_ref.clone()) {
                return Err(VakOrchestrationError::DuplicateUnit(
                    scope.unit_ref.to_string(),
                ));
            }
            scope.validate(workflow, &self.binding)?;
        }
        if self.binding.thread_form() != ThreadForm::Chain && !self.chain_inputs.is_empty() {
            return Err(VakOrchestrationError::UnexpectedField("chainInputs"));
        }
        match self.binding.thread_form() {
            ThreadForm::Single => {
                exactly(self.units.len(), 1)?;
                self.no_extras(false, false, false, false)?;
            }
            ThreadForm::Parallel => {
                at_least(self.units.len(), 2)?;
                pairwise_independent(workflow, &self.units)?;
                self.no_extras(false, false, false, false)?;
            }
            ThreadForm::Chain => {
                at_least(self.units.len(), 2)?;
                if self.chain_inputs.len() != self.units.len() - 1 {
                    return Err(VakOrchestrationError::InvalidChainInput(
                        "a melody requires one authored input binding per transition".into(),
                    ));
                }
                for pair in self.units.windows(2) {
                    if !unit(workflow, &pair[1].unit_ref)?
                        .dependencies
                        .contains(&pair[0].unit_ref)
                    {
                        return Err(VakOrchestrationError::InvalidChain {
                            predecessor: pair[0].unit_ref.to_string(),
                            successor: pair[1].unit_ref.to_string(),
                        });
                    }
                    let input = self
                        .chain_input(&pair[0].unit_ref, &pair[1].unit_ref)
                        .ok_or_else(|| VakOrchestrationError::MissingChainInput {
                            predecessor: pair[0].unit_ref.to_string(),
                            successor: pair[1].unit_ref.to_string(),
                        })?;
                    input.validate()?;
                }
                self.no_extras(false, false, false, false)?;
            }
            ThreadForm::Fusion => {
                at_least(self.units.len(), 2)?;
                pairwise_independent(workflow, &self.units)?;
                if self
                    .units
                    .iter()
                    .any(|scope| scope.subject_ref != self.binding.subject_ref)
                {
                    return Err(VakOrchestrationError::FusionConcernMismatch);
                }
                let key = self
                    .fusion_barrier_ref
                    .as_deref()
                    .ok_or(VakOrchestrationError::InvalidField("fusionBarrierRef"))?;
                let barrier = workflow
                    .barriers
                    .iter()
                    .find(|barrier| barrier.key == key)
                    .ok_or_else(|| VakOrchestrationError::MissingBarrier(key.into()))?;
                let planned = self
                    .units
                    .iter()
                    .map(|scope| scope.unit_ref.clone())
                    .collect::<BTreeSet<_>>();
                if !planned.is_subset(&barrier.waits_for) {
                    return Err(VakOrchestrationError::FusionBarrierMismatch);
                }
                self.no_extras(false, false, true, false)?;
            }
            ThreadForm::Sustained => {
                exactly(self.units.len(), 1)?;
                required(
                    self.continuation_ref.as_deref().unwrap_or_default(),
                    "continuationRef",
                )?;
                required(
                    self.stop_condition_ref.as_deref().unwrap_or_default(),
                    "stopConditionRef",
                )?;
                self.no_extras(true, true, false, false)?;
            }
            ThreadForm::Nested => {
                at_least(self.units.len(), 2)?;
                let planned = self
                    .units
                    .iter()
                    .map(|scope| scope.unit_ref.clone())
                    .collect::<BTreeSet<_>>();
                let root = &self.units[0].unit_ref;
                if workflow
                    .nesting
                    .iter()
                    .any(|edge| edge.child == *root && planned.contains(&edge.parent))
                {
                    return Err(VakOrchestrationError::InvalidNestedTopology);
                }
                for scope in self.units.iter().skip(1) {
                    if !workflow
                        .nesting
                        .iter()
                        .any(|edge| edge.child == scope.unit_ref && planned.contains(&edge.parent))
                    {
                        return Err(VakOrchestrationError::InvalidNestedTopology);
                    }
                }
                required(
                    self.parent_performance_ref.as_deref().unwrap_or_default(),
                    "parentPerformanceRef",
                )?;
                self.no_extras(false, false, false, true)?;
            }
        }
        Ok(())
    }

    pub fn chain_input(
        &self,
        predecessor: &WorkflowUnitRef,
        successor: &WorkflowUnitRef,
    ) -> Option<&VakChainInputBinding> {
        self.chain_inputs.iter().find(|input| {
            &input.predecessor_unit_ref == predecessor && &input.successor_unit_ref == successor
        })
    }

    fn no_extras(
        &self,
        continuation: bool,
        stop: bool,
        fusion: bool,
        parent: bool,
    ) -> Result<(), VakOrchestrationError> {
        for (allowed, value, field) in [
            (continuation, &self.continuation_ref, "continuationRef"),
            (stop, &self.stop_condition_ref, "stopConditionRef"),
            (fusion, &self.fusion_barrier_ref, "fusionBarrierRef"),
            (parent, &self.parent_performance_ref, "parentPerformanceRef"),
        ] {
            if !allowed && value.is_some() {
                return Err(VakOrchestrationError::UnexpectedField(field));
            }
        }
        Ok(())
    }
}

fn exactly(actual: usize, expected: usize) -> Result<(), VakOrchestrationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(VakOrchestrationError::InvalidUnitShape)
    }
}
fn at_least(actual: usize, expected: usize) -> Result<(), VakOrchestrationError> {
    if actual >= expected {
        Ok(())
    } else {
        Err(VakOrchestrationError::InvalidUnitShape)
    }
}
pub(crate) fn unit<'a>(
    workflow: &'a CompiledWorkflow,
    reference: &WorkflowUnitRef,
) -> Result<&'a CompiledWorkflowUnit, VakOrchestrationError> {
    workflow
        .units
        .values()
        .find(|unit| &unit.reference == reference)
        .ok_or_else(|| VakOrchestrationError::UnknownUnit(reference.to_string()))
}
fn pairwise_independent(
    workflow: &CompiledWorkflow,
    units: &[VakUnitScope],
) -> Result<(), VakOrchestrationError> {
    for (index, left) in units.iter().enumerate() {
        for right in units.iter().skip(index + 1) {
            let left = unit(workflow, &left.unit_ref)?;
            let right = unit(workflow, &right.unit_ref)?;
            if !(left.independence_from.contains(&right.reference)
                || right.independence_from.contains(&left.reference))
            {
                return Err(VakOrchestrationError::NotIndependent {
                    left: left.reference.to_string(),
                    right: right.reference.to_string(),
                });
            }
        }
    }
    Ok(())
}
