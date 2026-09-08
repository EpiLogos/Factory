use epilogos_factory::build::FactoryBuildState;
use epilogos_factory::core::identity::Revision;
use epilogos_factory::core::run::{
    CommandOutcome, EdgeKind, NodeKind, NodeState, Project, ProjectRef, Run, RunRef,
    WorkflowUnitRef,
};
use epilogos_factory::developmental_read::FactoryDevelopmentalState;
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, ExecutionInteropError,
    AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, WorkflowError, WorkflowNestingSource, WorkflowSource,
    BOUNDED_COORDINATION_CONTRACT, COMPILED_WORKFLOW_SCHEMA, WORKFLOW_SOURCE_SCHEMA,
    WORKFLOW_UNIT_IDENTITY_ALGORITHM,
};
use serde_json::json;
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const SCHEMA: &str = include_str!("../../contracts/factory/agent-workflow-source.schema.json");

fn fixture() -> WorkflowSource {
    serde_json::from_str(FIXTURE).expect("checked-in aggregate workflow source must deserialize")
}

fn restamp(mut source: WorkflowSource) -> WorkflowSource {
    source.source.digest = workflow_source_digest(&source).expect("source must canonicalize");
    source
}

fn compiled_fixture() -> epilogos_factory::workflow::CompiledWorkflow {
    compile_workflow(fixture()).expect("checked-in aggregate source must compile")
}

fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse::<RunRef>().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW"
            .parse::<ProjectRef>()
            .unwrap(),
        "Compile Agent-first workflow",
        "factory-test",
    )
    .unwrap()
}

fn selection(model: &str, provider: &str) -> AikitModelRosterSelection {
    AikitModelRosterSelection {
        roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
        model_ref: model.into(),
        provider_ref: provider.into(),
        ranking_policy: "balanced".into(),
        ranking_explanation: json!({"eligible": true}),
        provenance: vec!["selection:source".into()],
    }
}

fn demand(unit_ref: &WorkflowUnitRef) -> ExecutionDemand {
    ExecutionDemand {
        project_ref: "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
        run_ref: "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".into(),
        workflow_unit_ref: Some(unit_ref.to_string()),
        agency_ref: None,
        profile_ref: None,
        use_type: "agent-first-workflow-unit".into(),
        required_capabilities: BTreeSet::new(),
        required_modalities: BTreeSet::new(),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::new(),
        independence_from: BTreeSet::new(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    }
}

#[test]
fn checked_in_schema_and_fixture_are_the_aggregate_source_contract() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    assert_eq!(schema["title"], "Factory Agent-first workflow source");
    assert_eq!(
        schema["properties"]["schemaVersion"]["const"],
        WORKFLOW_SOURCE_SCHEMA
    );
    assert_eq!(
        schema["properties"]["coordinationContract"]["const"],
        BOUNDED_COORDINATION_CONTRACT
    );
    assert!(schema["properties"].get("units").is_some());
    assert!(schema["properties"].get("sourceUnitLocator").is_none());

    let source = fixture();
    assert_eq!(source.schema_version, WORKFLOW_SOURCE_SCHEMA);
    assert_eq!(source.coordination_contract, BOUNDED_COORDINATION_CONTRACT);
    assert_eq!(
        source.source.digest,
        workflow_source_digest(&source).unwrap()
    );
    assert_eq!(source.units.len(), 4);
}

#[test]
fn compilation_is_deterministic_and_resolves_source_local_graph_relations() {
    let first = compiled_fixture();
    let second = compiled_fixture();
    assert_eq!(first, second);
    assert_eq!(first.schema_version, COMPILED_WORKFLOW_SCHEMA);
    assert_eq!(first.identity_algorithm, WORKFLOW_UNIT_IDENTITY_ALGORITHM);
    assert_eq!(first.units.len(), 4);

    let inspect = &first.unit("inspect-source").unwrap().reference;
    let implementation = first.unit("implement-compiler").unwrap();
    let review = first.unit("review-adversarially").unwrap();
    assert_eq!(
        implementation.dependencies,
        BTreeSet::from([inspect.clone()])
    );
    assert_eq!(
        implementation.independence_from,
        BTreeSet::from([review.reference.clone()])
    );
    assert_eq!(first.barriers[0].waits_for.len(), 2);
    assert_eq!(first.barriers[0].releases.len(), 1);
}

#[test]
fn identity_is_order_stable_but_semantic_and_exact_provenance_sensitive() {
    let first = compiled_fixture();

    let mut reordered = fixture();
    reordered.units.reverse();
    reordered.units[0].capability_refs.reverse();
    reordered.barriers[0].waits_for.reverse();
    assert_eq!(
        first.units,
        compile_workflow(reordered).unwrap().units,
        "set and declaration order are not semantic identity"
    );

    let mut changed_context = fixture();
    changed_context.source.temporal_ref = Some("flow-time:tomorrow".into());
    changed_context.source.flow_ref = Some("flow:another-presentation".into());
    assert_eq!(
        first.units,
        compile_workflow(changed_context).unwrap().units,
        "optional carrying context is provenance but not unit identity"
    );

    let mut changed_semantics = fixture();
    changed_semantics.units[0]
        .required_difference
        .push_str(" with a new material condition");
    assert!(matches!(
        compile_workflow(changed_semantics.clone()),
        Err(WorkflowError::SourceDigestMismatch { .. })
    ));
    let changed_semantics = compile_workflow(restamp(changed_semantics)).unwrap();
    assert_ne!(first.units, changed_semantics.units);

    let mut changed_revision = fixture();
    changed_revision.source.revision = "source-revision-8".into();
    assert_ne!(
        first.units,
        compile_workflow(changed_revision).unwrap().units,
        "exact source revision is part of every unit identity"
    );
}

#[test]
fn runtime_carriers_are_rejected_from_source_and_do_not_change_attempt_correlation() {
    let mut raw: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    raw["provider"] = json!("provider:runtime");
    raw["model"] = json!("model:runtime");
    raw["session"] = json!("session:runtime");
    raw["workcell"] = json!("workcell:runtime");
    raw["presentation"] = json!("rich");
    assert!(serde_json::from_value::<WorkflowSource>(raw).is_err());

    let compiled = compiled_fixture();
    let unit = compiled.unit("implement-compiler").unwrap();
    let mut base_demand = demand(&unit.reference);
    base_demand.workflow_unit_ref = None;
    let bound_demand = unit.bind_execution_demand(base_demand);
    let attempt_a = accept_aikit_selection(
        bound_demand.clone(),
        selection("model:a", "provider:a"),
        "attempt-a",
    )
    .unwrap();
    let attempt_b = accept_aikit_selection(
        bound_demand,
        selection("model:b", "provider:b"),
        "attempt-b",
    )
    .unwrap();
    assert_ne!(attempt_a.selection.model_ref, attempt_b.selection.model_ref);
    assert_ne!(
        attempt_a.selection.provider_ref,
        attempt_b.selection.provider_ref
    );
    assert_ne!(attempt_a.decided_at, attempt_b.decided_at);
    assert_eq!(
        attempt_a.demand.workflow_unit_ref,
        attempt_b.demand.workflow_unit_ref
    );
    assert_eq!(
        attempt_a.demand.workflow_unit_ref,
        Some(unit.reference.to_string())
    );
    assert!(unit
        .capability_refs
        .is_subset(&attempt_a.demand.required_capabilities));
}

#[test]
fn malformed_provenance_and_ambiguous_source_fail_closed() {
    let mut wrong_contract = fixture();
    wrong_contract.coordination_contract = "factory.bounded-coordination/v0".into();
    assert!(matches!(
        compile_workflow(wrong_contract),
        Err(WorkflowError::WrongCoordinationContract(_))
    ));

    let mut bad_digest = fixture();
    bad_digest.source.digest = "not-a-digest".into();
    assert!(matches!(
        compile_workflow(bad_digest),
        Err(WorkflowError::InvalidDigest(_))
    ));

    let mut duplicate = fixture();
    let repeated_capability = duplicate.units[0].capability_refs[0].clone();
    duplicate.units[0].capability_refs.push(repeated_capability);
    assert!(matches!(
        workflow_source_digest(&duplicate),
        Err(WorkflowError::DuplicateValue {
            field: "capabilityRefs",
            ..
        })
    ));

    let mut no_agent = fixture();
    no_agent.units[0].agent_requirements.agent_refs.clear();
    assert!(matches!(
        workflow_source_digest(&no_agent),
        Err(WorkflowError::EmptyCollection("agentRequirements"))
    ));
}

#[test]
fn dangling_self_contradictory_and_transitive_cycles_are_rejected() {
    let mut dangling = fixture();
    dangling.units[0].dependencies.push("absent-unit".into());
    dangling = restamp(dangling);
    assert!(matches!(
        compile_workflow(dangling),
        Err(WorkflowError::DanglingUnit {
            field: "dependencies",
            ..
        })
    ));

    let mut self_dependency = fixture();
    self_dependency.units[0]
        .dependencies
        .push("inspect-source".into());
    self_dependency = restamp(self_dependency);
    assert!(matches!(
        compile_workflow(self_dependency),
        Err(WorkflowError::SelfDependency(_))
    ));

    let mut contradiction = fixture();
    contradiction.units[1]
        .independence_from
        .push("inspect-source".into());
    contradiction = restamp(contradiction);
    assert!(matches!(
        compile_workflow(contradiction),
        Err(WorkflowError::ContradictoryRelations { .. })
    ));

    let mut cycle = fixture();
    cycle.units[0]
        .dependencies
        .push("implement-compiler".into());
    cycle = restamp(cycle);
    assert!(matches!(
        compile_workflow(cycle),
        Err(WorkflowError::DependencyCycle(_))
    ));
}

#[test]
fn barrier_and_nesting_cycles_are_rejected_before_run_mutation() {
    let mut barrier_cycle = fixture();
    barrier_cycle.units[0]
        .dependencies
        .push("integrate-return".into());
    barrier_cycle = restamp(barrier_cycle);
    assert!(matches!(
        compile_workflow(barrier_cycle),
        Err(WorkflowError::DependencyCycle(_))
    ));

    let mut nesting_cycle = fixture();
    nesting_cycle.nesting = vec![
        WorkflowNestingSource {
            parent: "implement-compiler".into(),
            child: "review-adversarially".into(),
        },
        WorkflowNestingSource {
            parent: "review-adversarially".into(),
            child: "implement-compiler".into(),
        },
    ];
    nesting_cycle = restamp(nesting_cycle);
    assert!(matches!(
        compile_workflow(nesting_cycle),
        Err(WorkflowError::NestingCycle(_))
    ));
}

#[test]
fn aggregate_compilation_applies_atomically_through_existing_run_authority() {
    let compiled = compiled_fixture();
    let run = run();
    let run_ref = run.reference().clone();
    let project = Project::new(run.project_ref().clone());
    let mut build = FactoryBuildState::new(project, run).unwrap();
    let authority = build.run_mutation_authority(&run_ref).unwrap();
    let command = compiled.topology_command(Revision::INITIAL);
    let retry = command.clone();
    let outcome = build
        .apply_run_topology_command(&run_ref, &authority, command)
        .unwrap();
    assert_eq!(
        outcome,
        CommandOutcome::Applied {
            revision: Revision::new(2).unwrap(),
            topology_revision: Revision::new(2).unwrap()
        }
    );
    assert_eq!(
        build
            .apply_run_topology_command(&run_ref, &authority, retry)
            .unwrap(),
        CommandOutcome::AlreadyApplied {
            revision: Revision::new(2).unwrap(),
            topology_revision: Revision::new(2).unwrap()
        }
    );

    let state = FactoryDevelopmentalState::new(build, vec![]).unwrap();
    let reading = state.run_reading(&run_ref).unwrap();
    let run_map = &reading.run_map;

    let work_nodes = run_map
        .nodes()
        .values()
        .filter(|node| node.kind == NodeKind::Work)
        .collect::<Vec<_>>();
    assert_eq!(work_nodes.len(), 4);
    assert!(work_nodes.iter().all(|node| {
        node.semantic_ref
            .as_ref()
            .is_some_and(|reference| reference.kind() == "workflow-unit")
    }));
    assert!(work_nodes.iter().any(|node| {
        node.label.contains("Integrate implementation") && node.state == Some(NodeState::Planned)
    }));
    assert!(run_map
        .nodes()
        .values()
        .any(|node| node.kind == NodeKind::Gate));
    assert!(run_map
        .edges()
        .iter()
        .any(|edge| edge.relation == EdgeKind::Requires));

    let public_map = serde_json::to_value(&reading).unwrap();
    assert!(public_map["runMap"]["nodes"]
        .as_object()
        .unwrap()
        .values()
        .any(|node| node["semanticRef"]
            .as_str()
            .is_some_and(|value| value.starts_with("workflow-unit:"))));
}

#[test]
fn execution_intelligence_rejects_malformed_unit_correlation() {
    let mut demand = demand(&compiled_fixture().unit("inspect-source").unwrap().reference);
    demand.workflow_unit_ref = Some("workflow-unit:not-a-ulid".into());
    assert!(matches!(
        accept_aikit_selection(demand, selection("model:a", "provider:a"), "attempt"),
        Err(ExecutionInteropError::InvalidWorkflowUnitRef(_))
    ));
}
