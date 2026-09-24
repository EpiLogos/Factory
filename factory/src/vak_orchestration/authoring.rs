//! Per-unit C′ authored through the registered QL Vāk domain adapter.
//!
//! QL owns the vocabulary (`@epilogos/ql-vak`, `ql.vak-workflow-types/v1`).
//! Factory owns this lowering into its native [`CPrimeExecutionBinding`] and
//! the coherence of each selected thread form with the compiled topology. A
//! lowered binding is an authored requirement for conduct: it resolves no
//! body, grants no authority and starts no attempt.
use super::{
    CPrimeExecutionBinding, ThreadForm, VakOrchestrationError, AIKIT_OPERATIVE_SCOPE_CONTRACT,
    QL_C_PRIME_PROFILE_CONTRACT,
};
use crate::core::run::WorkflowUnitRef;
use crate::workflow::{CompiledAgentRequirements, CompiledWorkflow, CompiledWorkflowUnit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Stable lowering identity named by the domain-adapter registry.
pub const C_PRIME_LOWERING: &str = "factory.vak-orchestration/v1#c-prime-execution-binding";

/// The authored QL `CPrime` fields, exactly. The Factory contract test proves
/// this equals the vendored QL declaration; unknown fields are refused.
pub const AUTHORED_C_PRIME_FIELDS: [&str; 14] = [
    "CPF",
    "authority",
    "CT",
    "CP",
    "CF",
    "CFP",
    "CS",
    "direction",
    "actor",
    "interpretation",
    "whole",
    "resolvePath",
    "contextResolution",
    "sources",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredInterpretation {
    #[serde(rename = "ref")]
    pub reference: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredVakComposition {
    #[serde(rename = "CPF")]
    pub participation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(rename = "CT")]
    pub content: String,
    #[serde(rename = "CP")]
    pub position: String,
    #[serde(rename = "CF")]
    pub frame: String,
    #[serde(rename = "CFP")]
    pub thread: String,
    #[serde(rename = "CS")]
    pub sequence: String,
    pub direction: String,
    pub actor: String,
    pub interpretation: AuthoredInterpretation,
    pub whole: String,
    #[serde(rename = "resolvePath")]
    pub resolve_path: String,
    #[serde(rename = "contextResolution")]
    pub context_resolution: String,
    pub sources: Vec<String>,
}

impl AuthoredVakComposition {
    /// The unit's own subject becomes the binding subject, so an authored C′
    /// cannot address a different concern from the unit that carries it.
    pub fn lower(self, subject_ref: &str) -> Result<CPrimeExecutionBinding, VakOrchestrationError> {
        let declared = self.sources.len();
        let source_refs = self.sources.into_iter().collect::<BTreeSet<_>>();
        if source_refs.len() != declared {
            return Err(VakOrchestrationError::InvalidField("sources"));
        }
        let binding = CPrimeExecutionBinding {
            contract: QL_C_PRIME_PROFILE_CONTRACT.into(),
            ql_binding_ref: self.interpretation.reference,
            ql_binding_revision: self.interpretation.revision,
            actor_ref: self.actor,
            whole_ref: self.whole,
            subject_ref: subject_ref.into(),
            participation: self.participation,
            content: self.content,
            position: self.position,
            frame: self.frame,
            thread: self.thread,
            sequence: self.sequence,
            direction: self.direction,
            ai_kit_scope_contract: AIKIT_OPERATIVE_SCOPE_CONTRACT.into(),
            ai_kit_resolve_path_ref: self.resolve_path,
            context_resolution_ref: self.context_resolution,
            source_refs,
            undertaking_authority_ref: self.authority,
        };
        binding.validate()?;
        Ok(binding)
    }
}

/// Registry lowering entry: authored JSON in, native binding JSON out.
pub fn lower_authored_composition(
    authored: serde_json::Value,
    subject_ref: &str,
) -> Result<serde_json::Value, String> {
    let authored: AuthoredVakComposition =
        serde_json::from_value(authored).map_err(|error| format!("C-prime: {error}"))?;
    let binding = authored
        .lower(subject_ref)
        .map_err(|error| error.to_string())?;
    serde_json::to_value(binding).map_err(|error| error.to_string())
}

impl CPrimeExecutionBinding {
    /// A unit-carried binding addresses that unit's subject and is performed
    /// by one of that unit's declared participants.
    pub fn validate_for_unit(
        &self,
        subject_ref: &str,
        participants: &CompiledAgentRequirements,
    ) -> Result<(), VakOrchestrationError> {
        self.validate()?;
        if self.subject_ref != subject_ref {
            return Err(VakOrchestrationError::SubjectMismatch(
                self.subject_ref.clone(),
            ));
        }
        if !(participants.agent_refs.contains(&self.actor_ref)
            || participants.agent_set_refs.contains(&self.actor_ref)
            || participants.agency_refs.contains(&self.actor_ref))
        {
            return Err(VakOrchestrationError::UndeclaredActor(
                self.actor_ref.clone(),
            ));
        }
        Ok(())
    }
}

/// A thread-topology refusal located at the unit whose composition fails.
#[derive(Debug)]
pub struct CompositionTopologyError {
    pub unit: String,
    pub error: VakOrchestrationError,
}
impl std::fmt::Display for CompositionTopologyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.unit, self.error)
    }
}
impl std::error::Error for CompositionTopologyError {}
fn at(unit: &str, error: VakOrchestrationError) -> Box<CompositionTopologyError> {
    Box::new(CompositionTopologyError {
        unit: unit.to_owned(),
        error,
    })
}

/// Every thread form other than a single voice or a sustained undertaking is
/// performed by several units under one QL interpretation over one whole.
/// Their compiled topology must carry that form: a chord is pairwise
/// independent, a melody consumes each predecessor's return, a fusion waits
/// at a barrier, a canon is nested. The failing unit key accompanies the error.
pub fn validate_workflow_compositions(
    workflow: &CompiledWorkflow,
) -> Result<(), Box<CompositionTopologyError>> {
    type Performance<'a> = (&'a str, &'a str, &'a str, ThreadForm);
    let mut performances: BTreeMap<Performance<'_>, Vec<&CompiledWorkflowUnit>> = BTreeMap::new();
    for unit in workflow.units.values() {
        let Some(binding) = &unit.composition else {
            continue;
        };
        let form = binding.thread_form();
        if matches!(form, ThreadForm::Single | ThreadForm::Sustained) {
            continue;
        }
        performances
            .entry((
                binding.ql_binding_ref.as_str(),
                binding.ql_binding_revision.as_str(),
                binding.whole_ref.as_str(),
                form,
            ))
            .or_default()
            .push(unit);
    }
    for ((_, _, _, form), units) in performances {
        let first = units[0].key.clone();
        let fail = |error| Err(at(&first, error));
        if units.len() < 2 {
            return fail(VakOrchestrationError::InvalidUnitShape);
        }
        let members = units
            .iter()
            .map(|unit| unit.reference.clone())
            .collect::<BTreeSet<_>>();
        match form {
            ThreadForm::Parallel | ThreadForm::Fusion => {
                for (index, left) in units.iter().enumerate() {
                    for right in units.iter().skip(index + 1) {
                        if !(left.independence_from.contains(&right.reference)
                            || right.independence_from.contains(&left.reference))
                        {
                            return Err(at(
                                &left.key,
                                VakOrchestrationError::NotIndependent {
                                    left: left.key.clone(),
                                    right: right.key.clone(),
                                },
                            ));
                        }
                    }
                }
                if form == ThreadForm::Fusion {
                    if units
                        .iter()
                        .any(|unit| unit.subject_ref != units[0].subject_ref)
                    {
                        return fail(VakOrchestrationError::FusionConcernMismatch);
                    }
                    if !workflow
                        .barriers
                        .iter()
                        .any(|barrier| members.is_subset(&barrier.waits_for))
                    {
                        return fail(VakOrchestrationError::MissingBarrier(format!(
                            "no barrier waits for every fused reading of {first}"
                        )));
                    }
                }
            }
            ThreadForm::Chain => validate_chain(&units, &members)?,
            ThreadForm::Nested => {
                let children = workflow
                    .nesting
                    .iter()
                    .filter(|edge| members.contains(&edge.parent) && members.contains(&edge.child))
                    .map(|edge| edge.child.clone())
                    .collect::<BTreeSet<_>>();
                if members.difference(&children).count() != 1 {
                    return fail(VakOrchestrationError::InvalidNestedTopology);
                }
            }
            ThreadForm::Single | ThreadForm::Sustained => {}
        }
    }
    Ok(())
}

fn validate_chain(
    units: &[&CompiledWorkflowUnit],
    members: &BTreeSet<WorkflowUnitRef>,
) -> Result<(), Box<CompositionTopologyError>> {
    let predecessors = |unit: &CompiledWorkflowUnit| {
        unit.dependencies
            .intersection(members)
            .cloned()
            .collect::<Vec<_>>()
    };
    let roots = units
        .iter()
        .filter(|unit| predecessors(unit).is_empty())
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        return Err(at(
            &units[0].key,
            VakOrchestrationError::InvalidChainInput(
                "a melody has exactly one first voice under its interpretation and whole".into(),
            ),
        ));
    }
    let mut current = *roots[0];
    let mut walked = 1;
    loop {
        let next = units
            .iter()
            .filter(|unit| unit.dependencies.contains(&current.reference))
            .collect::<Vec<_>>();
        match next.as_slice() {
            [] => break,
            [successor] => {
                if predecessors(successor).len() != 1 {
                    return Err(at(
                        &successor.key,
                        VakOrchestrationError::InvalidChain {
                            predecessor: current.key.clone(),
                            successor: successor.key.clone(),
                        },
                    ));
                }
                if !successor
                    .inputs
                    .iter()
                    .any(|input| input.predecessor == current.reference)
                {
                    return Err(at(
                        &successor.key,
                        VakOrchestrationError::MissingChainInput {
                            predecessor: current.key.clone(),
                            successor: successor.key.clone(),
                        },
                    ));
                }
                current = successor;
                walked += 1;
            }
            [left, ..] => {
                return Err(at(
                    &left.key,
                    VakOrchestrationError::InvalidChain {
                        predecessor: current.key.clone(),
                        successor: left.key.clone(),
                    },
                ))
            }
        }
    }
    if walked != units.len() {
        return Err(at(
            &units[0].key,
            VakOrchestrationError::InvalidChainInput(
                "every voice of a melody follows the previous voice".into(),
            ),
        ));
    }
    Ok(())
}
