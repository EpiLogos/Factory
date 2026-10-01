//! Real source compilation, native topology, coordinator restore and admission.
//! No external execution or worker success is asserted by these boundary tests.

use epilogos_factory::core::run::{
    EdgeKind, NodeId, NodeKind, NodeState, Run, RunTopologyCommand, TopologyEdge, TopologyMutation,
    TopologyNode,
};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    validate_initial_subject_revisions, ExecutableOrchestration, ExecutionLaunch,
    OrchestrationError, OrchestrationSnapshot, ReturnedArtifact, WholeRunState,
};
use epilogos_factory::workflow::{compile_workflow, workflow_source_digest, WorkflowSource};
use serde_json::json;
use std::collections::BTreeSet;

fn source() -> WorkflowSource {
    serde_json::from_str(include_str!(
        "../../contracts/factory/fixtures/agent-workflow-source.json"
    ))
    .unwrap()
}

fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
        "native source basis guard",
        "factory",
    )
    .unwrap()
}

fn additional_source() -> WorkflowSource {
    let mut additional = source();
    additional.source.reference = "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FBY"
        .parse()
        .unwrap();
    additional.source.revision = "second-required-source".into();
    additional.workflow_key = "separate-required-work".into();
    additional.units.truncate(1);
    additional.units[0].key = "second-required-work".into();
    additional.barriers.clear();
    additional.source.digest = workflow_source_digest(&additional).unwrap();
    additional
}

fn retain_historical_partial_source(engine: &ExecutableOrchestration) -> ExecutableOrchestration {
    let mut owned = engine.run().clone();
    let extra = compile_workflow(additional_source()).unwrap();
    owned
        .apply_topology_command(
            &owned.mutation_authority(),
            extra.topology_command(owned.revision()),
        )
        .unwrap();
    // Earlier coordinator admission selected one source against this complete
    // canonical map. Retain that snapshot shape without changing any leg state,
    // result, execution identity, evidence or immutable source bytes.
    let mut retained = serde_json::to_value(engine.snapshot()).unwrap();
    retained["topologyRevision"] = serde_json::to_value(owned.map().topology_revision()).unwrap();
    let snapshot: OrchestrationSnapshot = serde_json::from_value(retained).unwrap();
    snapshot.restore(engine.workflow().clone(), owned).unwrap()
}

#[test]
fn native_source_selection_cannot_admit_or_restart_with_omitted_workflow_nodes() {
    let compiled = compile_workflow(source()).unwrap();
    let engine = ExecutableOrchestration::new(compiled.clone(), run()).unwrap();
    let mut historical = retain_historical_partial_source(&engine);
    let omitted = compile_workflow(additional_source()).unwrap();
    let omitted_unit = omitted
        .unit("second-required-work")
        .unwrap()
        .reference
        .clone();
    assert_eq!(historical.required_units().len(), compiled.units.len() + 1);
    assert_eq!(
        historical.unaccounted_units(),
        BTreeSet::from([omitted_unit.clone()])
    );
    assert!(matches!(
        ExecutableOrchestration::new(compiled, historical.run().clone()),
        Err(OrchestrationError::UnaccountedWorkflowUnits { units, .. })
            if units == BTreeSet::from([omitted_unit.clone()])
    ));
    let before = historical.snapshot();
    let unit = historical
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let selected = launch(&historical, "inspect-source");
    assert!(matches!(
        historical.start_serial("journey:partial-source-history", &unit, selected),
        Err(OrchestrationError::UnaccountedWorkflowUnits { units, .. })
            if units == BTreeSet::from([omitted_unit])
    ));
    assert_eq!(historical.snapshot(), before);
}

#[test]
fn historical_current_returns_do_not_complete_an_omitted_required_workflow_leg() {
    let mut selected = source();
    selected.units.truncate(1);
    selected.barriers.clear();
    selected.source.digest = workflow_source_digest(&selected).unwrap();
    let mut engine =
        ExecutableOrchestration::new(compile_workflow(selected).unwrap(), run()).unwrap();
    let unit = engine
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let selected = launch(&engine, "inspect-source");
    let delegation = engine
        .start_serial("journey:retained-native-return", &unit, selected)
        .unwrap();
    engine
        .return_artifact(
            &unit,
            ReturnedArtifact {
                artifact_ref: "artifact:controlled-owner-return".into(),
                subject_ref: delegation.subject_ref,
                subject_revision: delegation.basis_revision,
                producing_execution_ref: "execution:inspect-source".into(),
                evidence_refs: BTreeSet::from(["evidence:controlled-native-transition".into()]),
                semantic_difference: "Native returned artifact for the sole selected source unit"
                    .into(),
            },
        )
        .unwrap();
    assert_eq!(engine.whole_run_state(), WholeRunState::Complete);
    let restored = retain_historical_partial_source(&engine);
    assert!(restored.is_current_return(&unit));
    assert_eq!(restored.workflow().units.len(), 1);
    assert_eq!(restored.required_units().len(), 2);
    assert_eq!(restored.unaccounted_units().len(), 1);
    assert_eq!(restored.whole_run_state(), WholeRunState::Incomplete);
    assert_eq!(restored.legs(), engine.legs());
}

#[test]
fn manual_work_nodes_keep_their_existing_public_topology_behavior() {
    let mut owned = run();
    owned
        .apply_topology_command(
            &owned.mutation_authority(),
            RunTopologyCommand {
                command_id: "manual-work-before-workflow".into(),
                expected_revision: owned.revision(),
                mutation: TopologyMutation::Batch {
                    mutations: vec![
                        TopologyMutation::AddNode {
                            node: TopologyNode {
                                id: NodeId::new("manual-work").unwrap(),
                                kind: NodeKind::Work,
                                label: "Manual work with no workflow execution identity".into(),
                                state: Some(NodeState::Planned),
                                semantic_ref: None,
                            },
                        },
                        TopologyMutation::AddEdge {
                            edge: TopologyEdge {
                                from: NodeId::new("destination").unwrap(),
                                to: NodeId::new("manual-work").unwrap(),
                                relation: EdgeKind::BranchesTo,
                            },
                        },
                    ],
                },
            },
        )
        .unwrap();
    let compiled = compile_workflow(source()).unwrap();
    let engine = ExecutableOrchestration::new(compiled.clone(), owned).unwrap();
    assert_eq!(engine.required_units().len(), compiled.units.len());
    assert!(engine.unaccounted_units().is_empty());
    assert_eq!(
        engine.run().map().nodes()[&NodeId::new("manual-work").unwrap()].state,
        Some(NodeState::Planned)
    );
}

fn launch(engine: &ExecutableOrchestration, key: &str) -> ExecutionLaunch {
    let unit = engine.workflow().unit(key).unwrap();
    let selection = accept_aikit_selection(
        ExecutionDemand {
            project_ref: engine.run().project_ref().to_string(),
            run_ref: engine.run().reference().to_string(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some("agency:controlled-admission".into()),
            profile_ref: None,
            use_type: "native-guard-contract-test".into(),
            required_capabilities: BTreeSet::new(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        },
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:controlled-admission".into(),
            provider_ref: "provider:controlled-admission".into(),
            ranking_policy: "controlled-contract-test".into(),
            ranking_explanation: json!({"contractTest":true}),
            provenance: vec!["selection:controlled-admission".into()],
        },
        "2026-10-01T00:15:00Z",
    )
    .unwrap();
    ExecutionLaunch {
        execution_ref: format!("execution:{key}"),
        disposition: selection,
        retry_grant: None,
    }
}

#[test]
fn differing_initial_product_pins_for_one_subject_cannot_be_silently_selected() {
    let mut source = source();
    source.units[1].basis_revision = "a-different-product-head".into();
    source.source.digest = workflow_source_digest(&source).unwrap();
    let workflow = compile_workflow(source).unwrap();
    assert!(matches!(
        validate_initial_subject_revisions(&workflow),
        Err(OrchestrationError::AmbiguousInitialSubjectRevision { .. })
    ));
    assert!(matches!(
        ExecutableOrchestration::new(workflow, run()),
        Err(OrchestrationError::AmbiguousInitialSubjectRevision { .. })
    ));
}

#[test]
fn historical_ambiguous_snapshot_remains_readable_but_new_work_is_refused() {
    let mut source = source();
    source.units[1].basis_revision = "a-different-product-head".into();
    source.source.digest = workflow_source_digest(&source).unwrap();
    let workflow = compile_workflow(source).unwrap();
    let mut run = run();
    run.apply_topology_command(
        &run.mutation_authority(),
        workflow.topology_command(run.revision()),
    )
    .unwrap();
    // Retain the shape emitted by the earlier owner, which selected one basis
    // for the shared subject. Reading does not rewrite that historical source.
    let first = workflow.units.values().next().unwrap();
    let snapshot: OrchestrationSnapshot = serde_json::from_value(json!({
        "contract":"factory.orchestration-snapshot/v1", "runRef":run.reference(),
        "topologyRevision":run.map().topology_revision(), "workflowSource":workflow.source,
        "workflowKey":workflow.workflow_key, "legs":{},
        "subjectRevisions":{first.subject_ref.to_string():first.basis_revision},
        "activeWriters":{}, "retryGrants":{}, "reviewers":{}, "syntheses":{},
        "ownerObservations":{}, "concatenationAttempts":[], "selfReviewAttempts":[],
    }))
    .unwrap();
    let mut restored = snapshot.restore(workflow, run).unwrap();
    let before = restored.snapshot();
    let unit = restored
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let selected = launch(&restored, "inspect-source");
    assert!(matches!(
        restored.start_serial("journey:controlled-history", &unit, selected),
        Err(OrchestrationError::AmbiguousInitialSubjectRevision { .. })
    ));
    assert_eq!(restored.snapshot(), before);
}

#[test]
fn an_explicit_subject_advance_preserves_coherent_original_declarations() {
    let mut source = source();
    source.barriers.clear();
    source.nesting.clear();
    for unit in &mut source.units {
        unit.dependencies.clear();
        unit.independence_from.clear();
        unit.permitted_effects = vec!["read controlled source".into()];
    }
    source.source.digest = workflow_source_digest(&source).unwrap();
    let workflow = compile_workflow(source).unwrap();
    let subject = workflow
        .unit("inspect-source")
        .unwrap()
        .subject_ref
        .to_string();
    let mut engine = ExecutableOrchestration::new(workflow, run()).unwrap();
    let first = engine
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let first_launch = launch(&engine, "inspect-source");
    engine
        .start_serial("journey:controlled-advance", &first, first_launch)
        .unwrap();
    engine.advance_subject(&subject, "explicit-native-source-advance");
    let next = engine
        .workflow()
        .unit("review-adversarially")
        .unwrap()
        .reference
        .clone();
    let next_launch = launch(&engine, "review-adversarially");
    let delegation = engine
        .start_serial("journey:controlled-advance", &next, next_launch)
        .unwrap();
    assert_eq!(delegation.basis_revision, "explicit-native-source-advance");
    assert_eq!(
        engine
            .workflow()
            .unit("review-adversarially")
            .unwrap()
            .basis_revision,
        "947ce7a"
    );
    assert_eq!(
        engine.leg(&first).unwrap().delegation.basis_revision,
        "947ce7a"
    );
}
