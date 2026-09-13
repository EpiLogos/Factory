use epilogos_factory::core::run::{ProjectRef, Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, ExecutionDisposition,
    AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    ExecutableOrchestration, ExecutionLaunch, LegStatus, ReturnedArtifact,
};
use epilogos_factory::vak_orchestration::{
    sustained_retry_grant, CPrimeExecutionBinding, NativeVakPerformance, VakConductPlan,
    VakUnitScope, VakZCycle, ZStage, AIKIT_OPERATIVE_SCOPE_CONTRACT,
    QL_C_PRIME_PROFILE_CONTRACT, VAK_ORCHESTRATION_CONTRACT,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowNestingSource,
    WorkflowSource,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");

fn source() -> WorkflowSource {
    serde_json::from_str(FIXTURE).unwrap()
}
fn compiled() -> CompiledWorkflow {
    compile_workflow(source()).unwrap()
}
fn nested_compiled() -> CompiledWorkflow {
    let mut source = source();
    source.nesting = vec![WorkflowNestingSource {
        parent: "inspect-source".into(),
        child: "implement-compiler".into(),
    }];
    source.source.digest = workflow_source_digest(&source).unwrap();
    compile_workflow(source).unwrap()
}
fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD"
            .parse::<RunRef>()
            .unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW"
            .parse::<ProjectRef>()
            .unwrap(),
        "Factory Vāk orchestration acceptance",
        "factory-vak-orchestrator",
    )
    .unwrap()
}
fn engine(workflow: CompiledWorkflow) -> ExecutableOrchestration {
    ExecutableOrchestration::new(workflow, run()).unwrap()
}
fn unit(workflow: &CompiledWorkflow, key: &str) -> WorkflowUnitRef {
    workflow.unit(key).unwrap().reference.clone()
}
fn binding(thread: &str) -> CPrimeExecutionBinding {
    CPrimeExecutionBinding {
        contract: QL_C_PRIME_PROFILE_CONTRACT.into(),
        ql_binding_ref: "ql:whole:test".into(),
        ql_binding_revision: "ql-revision-1".into(),
        actor_ref: "agent:epii".into(),
        whole_ref: "whole:factory-vak-test".into(),
        subject_ref: "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
        participation: "authorised-undertaking".into(),
        content: "CT2".into(),
        position: "4.2".into(),
        frame: "CF5".into(),
        thread: thread.into(),
        sequence: "CS2".into(),
        direction: "forward".into(),
        ai_kit_scope_contract: AIKIT_OPERATIVE_SCOPE_CONTRACT.into(),
        ai_kit_resolve_path_ref: "resolve-scoped-path:test".into(),
        context_resolution_ref: "context-resolution:test".into(),
        source_refs: BTreeSet::from([
            "source:factory-workflow".into(),
            "source:ql-c-prime".into(),
            "source:aikit-scope".into(),
        ]),
        undertaking_authority_ref: Some("authority:undertaking".into()),
    }
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
fn plan(workflow: &CompiledWorkflow, thread: &str, keys: &[(&str, &str)]) -> VakConductPlan {
    VakConductPlan {
        contract: VAK_ORCHESTRATION_CONTRACT.into(),
        performance_ref: format!("performance:{thread}"),
        binding: binding(thread),
        units: keys
            .iter()
            .map(|(key, suffix)| scope(workflow, key, suffix))
            .collect(),
        continuation_ref: (thread == "CFP4").then(|| "continuation:sustained".into()),
        stop_condition_ref: (thread == "CFP4").then(|| "stop:sustained".into()),
        fusion_barrier_ref: (thread == "CFP3").then(|| "implementation-reviewed".into()),
        parent_performance_ref: (thread == "CFP5").then(|| "performance:outer".into()),
    }
}
fn disposition(run: &Run, unit: Option<&WorkflowUnitRef>) -> ExecutionDisposition {
    disposition_independent(run, unit, BTreeSet::new())
}
fn disposition_independent(
    run: &Run,
    unit: Option<&WorkflowUnitRef>,
    independence_from: BTreeSet<String>,
) -> ExecutionDisposition {
    accept_aikit_selection(
        ExecutionDemand {
            project_ref: run.project_ref().to_string(),
            run_ref: run.reference().to_string(),
            workflow_unit_ref: unit.map(ToString::to_string),
            agency_ref: Some("agency:factory-vak-test".into()),
            profile_ref: Some("profile:c-prime".into()),
            use_type: "factory-vak-orchestration-test".into(),
            required_capabilities: BTreeSet::new(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from,
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        },
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:factory-vak-test".into(),
            provider_ref: "provider:factory-vak-test".into(),
            ranking_policy: "test-contract".into(),
            ranking_explanation: json!({"eligible": true}),
            provenance: vec!["selection:factory-vak-test".into()],
        },
        "2026-09-13T20:00:00+01:00",
    )
    .unwrap()
}
fn launch(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
) -> ExecutionLaunch {
    ExecutionLaunch {
        execution_ref: execution_ref.into(),
        disposition: disposition(orchestration.run(), Some(unit)),
        retry_grant: None,
    }
}
fn artifact(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    artifact_ref: &str,
    execution_ref: &str,
    evidence_ref: &str,
) -> ReturnedArtifact {
    let compiled = orchestration
        .workflow()
        .units
        .values()
        .find(|candidate| &candidate.reference == unit)
        .unwrap();
    ReturnedArtifact {
        artifact_ref: artifact_ref.into(),
        subject_ref: compiled.subject_ref.to_string(),
        subject_revision: compiled.basis_revision.clone(),
        producing_execution_ref: execution_ref.into(),
        evidence_refs: BTreeSet::from([evidence_ref.into()]),
        semantic_difference: format!("returned {}", compiled.developmental_concern),
    }
}
fn complete_inspect(orchestration: &mut ExecutableOrchestration) {
    let inspect = unit(orchestration.workflow(), "inspect-source");
    let inspect_launch = launch(orchestration, &inspect, "execution:inspect-precondition");
    orchestration
        .start_serial("journey:vak", &inspect, inspect_launch)
        .unwrap();
    let returned = artifact(
        orchestration,
        &inspect,
        "artifact:inspect-precondition",
        "execution:inspect-precondition",
        "evidence:inspect-precondition",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
}

#[test]
fn single_voice_uses_the_native_serial_launch_and_preserves_scope() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let plan = plan(&workflow, "CFP0", &[("inspect-source", "inspect")]);
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:single"),
    )]);
    let performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", plan, launches).unwrap();
    let snapshot = performance.snapshot(&orchestration).unwrap();
    assert_eq!(snapshot.musical_role, "single-voice");
    assert_eq!(snapshot.attempts[0].status, LegStatus::Active);
    assert_eq!(snapshot.actor_ref, "agent:epii");
    assert_eq!(snapshot.ai_kit_resolve_path_ref, "resolve-scoped-path:test");
}

#[test]
fn chord_maps_to_native_independent_fork_and_rejects_scope_widening() {
    let workflow = compiled();
    let mut orchestration = engine(workflow.clone());
    complete_inspect(&mut orchestration);
    let left = unit(&workflow, "implement-compiler");
    let right = unit(&workflow, "review-adversarially");
    let plan = plan(
        &workflow,
        "CFP1",
        &[
            ("implement-compiler", "implementation"),
            ("review-adversarially", "review"),
        ],
    );
    let launches = BTreeMap::from([
        (
            left.clone(),
            launch(&orchestration, &left, "execution:parallel-a"),
        ),
        (
            right.clone(),
            launch(&orchestration, &right, "execution:parallel-b"),
        ),
    ]);
    let performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", plan, launches).unwrap();
    assert_eq!(
        performance.snapshot(&orchestration).unwrap().attempts.len(),
        2
    );

    let mut invalid = plan(
        &workflow,
        "CFP1",
        &[
            ("implement-compiler", "implementation"),
            ("review-adversarially", "review"),
        ],
    );
    invalid.units[0]
        .source_refs
        .insert("source:not-in-parent".into());
    assert!(invalid.validate(&workflow).is_err());
}

#[test]
fn melody_requires_actual_predecessor_return_before_next_native_launch() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let implement = unit(&workflow, "implement-compiler");
    let mut orchestration = engine(workflow.clone());
    let plan = plan(
        &workflow,
        "CFP2",
        &[
            ("inspect-source", "inspect"),
            ("implement-compiler", "implementation"),
        ],
    );
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:chain-a"),
    )]);
    let mut performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", plan, launches).unwrap();
    let premature = launch(&orchestration, &implement, "execution:chain-b");
    assert!(
        performance
            .continue_sequence(&mut orchestration, "journey:vak", premature)
            .is_err()
    );
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:chain-a",
        "execution:chain-a",
        "evidence:chain-a",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let continuation = launch(&orchestration, &implement, "execution:chain-b");
    let next = performance
        .continue_sequence(&mut orchestration, "journey:vak", continuation)
        .unwrap();
    assert_eq!(next, implement);
}

#[test]
fn fusion_preserves_both_voices_then_uses_native_barrier_and_independent_synthesis() {
    let workflow = compiled();
    let mut orchestration = engine(workflow.clone());
    complete_inspect(&mut orchestration);
    let left = unit(&workflow, "implement-compiler");
    let right = unit(&workflow, "review-adversarially");
    let fusion_plan = plan(
        &workflow,
        "CFP3",
        &[
            ("implement-compiler", "implementation"),
            ("review-adversarially", "review"),
        ],
    );
    let launches = BTreeMap::from([
        (
            left.clone(),
            launch(&orchestration, &left, "execution:fusion-a"),
        ),
        (
            right.clone(),
            launch(&orchestration, &right, "execution:fusion-b"),
        ),
    ]);
    let performance = NativeVakPerformance::start(
        &mut orchestration,
        "journey:vak",
        fusion_plan,
        launches,
    )
    .unwrap();
    let left_return = artifact(
        &orchestration,
        &left,
        "artifact:fusion-a",
        "execution:fusion-a",
        "evidence:fusion-a",
    );
    orchestration.return_artifact(&left, left_return).unwrap();
    let right_return = artifact(
        &orchestration,
        &right,
        "artifact:fusion-b",
        "execution:fusion-b",
        "evidence:fusion-b",
    );
    orchestration.return_artifact(&right, right_return).unwrap();
    let reviewer = disposition_independent(
        orchestration.run(),
        None,
        BTreeSet::from(["execution:fusion-a".into(), "execution:fusion-b".into()]),
    );
    let (_, synthesis) = performance
        .synthesize_fusion(
            &mut orchestration,
            "execution:fusion-reviewer",
            &reviewer,
            "synthesis:fusion",
            BTreeSet::from(["artifact:fusion-a".into(), "artifact:fusion-b".into()]),
            BTreeSet::from(["evidence:fusion-a".into(), "evidence:fusion-b".into()]),
            "integrated difference preserving both readings",
        )
        .unwrap();
    assert_eq!(synthesis.artifact_refs.len(), 2);
    assert_eq!(
        performance.snapshot(&orchestration).unwrap().musical_role,
        "fusion"
    );
}

#[test]
fn sustained_drone_requires_continuation_and_resumes_only_through_native_retry_grant() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let retry = sustained_retry_grant("retry:sustained", 2).unwrap();
    let first = ExecutionLaunch {
        execution_ref: "execution:sustain-1".into(),
        disposition: disposition(orchestration.run(), Some(&inspect)),
        retry_grant: Some(retry.clone()),
    };
    let mut performance = NativeVakPerformance::start(
        &mut orchestration,
        "journey:vak",
        plan(&workflow, "CFP4", &[("inspect-source", "sustained")]),
        BTreeMap::from([(inspect.clone(), first)]),
    )
    .unwrap();
    orchestration
        .fail(&inspect, "provider interrupted")
        .unwrap();
    let second = ExecutionLaunch {
        execution_ref: "execution:sustain-2".into(),
        disposition: disposition(orchestration.run(), Some(&inspect)),
        retry_grant: Some(retry),
    };
    performance
        .resume_sustained(
            &mut orchestration,
            "journey:vak",
            "retry:sustained",
            second,
        )
        .unwrap();
    assert_eq!(orchestration.leg(&inspect).unwrap().attempts.len(), 2);
}

#[test]
fn canon_uses_compiled_native_nesting_and_never_invents_a_child_scope() {
    let workflow = nested_compiled();
    let inspect = unit(&workflow, "inspect-source");
    let implement = unit(&workflow, "implement-compiler");
    let mut orchestration = engine(workflow.clone());
    let nested_plan = plan(
        &workflow,
        "CFP5",
        &[
            ("inspect-source", "root"),
            ("implement-compiler", "child"),
        ],
    );
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:nested-root"),
    )]);
    let mut performance = NativeVakPerformance::start(
        &mut orchestration,
        "journey:vak",
        nested_plan,
        launches,
    )
    .unwrap();
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:nested-root",
        "execution:nested-root",
        "evidence:nested-root",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let child_launch = launch(&orchestration, &implement, "execution:nested-child");
    assert_eq!(
        performance
            .continue_sequence(&mut orchestration, "journey:vak", child_launch)
            .unwrap(),
        implement
    );
}

#[test]
fn z_cycle_rehears_actual_evidence_and_requires_a_changed_binding_to_recompose() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let plan = plan(&workflow, "CFP0", &[("inspect-source", "z")]);
    let mut z = VakZCycle::compose("z:test", &plan).unwrap();
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:z"),
    )]);
    let performance = NativeVakPerformance::start(
        &mut orchestration,
        "journey:vak",
        plan.clone(),
        launches,
    )
    .unwrap();
    z.performing(&performance.snapshot(&orchestration).unwrap())
        .unwrap();
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:z",
        "execution:z",
        "evidence:z",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let snapshot = performance.snapshot(&orchestration).unwrap();
    z.record(&snapshot).unwrap();
    z.rehear(BTreeSet::from(["evidence:z".into()])).unwrap();
    assert!(z.recompose(&plan.binding, &plan.binding).is_err());
    let mut next = plan.binding.clone();
    next.ql_binding_revision = "ql-revision-2".into();
    z.recompose(&plan.binding, &next).unwrap();
    assert_eq!(z.stage, ZStage::Recomposed);
}

#[test]
fn c_prime_metadata_cannot_grant_autonomous_authority_or_change_native_subject() {
    let workflow = compiled();
    let mut invalid = plan(&workflow, "CFP0", &[("inspect-source", "invalid")]);
    invalid.binding.undertaking_authority_ref = None;
    assert!(invalid.validate(&workflow).is_err());
    invalid.binding.participation = "dialogical".into();
    invalid.units[0].subject_ref = "project:another".into();
    assert!(invalid.validate(&workflow).is_err());
}
