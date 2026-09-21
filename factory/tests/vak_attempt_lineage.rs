use epilogos_factory::attempt_runtime::{
    ExecutionBody, ExecutionBudget, SituatedExecutionDisposition, SituatedParticipant,
};
use epilogos_factory::core::run::{ProjectRef, Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{ExecutableOrchestration, ExecutionLaunch, ReturnedArtifact};
use epilogos_factory::vak_orchestration::{
    vak_attempt_start, CPrimeExecutionBinding, VakChainInputBinding, VakChainMaterial,
    VakConductPlan, VakUnitScope, AIKIT_OPERATIVE_SCOPE_CONTRACT, QL_C_PRIME_PROFILE_CONTRACT,
    VAK_CHAIN_INPUT_TRACKING_KIND, VAK_ORCHESTRATION_CONTRACT, VAK_SCOPE_TRACKING_KIND,
};
use epilogos_factory::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use serde_json::json;
use std::collections::BTreeSet;

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";

fn workflow() -> CompiledWorkflow {
    let source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
    compile_workflow(source).unwrap()
}

fn scope(workflow: &CompiledWorkflow, key: &str, suffix: &str) -> VakUnitScope {
    let unit = workflow.unit(key).unwrap();
    VakUnitScope {
        unit_ref: unit.reference.clone(),
        whole_ref: format!("whole:{suffix}"),
        subject_ref: unit.subject_ref.to_string(),
        ai_kit_resolve_path_ref: format!("resolve-scoped-path:{suffix}"),
        context_resolution_ref: format!("context-resolution:{suffix}"),
        source_refs: BTreeSet::from(["source:factory-workflow".into()]),
    }
}

fn plan(workflow: &CompiledWorkflow) -> VakConductPlan {
    VakConductPlan {
        contract: VAK_ORCHESTRATION_CONTRACT.into(),
        performance_ref: "performance:durable-chain".into(),
        binding: CPrimeExecutionBinding {
            contract: QL_C_PRIME_PROFILE_CONTRACT.into(),
            ql_binding_ref: "ql:whole:durable-chain".into(),
            ql_binding_revision: "ql-r7".into(),
            actor_ref: "agent:epii".into(),
            whole_ref: "whole:durable-chain".into(),
            subject_ref: PROJECT.into(),
            participation: "authorised-undertaking".into(),
            content: "CT2".into(),
            position: "4.2".into(),
            frame: "CF5".into(),
            thread: "CFP2".into(),
            sequence: "CS2".into(),
            direction: "forward".into(),
            ai_kit_scope_contract: AIKIT_OPERATIVE_SCOPE_CONTRACT.into(),
            ai_kit_resolve_path_ref: "resolve-scoped-path:durable-chain".into(),
            context_resolution_ref: "context-resolution:durable-chain".into(),
            source_refs: BTreeSet::from([
                "source:factory-workflow".into(),
                "source:ql-c-prime".into(),
                "source:aikit-scope".into(),
            ]),
            undertaking_authority_ref: Some("authority:undertaking".into()),
        },
        units: vec![
            scope(workflow, "inspect-source", "inspect"),
            scope(workflow, "review-adversarially", "review"),
        ],
        chain_inputs: vec![VakChainInputBinding {
            predecessor_unit_ref: workflow.unit("inspect-source").unwrap().reference.clone(),
            successor_unit_ref: workflow
                .unit("review-adversarially")
                .unwrap()
                .reference
                .clone(),
            receiving_context_ref: "receiving-context:inspect-to-review".into(),
        }],
        continuation_ref: None,
        stop_condition_ref: None,
        fusion_barrier_ref: None,
        parent_performance_ref: None,
    }
}

fn disposition(workflow: &CompiledWorkflow, key: &str) -> SituatedExecutionDisposition {
    let unit = workflow.unit(key).unwrap();
    let demand = ExecutionDemand {
        project_ref: PROJECT.into(),
        run_ref: RUN.into(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some("agency:durable-chain".into()),
        profile_ref: Some("profile:c-prime".into()),
        use_type: "factory-vak-durable-attempt-test".into(),
        required_capabilities: unit.capability_refs.clone(),
        required_modalities: BTreeSet::from(["text".into()]),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::new(),
        independence_from: BTreeSet::new(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    };
    let selection = accept_aikit_selection(
        demand,
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:durable-chain".into(),
            provider_ref: "provider:durable-chain".into(),
            ranking_policy: "test-contract".into(),
            ranking_explanation: json!({"eligible": true}),
            provenance: vec!["selection:durable-chain".into()],
        },
        "2026-09-13T21:00:00+01:00",
    )
    .unwrap();
    SituatedExecutionDisposition {
        selected_inputs: Vec::new(),
        selection,
        participant: SituatedParticipant {
            agent_ref: unit
                .agent_requirements
                .agent_refs
                .iter()
                .next()
                .unwrap()
                .clone(),
            agency_ref: "agency:durable-chain".into(),
            world_binding_ref: "world-binding:durable-chain".into(),
            profile_ref: Some("profile:c-prime".into()),
            source_ref: workflow.source.reference.to_string(),
            source_revision: workflow.source.revision.clone(),
            source_digest: format!("blake3:{}", workflow.source.digest),
        },
        context_refs: BTreeSet::from(["context:preexisting".into()]),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        body: ExecutionBody {
            model_ref: "model:durable-chain".into(),
            provider_ref: "provider:durable-chain".into(),
            route_ref: "route:durable-chain".into(),
            harness_ref: "harness:durable-chain".into(),
            harness_composition_ref: "composition:durable-chain".into(),
            agent_session_ref: "agent-session:durable-chain".into(),
            session_space_ref: "space:durable-chain".into(),
            material_world_ref: Some("material-world:durable-chain".into()),
            workcell_ref: Some("workcell:durable-chain".into()),
        },
        placement: None,
        permitted_effects: unit.permitted_effects.clone(),
        verification_obligations: unit.verification_obligations.clone(),
        return_address: unit.return_address.clone(),
        stop_conditions: unit.stop_conditions.clone(),
        escalation_conditions: unit.escalation_conditions.clone(),
        budget: ExecutionBudget {
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            wall_clock_timeout_ms: Some(120_000),
            retry_grant_ref: None,
            maximum_attempts: None,
        },
    }
}

#[test]
fn durable_attempt_carries_scope_and_predecessor_result_into_native_owner_context() {
    let workflow = workflow();
    let plan = plan(&workflow);
    let predecessor = workflow.unit("inspect-source").unwrap();
    let successor = workflow.unit("review-adversarially").unwrap();
    let run = Run::new(
        RUN.parse::<RunRef>().unwrap(),
        PROJECT.parse::<ProjectRef>().unwrap(),
        "Factory Vāk durable lineage acceptance",
        "factory-vak-lineage",
    )
    .unwrap();
    let mut orchestration = ExecutableOrchestration::new(workflow.clone(), run).unwrap();
    let predecessor_disposition = disposition(&workflow, "inspect-source");
    orchestration
        .start_serial(
            "journey:durable-chain",
            &predecessor.reference,
            ExecutionLaunch {
                execution_ref: "execution:inspect-returned".into(),
                disposition: predecessor_disposition.selection,
                retry_grant: None,
            },
        )
        .unwrap();
    orchestration
        .return_artifact(
            &predecessor.reference,
            ReturnedArtifact {
                artifact_ref: "artifact:inspect-returned".into(),
                subject_ref: predecessor.subject_ref.to_string(),
                subject_revision: predecessor.basis_revision.clone(),
                producing_execution_ref: "execution:inspect-returned".into(),
                evidence_refs: BTreeSet::from(["evidence:inspect-returned".into()]),
                semantic_difference: "inspected source basis".into(),
            },
        )
        .unwrap();
    let material = VakChainMaterial {
        predecessor_unit_ref: predecessor.reference.clone(),
        predecessor_execution_ref: "execution:inspect-returned".into(),
        successor_unit_ref: successor.reference.clone(),
        subject_ref: predecessor.subject_ref.to_string(),
        subject_revision: predecessor.basis_revision.clone(),
        receiving_context_ref: "receiving-context:inspect-to-review".into(),
        artifact_refs: BTreeSet::from(["artifact:inspect-returned".into()]),
        evidence_refs: BTreeSet::from(["evidence:inspect-returned".into()]),
        semantic_differences: BTreeSet::from(["inspected source basis".into()]),
    };
    let original = disposition(&workflow, "review-adversarially");
    let start = vak_attempt_start(
        &orchestration,
        &plan,
        &successor.reference,
        "attempt:durable-chain",
        "task:review-from-inspection",
        original.clone(),
        None,
        Some(&material),
    )
    .unwrap();

    assert_eq!(start.tracking.len(), 2);
    assert!(start
        .tracking
        .iter()
        .any(|fact| fact.kind == VAK_SCOPE_TRACKING_KIND));
    assert!(start
        .tracking
        .iter()
        .any(|fact| fact.kind == VAK_CHAIN_INPUT_TRACKING_KIND));
    for reference in [
        "performance:durable-chain",
        "resolve-scoped-path:review",
        "context-resolution:review",
        "artifact:inspect-returned",
        "evidence:inspect-returned",
        "execution:inspect-returned",
        "receiving-context:inspect-to-review",
    ] {
        assert!(start.disposition.context_refs.contains(reference));
    }
    assert!(start
        .disposition
        .context_refs
        .contains("context:preexisting"));
    assert_eq!(start.disposition.participant, original.participant);
    assert_eq!(start.disposition.body, original.body);
    assert_eq!(
        start.disposition.permitted_effects,
        original.permitted_effects
    );
    assert_eq!(
        start.disposition.verification_obligations,
        original.verification_obligations
    );
    assert_eq!(start.disposition.return_address, original.return_address);

    let mut stale = orchestration.clone();
    stale.advance_subject(PROJECT, "advanced-project-revision");
    assert!(vak_attempt_start(
        &stale,
        &plan,
        &successor.reference,
        "attempt:stale-chain",
        "task:stale-review",
        original.clone(),
        None,
        Some(&material),
    )
    .is_err());

    let mut foreign_source = original;
    foreign_source.participant.source_revision = "foreign-workflow-revision".into();
    assert!(vak_attempt_start(
        &orchestration,
        &plan,
        &successor.reference,
        "attempt:foreign-source",
        "task:foreign-review",
        foreign_source,
        None,
        Some(&material),
    )
    .is_err());
}
