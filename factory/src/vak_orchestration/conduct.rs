//! Attempt-time conduct derived from a commissioned workflow's unit C′s (#217).
//!
//! A compiled workflow that carries lowered `CPrimeExecutionBinding`s already
//! passed `validate_workflow_compositions`, which groups those units by QL
//! binding, whole and thread form and checks each form against the compiled
//! topology. Derivation walks the same grouping and lowers each group into the
//! exact `VakConductPlan` shape the explicitly supplied plans use — no second
//! plan model, no new authority: the derived plans still go through
//! `VakConductPlan::validate` and `NativeVakPerformance` before any attempt.
//!
//! Derivation is total over compile-validated topologies and deterministic
//! from unit content alone:
//!
//! - **CFP0 single voice** — one plan per unit, serial conduct of that leg.
//! - **CFP1 chord** — one plan over the interpretation's units in ascending
//!   unit-ref order, forked at start; no extras.
//! - **CFP2 melody** — one plan ordered by the validated chain walk (single
//!   root, each successor consumes its predecessor), with one authored input
//!   binding per transition carrying the successor's authored
//!   `receivingContextRef`.
//! - **CFP3 fusion** — one plan over the independent readings of the one
//!   concern, with the barrier that waits for every planned reading (the
//!   tightest such barrier, tie-broken by key) as `fusionBarrierRef`.
//! - **CFP4 sustained** — one plan per unit whose continuation and authored
//!   stop condition are named `continuation:<unit-ref>` and
//!   `stop:<unit-ref>`; the authored stop text itself stays on the native leg
//!   and is read back by `stop_sustained` as `nativeStopConditions`.
//! - **CFP5 canon** — one plan rooted at the member no other member nests,
//!   then members in the order `continue_nested` performs them: each next
//!   member's runtime parent (the first nesting edge for that child, which
//!   compilation keeps sorted) is already placed. The plan names the
//!   workflow-level performance of its interpretation as
//!   `parentPerformanceRef`.
//!
//! Units without a composition are invisible to derivation; they stay on the
//! plain bounded-coordination path.
//!
//! Recorded derivation choices (the program canon fixes the grouping law but
//! not these): a performance is conducted under its **opening voice's
//! binding** — the binding of the unit the plan performs first — with
//! `sourceRefs` widened to the union of the performance's units, because a
//! plan carries one interpretation-level lineage while each unit scope keeps
//! its own whole, resolve path, context resolution and sources. Per-voice
//! content, position, frame, sequence, direction and actor remain on the
//! units' own lowered bindings; the performance-level snapshot reads the
//! opening voice, exactly as the explicitly supplied plans do.
use super::{
    CPrimeExecutionBinding, ThreadForm, VakChainInputBinding, VakConductPlan,
    VakOrchestrationError, VakUnitScope, VAK_ORCHESTRATION_CONTRACT,
};
use crate::core::run::WorkflowUnitRef;
use crate::workflow::{CompiledWorkflow, CompiledWorkflowUnit};
use std::collections::{BTreeMap, BTreeSet};

/// The conduct plans one compiled workflow derives: one per C′ performance,
/// ordered by `performanceRef`. Empty when no unit carries a composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VakConductPlans {
    plans: Vec<VakConductPlan>,
}

impl VakConductPlans {
    /// The derived plans, in deterministic `performanceRef` order.
    pub fn plans(&self) -> &[VakConductPlan] {
        &self.plans
    }

    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// The plan a launch set begins: fork forms require exactly their full
    /// unit set, every other form exactly its opening unit. Any other launch
    /// set conducts nothing here and stays on the default path.
    pub fn plan_for_units(&self, units: &BTreeSet<WorkflowUnitRef>) -> Option<&VakConductPlan> {
        self.plans
            .iter()
            .find(|plan| match plan.binding.thread_form() {
                ThreadForm::Parallel | ThreadForm::Fusion => {
                    let planned = plan
                        .units
                        .iter()
                        .map(|scope| scope.unit_ref.clone())
                        .collect::<BTreeSet<_>>();
                    &planned == units
                }
                _ => plan
                    .units
                    .first()
                    .is_some_and(|opening| *units == BTreeSet::from([opening.unit_ref.clone()])),
            })
    }
}

/// Derive the conduct plans of a compiled workflow from its units' C′
/// compositions. Deterministic; refuses nothing that compile-time validation
/// admitted except a canon whose continuation parent is not a member of its
/// own interpretation (such a cluster cannot be performed nested).
pub fn derive_conduct_plans(
    workflow: &CompiledWorkflow,
) -> Result<VakConductPlans, VakOrchestrationError> {
    type Member<'a> = (&'a CompiledWorkflowUnit, &'a CPrimeExecutionBinding);
    // The compile-time grouping law, exactly: QL binding, revision, whole and
    // thread form. Single and sustained voices are per-unit performances and
    // were skipped there only because one unit needs no topology check.
    let mut groups: BTreeMap<(String, String, String, ThreadForm), Vec<Member<'_>>> =
        BTreeMap::new();
    let mut singles: Vec<Member<'_>> = Vec::new();
    for compiled in workflow.units.values() {
        let Some(binding) = &compiled.composition else {
            continue;
        };
        match binding.thread_form() {
            ThreadForm::Single | ThreadForm::Sustained => singles.push((compiled, binding)),
            form => groups
                .entry((
                    binding.ql_binding_ref.clone(),
                    binding.ql_binding_revision.clone(),
                    binding.whole_ref.clone(),
                    form,
                ))
                .or_default()
                .push((compiled, binding)),
        }
    }
    let mut plans = Vec::new();
    for members in groups.into_values() {
        plans.push(performance(workflow, &members)?);
    }
    for (compiled, binding) in singles {
        plans.push(performance(workflow, &[(compiled, binding)])?);
    }
    plans.sort_by(|left, right| left.performance_ref.cmp(&right.performance_ref));
    for plan in &plans {
        plan.validate(workflow)?;
    }
    Ok(VakConductPlans { plans })
}

/// Assemble one performance plan from its members. The members are ordered
/// first; the opening voice's binding carries the performance, with the union
/// of the members' sources as its lineage; every member keeps its own unit
/// scope.
fn performance<'a>(
    workflow: &CompiledWorkflow,
    members: &[(&'a CompiledWorkflowUnit, &'a CPrimeExecutionBinding)],
) -> Result<VakConductPlan, VakOrchestrationError> {
    let ordered = performance_order(workflow, members)?;
    let bindings = ordered
        .iter()
        .map(|compiled| {
            compiled
                .composition
                .as_ref()
                .ok_or_else(|| VakOrchestrationError::UnknownUnit(compiled.key.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let opening = bindings[0];
    let mut source_refs = BTreeSet::new();
    for binding in &bindings {
        source_refs.extend(binding.source_refs.iter().cloned());
    }
    let binding = CPrimeExecutionBinding {
        source_refs,
        ..opening.clone()
    };
    let form = binding.thread_form();
    let units = ordered
        .iter()
        .zip(bindings.iter())
        .map(|(compiled, binding)| VakUnitScope {
            unit_ref: compiled.reference.clone(),
            whole_ref: binding.whole_ref.clone(),
            subject_ref: compiled.subject_ref.to_string(),
            ai_kit_resolve_path_ref: binding.ai_kit_resolve_path_ref.clone(),
            context_resolution_ref: binding.context_resolution_ref.clone(),
            source_refs: binding.source_refs.clone(),
        })
        .collect();
    let thread = binding.thread.clone();
    let interpretation = format!(
        "{}:{}@{}:{}",
        workflow.workflow_key,
        opening.ql_binding_ref,
        opening.ql_binding_revision,
        opening.whole_ref,
    );
    let mut plan = VakConductPlan {
        contract: VAK_ORCHESTRATION_CONTRACT.into(),
        performance_ref: format!("performance:{interpretation}:{thread}"),
        binding,
        units,
        chain_inputs: Vec::new(),
        continuation_ref: None,
        stop_condition_ref: None,
        fusion_barrier_ref: None,
        parent_performance_ref: None,
    };
    match form {
        ThreadForm::Chain => {
            for pair in ordered.windows(2) {
                let (predecessor, successor) = (pair[0], pair[1]);
                let authored = successor
                    .inputs
                    .iter()
                    .find(|input| input.predecessor == predecessor.reference)
                    .ok_or_else(|| VakOrchestrationError::MissingChainInput {
                        predecessor: predecessor.reference.to_string(),
                        successor: successor.reference.to_string(),
                    })?;
                plan.chain_inputs.push(VakChainInputBinding {
                    predecessor_unit_ref: predecessor.reference.clone(),
                    successor_unit_ref: successor.reference.clone(),
                    receiving_context_ref: authored.receiving_context_ref.clone(),
                });
            }
        }
        ThreadForm::Fusion => {
            let planned = ordered
                .iter()
                .map(|compiled| compiled.reference.clone())
                .collect::<BTreeSet<_>>();
            let barrier = workflow
                .barriers
                .iter()
                .filter(|barrier| planned.is_subset(&barrier.waits_for))
                .min_by_key(|barrier| (barrier.waits_for.len(), barrier.key.clone()))
                .ok_or_else(|| {
                    VakOrchestrationError::MissingBarrier(format!(
                        "no barrier waits for every fused reading of {}",
                        ordered[0].key
                    ))
                })?;
            plan.fusion_barrier_ref = Some(barrier.key.clone());
        }
        ThreadForm::Sustained => {
            let sustained = ordered[0].reference.to_string();
            plan.continuation_ref = Some(format!("continuation:{sustained}"));
            plan.stop_condition_ref = Some(format!("stop:{sustained}"));
        }
        ThreadForm::Nested => {
            plan.parent_performance_ref = Some(format!("performance:{interpretation}:parent"));
        }
        ThreadForm::Single | ThreadForm::Parallel => {}
    }
    Ok(plan)
}

/// The performance order of a plan's members: the compiled chain walk for a
/// melody, the nested performance order for a canon, ascending unit-ref order
/// for every other form.
fn performance_order<'a>(
    workflow: &CompiledWorkflow,
    members: &[(&'a CompiledWorkflowUnit, &'a CPrimeExecutionBinding)],
) -> Result<Vec<&'a CompiledWorkflowUnit>, VakOrchestrationError> {
    let form = members
        .first()
        .map(|(_, binding)| binding.thread_form())
        .unwrap_or(ThreadForm::Single);
    match form {
        ThreadForm::Chain => chain_order(members),
        ThreadForm::Nested => nested_order(workflow, members),
        _ => {
            let mut ordered = members
                .iter()
                .map(|(compiled, _)| *compiled)
                .collect::<Vec<_>>();
            ordered.sort_by(|left, right| left.reference.cmp(&right.reference));
            Ok(ordered)
        }
    }
}

/// The validated melody walk: one root, then each unique successor. Mirrors
/// `validate_chain` so a compiled melody derives in performance order.
fn chain_order<'a>(
    members: &[(&'a CompiledWorkflowUnit, &'a CPrimeExecutionBinding)],
) -> Result<Vec<&'a CompiledWorkflowUnit>, VakOrchestrationError> {
    let planned = members
        .iter()
        .map(|(compiled, _)| compiled.reference.clone())
        .collect::<BTreeSet<_>>();
    let predecessors = |compiled: &CompiledWorkflowUnit| {
        compiled
            .dependencies
            .intersection(&planned)
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let mut current = members
        .iter()
        .map(|(compiled, _)| *compiled)
        .find(|compiled| predecessors(compiled).is_empty())
        .ok_or_else(|| {
            VakOrchestrationError::InvalidChainInput(
                "a melody has exactly one first voice under its interpretation and whole".into(),
            )
        })?;
    let mut ordered = vec![current];
    loop {
        let next = members
            .iter()
            .map(|(compiled, _)| *compiled)
            .filter(|compiled| compiled.dependencies.contains(&current.reference))
            .collect::<Vec<_>>();
        match next.as_slice() {
            [] => break,
            [successor] => {
                if predecessors(successor).len() != 1 {
                    return Err(VakOrchestrationError::InvalidChain {
                        predecessor: current.reference.to_string(),
                        successor: successor.reference.to_string(),
                    });
                }
                ordered.push(successor);
                current = successor;
            }
            [left, ..] => {
                return Err(VakOrchestrationError::InvalidChain {
                    predecessor: current.reference.to_string(),
                    successor: left.reference.to_string(),
                })
            }
        }
    }
    if ordered.len() != members.len() {
        return Err(VakOrchestrationError::InvalidChainInput(
            "every voice of a melody follows the previous voice".into(),
        ));
    }
    Ok(ordered)
}

/// The canon performance order: the root no member nests, then members whose
/// runtime parent — the first nesting edge for that child, which is what
/// `continue_nested` resolves — is already placed, smallest unit-ref first.
fn nested_order<'a>(
    workflow: &CompiledWorkflow,
    members: &[(&'a CompiledWorkflowUnit, &'a CPrimeExecutionBinding)],
) -> Result<Vec<&'a CompiledWorkflowUnit>, VakOrchestrationError> {
    let planned = members
        .iter()
        .map(|(compiled, _)| compiled.reference.clone())
        .collect::<BTreeSet<_>>();
    let by_ref = members
        .iter()
        .map(|(compiled, _)| (compiled.reference.clone(), *compiled))
        .collect::<BTreeMap<_, _>>();
    let member_children = workflow
        .nesting
        .iter()
        .filter(|edge| planned.contains(&edge.parent) && planned.contains(&edge.child))
        .map(|edge| edge.child.clone())
        .collect::<BTreeSet<_>>();
    let mut roots = planned
        .difference(&member_children)
        .cloned()
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        return Err(VakOrchestrationError::InvalidNestedTopology);
    }
    let root = roots.remove(0);
    // `continue_nested` resolves a child's parent as the first nesting edge
    // for it; a child whose runtime parent sits outside its own canon cannot
    // be performed nested under its own interpretation.
    let runtime_parent = |child: &WorkflowUnitRef| {
        workflow
            .nesting
            .iter()
            .find(|edge| edge.child == *child)
            .map(|edge| edge.parent.clone())
    };
    let mut ordered = vec![by_ref[&root]];
    let mut placed = BTreeSet::from([root]);
    while ordered.len() < members.len() {
        let next = planned
            .iter()
            .filter(|unit_ref| !placed.contains(*unit_ref))
            .find(|unit_ref| {
                runtime_parent(unit_ref).is_some_and(|parent| placed.contains(&parent))
            })
            .ok_or(VakOrchestrationError::InvalidNestedTopology)?;
        ordered.push(by_ref[next]);
        placed.insert(next.clone());
    }
    Ok(ordered)
}
