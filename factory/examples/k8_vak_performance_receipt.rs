//! Emit one settled Factory-owned Vāk performance receipt for cross-owner K8
//! acceptance. This is an acceptance harness around the existing native owners;
//! it does not reimplement orchestration, C′ semantics or AIKit scope.

use epilogos_factory::core::run::{ProjectRef, Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, ExecutionDisposition,
    AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    ExecutableOrchestration, ExecutionLaunch, ReturnedArtifact,
};
use epilogos_factory::vak_orchestration::{
    CPrimeExecutionBinding, NativeVakPerformance, VakConductPlan, VakUnitScope,
    AIKIT_OPERATIVE_SCOPE_CONTRACT, QL_C_PRIME_PROFILE_CONTRACT, VAK_ORCHESTRATION_CONTRACT,
};
use epilogos_factory::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";

fn workflow() -> CompiledWorkflow {
    let source: WorkflowSource = serde_json::from_str(FIXTURE).expect("workflow fixture");
    compile_workflow(source).expect("compiled workflow")
}

fn run() -> Run {
    Run::new(
        RUN.parse::<RunRef>().expect("run ref"),
        PROJECT.parse::<ProjectRef>().expect("project ref"),
        "Factory Vāk orchestration K8 receipt",
        "factory-vak-orchestrator",
    )
    .expect("run")
}

fn unit(workflow: &CompiledWorkflow, key: &str) -> WorkflowUnitRef {
    workflow.unit(key).expect("workflow unit").reference.clone()
}

fn scope(workflow: &CompiledWorkflow, key: &str) -> VakUnitScope {
    let unit = workflow.unit(key).expect("workflow unit");
    VakUnitScope {
        unit_ref: unit.reference.clone(),
        whole_ref: "whole:k8-vak-receipt".into(),
        subject_ref: unit.subject_ref.to_string(),
        ai_kit_resolve_path_ref: "resolve-scoped-path:k8-vak-receipt".into(),
        context_resolution_ref: "context-resolution:k8-vak-receipt".into(),
        source_refs: BTreeSet::from(["source:factory-workflow".into()]),
    }
}

fn binding() -> CPrimeExecutionBinding {
    CPrimeExecutionBinding {
        contract: QL_C_PRIME_PROFILE_CONTRACT.into(),
        ql_binding_ref: "ql:whole:k8-vak-receipt".into(),
        ql_binding_revision: "ql-revision-1".into(),
        actor_ref: "agent:epii".into(),
        whole_ref: "whole:factory-k8-vak-receipt".into(),
        subject_ref: PROJECT.into(),
        participation: "authorised-undertaking".into(),
        content: "CT2".into(),
        position: "4.2".into(),
        frame: "CF5".into(),
        thread: "CFP0".into(),
        sequence: "CS2".into(),
        direction: "forward".into(),
        ai_kit_scope_contract: AIKIT_OPERATIVE_SCOPE_CONTRACT.into(),
        ai_kit_resolve_path_ref: "resolve-scoped-path:k8-vak-receipt".into(),
        context_resolution_ref: "context-resolution:k8-vak-receipt".into(),
        source_refs: BTreeSet::from([
            "source:factory-workflow".into(),
            "source:ql-c-prime".into(),
            "source:aikit-scope".into(),
        ]),
        undertaking_authority_ref: Some("authority:k8-vak-receipt".into()),
    }
}

fn disposition(run: &Run, unit: &WorkflowUnitRef) -> ExecutionDisposition {
    accept_aikit_selection(
        ExecutionDemand {
            project_ref: run.project_ref().to_string(),
            run_ref: run.reference().to_string(),
            workflow_unit_ref: Some(unit.to_string()),
            agency_ref: Some("agency:factory-k8-vak-receipt".into()),
            profile_ref: Some("profile:c-prime".into()),
            use_type: "factory-k8-vak-receipt".into(),
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
            model_ref: "model:factory-k8-vak-receipt".into(),
            provider_ref: "provider:factory-k8-vak-receipt".into(),
            ranking_policy: "cross-owner-acceptance".into(),
            ranking_explanation: json!({"eligible": true}),
            provenance: vec!["selection:factory-k8-vak-receipt".into()],
        },
        "2026-09-13T20:00:00+01:00",
    )
    .expect("AIKit disposition")
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .expect("usage: k8_vak_performance_receipt OUTPUT_JSON");
    let workflow = workflow();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = ExecutableOrchestration::new(workflow.clone(), run()).expect("owner");
    let plan = VakConductPlan {
        contract: VAK_ORCHESTRATION_CONTRACT.into(),
        performance_ref: "performance:k8-owner-receipt".into(),
        binding: binding(),
        units: vec![scope(&workflow, "inspect-source")],
        chain_inputs: Vec::new(),
        continuation_ref: None,
        stop_condition_ref: None,
        fusion_barrier_ref: None,
        parent_performance_ref: None,
    };
    let launch = ExecutionLaunch {
        execution_ref: "execution:k8-owner-receipt".into(),
        disposition: disposition(orchestration.run(), &inspect),
        retry_grant: None,
    };
    let performance = NativeVakPerformance::start(
        &mut orchestration,
        "journey:k8-vak-receipt",
        plan,
        BTreeMap::from([(inspect.clone(), launch)]),
    )
    .expect("performance start");
    let compiled = workflow.unit("inspect-source").expect("inspect source");
    orchestration
        .return_artifact(
            &inspect,
            ReturnedArtifact {
                artifact_ref: "artifact:k8-owner-receipt".into(),
                subject_ref: compiled.subject_ref.to_string(),
                subject_revision: compiled.basis_revision.clone(),
                producing_execution_ref: "execution:k8-owner-receipt".into(),
                evidence_refs: BTreeSet::from(["evidence:k8-owner-receipt".into()]),
                semantic_difference: "owner-executed Vāk performance for K8 admission".into(),
            },
        )
        .expect("native Return");
    let snapshot = performance.snapshot(&orchestration).expect("snapshot");
    assert!(snapshot.has_actual_execution());
    assert!(snapshot.settled());
    assert_eq!(snapshot.contract, VAK_ORCHESTRATION_CONTRACT);
    assert_eq!(snapshot.musical_role, "single-voice");
    let bytes = serde_json::to_vec_pretty(&snapshot).expect("serialize receipt");
    let roundtrip = serde_json::from_slice::<epilogos_factory::vak_orchestration::VakPerformanceSnapshot>(&bytes)
        .expect("roundtrip receipt");
    assert_eq!(roundtrip, snapshot);
    if let Some(parent) = Path::new(&output).parent() {
        std::fs::create_dir_all(parent).expect("output directory");
    }
    std::fs::write(&output, bytes).expect("write receipt");
    println!("{}", serde_json::to_string(&snapshot).expect("receipt line"));
}
