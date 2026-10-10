use epilogos_factory::core::run::{
    ProjectRef, Run, RunRef, RunThought, RunThoughtCommand, RunThoughtId, RunThoughtLifecycle,
    ThoughtConsumptionInput, ThoughtConsumptionSources, ThoughtFieldError, ThoughtProducer,
    ThoughtSource, ThoughtSourceObservation, ThoughtUse, ThoughtUseKind, WorkflowUnitRef,
};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, ExecutionDisposition,
    AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::journey::{Journey, JourneyCommission};
use epilogos_factory::journey_praxis::{
    JourneyMethodProofCorrelation, JourneyPraxisContext, AIKIT_METHOD_PROOF_SCHEMA,
};
use epilogos_factory::orchestration::{
    ExecutableOrchestration, ExecutionLaunch, LegStatus, ReturnedArtifact,
};
use epilogos_factory::vak_orchestration::{
    derive_conduct_plans, sustained_retry_grant, vak_scope_tracking, CPrimeExecutionBinding,
    NativeVakPerformance, VakChainInputBinding, VakConductPlan, VakThoughtConsumptionRequest,
    VakUnitScope, VakZCycle, ZStage, AIKIT_OPERATIVE_SCOPE_CONTRACT, QL_C_PRIME_PROFILE_CONTRACT,
    VAK_CHAIN_INPUT_TRACKING_KIND, VAK_ORCHESTRATION_CONTRACT, VAK_SCOPE_TRACKING_KIND,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowNestingSource,
    WorkflowSource, WorkflowUnitSource,
};
use epilogos_factory::workflow_authoring::load_workflow;
use epilogos_factory::workflow_inputs::WorkflowInputSource;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";

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
        RUN.parse::<RunRef>().unwrap(),
        PROJECT.parse::<ProjectRef>().unwrap(),
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
        subject_ref: PROJECT.into(),
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
        chain_inputs: if thread == "CFP2" {
            keys.windows(2)
                .map(|pair| VakChainInputBinding {
                    predecessor_unit_ref: unit(workflow, pair[0].0),
                    successor_unit_ref: unit(workflow, pair[1].0),
                    receiving_context_ref: format!(
                        "receiving-context:{}-to-{}",
                        pair[0].0, pair[1].0
                    ),
                })
                .collect()
        } else {
            Vec::new()
        },
        continuation_ref: (thread == "CFP4").then(|| "continuation:sustained".into()),
        stop_condition_ref: (thread == "CFP4").then(|| "stop:sustained".into()),
        fusion_barrier_ref: (thread == "CFP3").then(|| "implementation-reviewed".into()),
        parent_performance_ref: (thread == "CFP5").then(|| "performance:outer".into()),
    }
}
fn disposition_provider(
    run: &Run,
    unit: Option<&WorkflowUnitRef>,
    independence_from: BTreeSet<String>,
    provider: &str,
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
            model_ref: format!("model:{provider}"),
            provider_ref: format!("provider:{provider}"),
            ranking_policy: "test-contract".into(),
            ranking_explanation: json!({"eligible": true}),
            provenance: vec![format!("selection:{provider}")],
        },
        "2026-09-13T20:00:00+01:00",
    )
    .unwrap()
}
fn disposition_independent(
    run: &Run,
    unit: Option<&WorkflowUnitRef>,
    independence_from: BTreeSet<String>,
) -> ExecutionDisposition {
    disposition_provider(run, unit, independence_from, "factory-vak-test")
}
fn launch(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
) -> ExecutionLaunch {
    launch_provider(orchestration, unit, execution_ref, "factory-vak-test")
}
fn launch_provider(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
    provider: &str,
) -> ExecutionLaunch {
    ExecutionLaunch {
        execution_ref: execution_ref.into(),
        disposition: disposition_provider(
            orchestration.run(),
            Some(unit),
            BTreeSet::new(),
            provider,
        ),
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
fn thought_source(owner: &str, reference: &str, revision: &str) -> ThoughtSource {
    ThoughtSource {
        owner: owner.into(),
        reference: reference.into(),
        revision: revision.into(),
    }
}

struct CurrentSources;
impl ThoughtConsumptionSources for CurrentSources {
    fn observe(
        &self,
        source: &ThoughtSource,
    ) -> Result<ThoughtSourceObservation, ThoughtFieldError> {
        Ok(ThoughtSourceObservation::Current {
            source: source.clone(),
            receipt: ThoughtSource {
                owner: "fixture-native-observer".into(),
                reference: format!("observation:{}", source.reference),
                revision: format!("observed:{}", source.revision),
            },
        })
    }
}

#[test]
fn single_voice_uses_native_serial_work_and_preserves_exact_scope_and_source_basis() {
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
    assert_eq!(snapshot.workflow_source_revision, "source-revision-7");
    assert_eq!(snapshot.attempts[0].whole_ref, "whole:inspect");
    assert_eq!(snapshot.attempts[0].subject_ref, PROJECT);
}

#[test]
fn chord_is_independent_parallelism_and_partial_failure_does_not_relabel_siblings() {
    let workflow = compiled();
    let mut orchestration = engine(workflow.clone());
    complete_inspect(&mut orchestration);
    let left = unit(&workflow, "implement-compiler");
    let right = unit(&workflow, "review-adversarially");
    let chord_plan = plan(
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
        NativeVakPerformance::start(&mut orchestration, "journey:vak", chord_plan, launches)
            .unwrap();

    // Return the second voice first: completion order must not become identity.
    let returned = artifact(
        &orchestration,
        &right,
        "artifact:parallel-b",
        "execution:parallel-b",
        "evidence:parallel-b",
    );
    orchestration.return_artifact(&right, returned).unwrap();
    orchestration.fail(&left, "implementation failed").unwrap();
    let snapshot = performance.snapshot(&orchestration).unwrap();
    assert!(snapshot.settled());
    let left_reading = snapshot
        .attempts
        .iter()
        .find(|attempt| attempt.unit_ref == left)
        .unwrap();
    let right_reading = snapshot
        .attempts
        .iter()
        .find(|attempt| attempt.unit_ref == right)
        .unwrap();
    assert_eq!(left_reading.status, LegStatus::Failed);
    assert_eq!(right_reading.status, LegStatus::Returned);
    assert_eq!(left_reading.whole_ref, "whole:implementation");
    assert_eq!(right_reading.whole_ref, "whole:review");
    assert_eq!(left_reading.actor_ref, right_reading.actor_ref);

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
fn melody_consumes_selected_current_predecessor_result_not_a_label_or_late_result() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let implement = unit(&workflow, "implement-compiler");
    let mut orchestration = engine(workflow.clone());
    let chain_plan = plan(
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
        NativeVakPerformance::start(&mut orchestration, "journey:vak", chain_plan, launches)
            .unwrap();
    let premature = launch(&orchestration, &implement, "execution:chain-premature");
    assert!(performance
        .continue_chain(
            &mut orchestration,
            "journey:vak",
            BTreeSet::from(["artifact:chain-a".into()]),
            premature,
        )
        .is_err());
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:chain-a",
        "execution:chain-a",
        "evidence:chain-a",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let invalid = launch(&orchestration, &implement, "execution:chain-invalid");
    assert!(performance
        .continue_chain(
            &mut orchestration,
            "journey:vak",
            BTreeSet::from(["artifact:not-returned".into()]),
            invalid,
        )
        .is_err());
    let continuation = launch(&orchestration, &implement, "execution:chain-b");
    let material = performance
        .continue_chain(
            &mut orchestration,
            "journey:vak",
            BTreeSet::from(["artifact:chain-a".into()]),
            continuation,
        )
        .unwrap();
    assert_eq!(material.predecessor_execution_ref, "execution:chain-a");
    assert_eq!(material.successor_unit_ref, implement);
    assert_eq!(
        material.receiving_context_ref,
        "receiving-context:inspect-source-to-implement-compiler"
    );
    assert_eq!(
        material.evidence_refs,
        BTreeSet::from(["evidence:chain-a".into()])
    );
    let input_fact = material
        .tracking_fact(&performance.plan().performance_ref)
        .unwrap();
    assert_eq!(input_fact.kind, VAK_CHAIN_INPUT_TRACKING_KIND);
    assert!(input_fact.evidence_refs.contains("artifact:chain-a"));
    let scope_fact = vak_scope_tracking(performance.plan(), &implement).unwrap();
    assert_eq!(scope_fact.kind, VAK_SCOPE_TRACKING_KIND);
    assert!(scope_fact
        .evidence_refs
        .contains("resolve-scoped-path:implementation"));
    assert_eq!(
        performance
            .snapshot(&orchestration)
            .unwrap()
            .chain_inputs
            .len(),
        1
    );
    let mut unauthored = plan(
        &workflow,
        "CFP2",
        &[
            ("inspect-source", "inspect"),
            ("implement-compiler", "implementation"),
        ],
    );
    unauthored.chain_inputs.clear();
    assert!(unauthored.validate(&workflow).is_err());
}

#[test]
fn fusion_preserves_distinct_readings_then_uses_native_barrier_reviewer_and_synthesis() {
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
    let performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", fusion_plan, launches)
            .unwrap();
    let right_return = artifact(
        &orchestration,
        &right,
        "artifact:fusion-b",
        "execution:fusion-b",
        "evidence:fusion-b",
    );
    orchestration.return_artifact(&right, right_return).unwrap();
    let left_return = artifact(
        &orchestration,
        &left,
        "artifact:fusion-a",
        "execution:fusion-a",
        "evidence:fusion-a",
    );
    orchestration.return_artifact(&left, left_return).unwrap();
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
fn drone_retry_stop_cancellation_and_late_returns_preserve_every_attempt_and_semantic_identity() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let retry = sustained_retry_grant("retry:sustained", 3).unwrap();
    let mut first = launch_provider(
        &orchestration,
        &inspect,
        "execution:sustain-1",
        "provider-a",
    );
    first.retry_grant = Some(retry.clone());
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

    let mut second = launch_provider(
        &orchestration,
        &inspect,
        "execution:sustain-2",
        "provider-b",
    );
    second.retry_grant = Some(retry.clone());
    performance
        .resume_sustained(&mut orchestration, "journey:vak", "retry:sustained", second)
        .unwrap();

    // A late result from the failed first attempt stays on that historical attempt.
    let historical_late = artifact(
        &orchestration,
        &inspect,
        "artifact:sustain-old-late",
        "execution:sustain-1",
        "evidence:sustain-old-late",
    );
    orchestration
        .return_artifact(&inspect, historical_late)
        .unwrap();

    performance
        .stop_sustained(
            &mut orchestration,
            "stop:sustained",
            "central-now-owner",
            "now-r7",
            true,
            BTreeSet::from(["evidence:stop-condition".into()]),
        )
        .unwrap();
    assert_eq!(
        orchestration.leg(&inspect).unwrap().status,
        LegStatus::Quiescent
    );

    // Current work returning after explicit cancellation is retained as late Return.
    let current_late = artifact(
        &orchestration,
        &inspect,
        "artifact:sustain-current-late",
        "execution:sustain-2",
        "evidence:sustain-current-late",
    );
    orchestration
        .return_artifact(&inspect, current_late)
        .unwrap();
    assert_eq!(
        orchestration.leg(&inspect).unwrap().status,
        LegStatus::LateResult
    );

    let mut forbidden = launch_provider(
        &orchestration,
        &inspect,
        "execution:sustain-3",
        "provider-c",
    );
    forbidden.retry_grant = Some(retry);
    assert!(performance
        .resume_sustained(
            &mut orchestration,
            "journey:vak",
            "retry:sustained",
            forbidden,
        )
        .is_err());

    let snapshot = performance.snapshot(&orchestration).unwrap();
    assert_eq!(snapshot.attempts.len(), 2);
    assert!(snapshot.sustained_stop.is_some());
    let first = &snapshot.attempts[0];
    let second = &snapshot.attempts[1];
    assert_eq!(first.status, LegStatus::Failed);
    assert_eq!(first.provider_ref, "provider:provider-a");
    assert!(first
        .late_artifact_refs
        .contains("artifact:sustain-old-late"));
    assert_eq!(second.status, LegStatus::LateResult);
    assert_eq!(second.provider_ref, "provider:provider-b");
    assert!(second
        .late_artifact_refs
        .contains("artifact:sustain-current-late"));
    assert_eq!(first.actor_ref, second.actor_ref);
    assert_eq!(first.subject_ref, second.subject_ref);
    assert_eq!(first.whole_ref, second.whole_ref);
    assert_eq!(first.source_refs, second.source_refs);
    assert_eq!(snapshot.workflow_source_revision, "source-revision-7");
}

#[test]
fn canon_uses_compiled_native_parent_child_nesting_without_inventing_child_scope() {
    let workflow = nested_compiled();
    let inspect = unit(&workflow, "inspect-source");
    let implement = unit(&workflow, "implement-compiler");
    let mut orchestration = engine(workflow.clone());
    let nested_plan = plan(
        &workflow,
        "CFP5",
        &[("inspect-source", "root"), ("implement-compiler", "child")],
    );
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:nested-root"),
    )]);
    let mut performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", nested_plan, launches)
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
            .continue_nested(&mut orchestration, "journey:vak", child_launch)
            .unwrap(),
        implement
    );
    let snapshot = performance.snapshot(&orchestration).unwrap();
    assert_eq!(snapshot.attempts.len(), 2);
    assert_eq!(snapshot.attempts[0].whole_ref, "whole:root");
    assert_eq!(snapshot.attempts[1].whole_ref, "whole:child");
}

#[test]
fn z_cycle_rehears_actual_evidence_and_requires_changed_binding_to_recompose() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let plan = plan(&workflow, "CFP0", &[("inspect-source", "z")]);
    let mut z = VakZCycle::compose("z:test", &plan).unwrap();
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:z"),
    )]);
    let performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", plan.clone(), launches)
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
fn performed_return_is_consumed_by_existing_run_cognition_and_journey_praxis_without_recognition() {
    let workflow = compiled();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let plan = plan(&workflow, "CFP0", &[("inspect-source", "return")]);
    let launches = BTreeMap::from([(
        inspect.clone(),
        launch(&orchestration, &inspect, "execution:return"),
    )]);
    let performance =
        NativeVakPerformance::start(&mut orchestration, "journey:vak", plan, launches).unwrap();

    let thought_id = RunThoughtId::new("question-one").unwrap();
    let thought_expected_revision = orchestration.run().revision();
    let thought_run_ref = orchestration.run().reference().clone();
    orchestration
        .retain_run_thought(RunThoughtCommand {
            command_id: "retain:question-one".into(),
            expected_revision: thought_expected_revision,
            thought: RunThought {
                id: thought_id.clone(),
                run_ref: thought_run_ref,
                anchor_ref: "source:thought".into(),
                anchor_revision: Some("thought-r1".into()),
                passage: None,
                producer: ThoughtProducer {
                    agent_ref: Some("agent:epii".into()),
                    agency_ref: Some("agency:factory-vak-test".into()),
                    agent_session_ref: None,
                    execution_ref: Some("execution:return".into()),
                },
                run_map_subject_refs: vec![inspect.to_string()],
                related_refs: vec!["performance:CFP0".into()],
                relation_evidence_refs: vec!["evidence:question".into()],
                lifecycle: RunThoughtLifecycle::Active,
            },
        })
        .unwrap();

    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:return",
        "execution:return",
        "evidence:return",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();

    performance
        .consume_thoughts(
            &mut orchestration,
            VakThoughtConsumptionRequest {
                command_id: "consume:question-one".into(),
                consumption_id: RunThoughtId::new("consumption-one").unwrap(),
                consumer_ref: "agent:epii".into(),
                working_field: thought_source("central", "now:test", "now-r1"),
                inputs: vec![ThoughtConsumptionInput {
                    thought_id: thought_id.clone(),
                    anchor: thought_source("central", "source:thought", "thought-r1"),
                    interpretation: Some(thought_source("ql-mef", "T0", "ql-vak-r1")),
                }],
                human_response: vec![thought_source(
                    "central",
                    "human-response:test",
                    "response-r1",
                )],
                assessment: thought_source("factory", "assessment:test", "assessment-r1"),
                uses: vec![ThoughtUse {
                    kind: ThoughtUseKind::RecognisedPraxis,
                    source: thought_source("aikit", "skill/recognised-praxis", "skill-r2"),
                    receiving: thought_source("aikit", "recognition:test", "recognition-r1"),
                }],
                retention_policy: thought_source("central", "retention:test", "retention-r1"),
                resulting_lifecycle: RunThoughtLifecycle::Integrated,
            },
            &CurrentSources,
        )
        .unwrap();
    let consumed = orchestration
        .run()
        .thought_field()
        .get(&thought_id)
        .unwrap();
    assert_eq!(consumed.lifecycle, RunThoughtLifecycle::Integrated);
    assert!(orchestration
        .run()
        .thought_field()
        .consumed_by(&thought_id)
        .is_some());

    let snapshot = performance.snapshot(&orchestration).unwrap();
    let mut journey = Journey::new(
        "journey:01ARZ3NDEKTSV4RRFFQ69G5FBE".parse().unwrap(),
        PROJECT.parse().unwrap(),
        JourneyCommission {
            purpose: "Prove Factory Vāk Return".into(),
            commission_ref: Some("commission:vak".into()),
            why_refs: vec!["source:ql-c-prime".into()],
        },
        "Return actual execution evidence",
        "2026-09-13T21:00:00+01:00",
    )
    .unwrap();
    journey
        .add_run(
            orchestration.run().reference().clone(),
            vec![snapshot.performance_ref.clone()],
            vec![],
        )
        .unwrap();
    journey.correlate_activity("activity:factory-vak").unwrap();
    let returned = snapshot
        .journey_return(
            "return:factory-vak",
            "Actual commissioned Vāk performance returned",
        )
        .unwrap();
    assert!(returned.recognition_ref.is_none());
    journey.record_return(returned).unwrap();

    let mut praxis = JourneyPraxisContext::new(&journey);
    let praxis_return = snapshot
        .journey_praxis_return(
            "skill/recognised-praxis",
            "skill-r2",
            vec!["activity:factory-vak".into()],
            vec!["return:factory-vak".into()],
            Some(JourneyMethodProofCorrelation {
                contract: AIKIT_METHOD_PROOF_SCHEMA.into(),
                proof_ref: "proof:factory-vak".into(),
                verification_refs: vec!["evidence:return".into()],
            }),
        )
        .unwrap();
    praxis
        .record_praxis_return(&journey, praxis_return)
        .unwrap();
    assert_eq!(praxis.reading(&journey).unwrap().praxis_returns.len(), 1);
    assert!(journey.recognitions.is_empty());
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

// --- #217: conduct derived from unit compositions ---

fn ql_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ql-vak-workflows")
}

fn compiled_fixture(name: &str) -> CompiledWorkflow {
    load_workflow(&ql_fixtures(), Path::new(name))
        .unwrap_or_else(|diagnostic| panic!("{name}: {diagnostic}"))
        .compiled
}

/// A compiled workflow carrying the given unit compositions, authored input
/// routes and nesting. Each composition is the hand-constructed plan's
/// binding with the unit's own declared participant as actor, so it passes
/// the same compile-time composition law the QL-authored fixtures pass.
fn composed(
    compositions: &[(&str, &str)],
    inputs: &[(&str, &str, &str)],
    nesting: &[(&str, &str)],
) -> CompiledWorkflow {
    let mut source: WorkflowSource = source();
    for (key, thread) in compositions {
        for unit in source.units.iter_mut() {
            if unit.key == *key {
                let mut composition = binding(thread);
                composition.actor_ref = declared_actor(unit);
                unit.composition = Some(composition);
            }
        }
    }
    for (key, predecessor, receiving_context_ref) in inputs {
        for unit in source.units.iter_mut() {
            if unit.key == *key {
                unit.inputs.push(WorkflowInputSource {
                    predecessor: (*predecessor).into(),
                    receiving_context_ref: (*receiving_context_ref).into(),
                });
            }
        }
    }
    for (parent, child) in nesting {
        source.nesting.push(WorkflowNestingSource {
            parent: (*parent).into(),
            child: (*child).into(),
        });
    }
    source.source.digest = workflow_source_digest(&source).unwrap();
    compile_workflow(source).unwrap()
}

fn declared_actor(unit: &WorkflowUnitSource) -> String {
    unit.agent_requirements
        .agent_refs
        .iter()
        .chain(unit.agent_requirements.agent_set_refs.iter())
        .chain(unit.agent_requirements.agency_refs.iter())
        .next()
        .cloned()
        .expect("declared participant")
}

fn composition_actor(workflow: &CompiledWorkflow, key: &str) -> String {
    workflow
        .unit(key)
        .unwrap()
        .composition
        .as_ref()
        .unwrap()
        .actor_ref
        .clone()
}

/// The derived performance carries its opening voice's binding: the hand
/// plan's binding with that voice's declared actor.
fn opening_binding(hand: &CPrimeExecutionBinding, actor: &str) -> CPrimeExecutionBinding {
    let mut expected = hand.clone();
    expected.actor_ref = actor.into();
    expected
}

fn unit_refs(plan: &VakConductPlan) -> Vec<WorkflowUnitRef> {
    plan.units
        .iter()
        .map(|scope| scope.unit_ref.clone())
        .collect()
}

fn scope_keys<'a>(workflow: &'a CompiledWorkflow, plan: &VakConductPlan) -> Vec<&'a str> {
    plan.units
        .iter()
        .map(|scope| {
            workflow
                .units
                .values()
                .find(|compiled| compiled.reference == scope.unit_ref)
                .expect("planned unit")
                .key
                .as_str()
        })
        .collect()
}

/// Every derived scope is the authored C′ of its own unit, addressed at that
/// unit's native subject.
fn assert_scopes_carry_their_authored_composition(
    workflow: &CompiledWorkflow,
    plan: &VakConductPlan,
) {
    for scope in &plan.units {
        let compiled = workflow
            .units
            .values()
            .find(|compiled| compiled.reference == scope.unit_ref)
            .unwrap();
        let composition = compiled.composition.as_ref().unwrap();
        assert_eq!(scope.whole_ref, composition.whole_ref);
        assert_eq!(
            scope.ai_kit_resolve_path_ref,
            composition.ai_kit_resolve_path_ref
        );
        assert_eq!(
            scope.context_resolution_ref,
            composition.context_resolution_ref
        );
        assert_eq!(scope.source_refs, composition.source_refs);
        assert_eq!(scope.subject_ref, compiled.subject_ref.to_string());
    }
}

#[test]
fn derived_conduct_matches_the_hand_constructed_plan_shape_for_every_thread_form() {
    // CFP0 single voice: one unit, one plan, opening-voice binding.
    let workflow = composed(&[("inspect-source", "CFP0")], &[], &[]);
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(&workflow, "CFP0", &[("inspect-source", "inspect")]);
    let derived_plan = &derived.plans()[0];
    assert_eq!(derived_plan.contract, hand.contract);
    assert_eq!(
        derived_plan.binding,
        opening_binding(
            &hand.binding,
            &composition_actor(&workflow, "inspect-source")
        )
    );
    assert_eq!(unit_refs(derived_plan), unit_refs(&hand));
    assert_eq!(derived_plan.chain_inputs, hand.chain_inputs);
    assert_eq!(derived_plan.continuation_ref, None);
    assert_eq!(derived_plan.stop_condition_ref, None);
    assert_eq!(derived_plan.fusion_barrier_ref, hand.fusion_barrier_ref);
    assert_eq!(derived_plan.parent_performance_ref, None);
    assert_eq!(
        derived_plan.performance_ref,
        format!(
            "performance:{}:{}@{}:{}:CFP0",
            workflow.workflow_key,
            hand.binding.ql_binding_ref,
            hand.binding.ql_binding_revision,
            hand.binding.whole_ref
        )
    );
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // CFP1 chord: the same two independent voices, forked together.
    let workflow = composed(
        &[
            ("implement-compiler", "CFP1"),
            ("review-adversarially", "CFP1"),
        ],
        &[],
        &[],
    );
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(
        &workflow,
        "CFP1",
        &[
            ("implement-compiler", "implementation"),
            ("review-adversarially", "review"),
        ],
    );
    let derived_plan = &derived.plans()[0];
    // Fork order is ascending unit ref; the opening voice carries the binding.
    let left = unit(&workflow, "implement-compiler");
    let right = unit(&workflow, "review-adversarially");
    let opening_key = if left <= right {
        "implement-compiler"
    } else {
        "review-adversarially"
    };
    assert_eq!(
        derived_plan.binding,
        opening_binding(&hand.binding, &composition_actor(&workflow, opening_key))
    );
    assert_eq!(
        unit_refs(derived_plan)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        unit_refs(&hand).iter().cloned().collect::<BTreeSet<_>>()
    );
    assert_eq!(derived_plan.fusion_barrier_ref, None);
    assert_eq!(derived_plan.parent_performance_ref, None);
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // CFP2 melody: chain order with one authored input binding per transition.
    let workflow = composed(
        &[("inspect-source", "CFP2"), ("implement-compiler", "CFP2")],
        &[(
            "implement-compiler",
            "inspect-source",
            "receiving-context:inspect-source-to-implement-compiler",
        )],
        &[],
    );
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(
        &workflow,
        "CFP2",
        &[
            ("inspect-source", "inspect"),
            ("implement-compiler", "implementation"),
        ],
    );
    let derived_plan = &derived.plans()[0];
    assert_eq!(
        derived_plan.binding,
        opening_binding(
            &hand.binding,
            &composition_actor(&workflow, "inspect-source")
        )
    );
    assert_eq!(unit_refs(derived_plan), unit_refs(&hand));
    assert_eq!(derived_plan.chain_inputs, hand.chain_inputs);
    assert_eq!(derived_plan.chain_inputs.len(), 1);
    assert_eq!(
        derived_plan.chain_inputs[0].receiving_context_ref,
        "receiving-context:inspect-source-to-implement-compiler"
    );
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // CFP3 fusion: the independent readings wait at the one compiled barrier.
    let workflow = composed(
        &[
            ("implement-compiler", "CFP3"),
            ("review-adversarially", "CFP3"),
        ],
        &[],
        &[],
    );
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(
        &workflow,
        "CFP3",
        &[
            ("implement-compiler", "implementation"),
            ("review-adversarially", "review"),
        ],
    );
    let derived_plan = &derived.plans()[0];
    let left = unit(&workflow, "implement-compiler");
    let right = unit(&workflow, "review-adversarially");
    let opening_key = if left <= right {
        "implement-compiler"
    } else {
        "review-adversarially"
    };
    assert_eq!(
        derived_plan.binding,
        opening_binding(&hand.binding, &composition_actor(&workflow, opening_key))
    );
    assert_eq!(
        unit_refs(derived_plan)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        unit_refs(&hand).iter().cloned().collect::<BTreeSet<_>>()
    );
    assert_eq!(
        derived_plan.fusion_barrier_ref,
        hand.fusion_barrier_ref.clone()
    );
    assert_eq!(
        derived_plan.fusion_barrier_ref,
        Some("implementation-reviewed".into())
    );
    assert_eq!(derived_plan.parent_performance_ref, None);
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // CFP4 sustained: the continuation and stop condition name the unit.
    let workflow = composed(&[("inspect-source", "CFP4")], &[], &[]);
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(&workflow, "CFP4", &[("inspect-source", "sustained")]);
    let derived_plan = &derived.plans()[0];
    assert_eq!(
        derived_plan.binding,
        opening_binding(
            &hand.binding,
            &composition_actor(&workflow, "inspect-source")
        )
    );
    assert_eq!(unit_refs(derived_plan), unit_refs(&hand));
    let inspect = unit(&workflow, "inspect-source").to_string();
    assert_eq!(
        derived_plan.continuation_ref,
        Some(format!("continuation:{inspect}"))
    );
    assert_eq!(
        derived_plan.stop_condition_ref,
        Some(format!("stop:{inspect}"))
    );
    assert_eq!(derived_plan.fusion_barrier_ref, None);
    assert_eq!(derived_plan.parent_performance_ref, None);
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // CFP5 canon: root first, child in nested continuation order.
    let workflow = composed(
        &[("inspect-source", "CFP5"), ("implement-compiler", "CFP5")],
        &[],
        &[("inspect-source", "implement-compiler")],
    );
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    let hand = plan(
        &workflow,
        "CFP5",
        &[("inspect-source", "root"), ("implement-compiler", "child")],
    );
    let derived_plan = &derived.plans()[0];
    assert_eq!(
        derived_plan.binding,
        opening_binding(
            &hand.binding,
            &composition_actor(&workflow, "inspect-source")
        )
    );
    assert_eq!(unit_refs(derived_plan), unit_refs(&hand));
    assert_eq!(
        scope_keys(&workflow, derived_plan),
        ["inspect-source", "implement-compiler"]
    );
    assert_eq!(
        derived_plan.parent_performance_ref,
        Some(format!(
            "performance:{}:{}@{}:{}:parent",
            workflow.workflow_key,
            hand.binding.ql_binding_ref,
            hand.binding.ql_binding_revision,
            hand.binding.whole_ref
        ))
    );
    assert_eq!(derived_plan.chain_inputs, Vec::new());
    assert_scopes_carry_their_authored_composition(&workflow, derived_plan);

    // Determinism: the same compiled content derives the same plans.
    assert_eq!(derive_conduct_plans(&workflow).unwrap(), derived);
}

#[test]
fn ql_fixture_workflows_derive_their_authored_conduct_plans() {
    // Expression development: a five-voice melody, a two-voice chord and the
    // returning single voice, all under one interpretation.
    let workflow = compiled_fixture("expression-development.workflow.ts");
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 3);
    let melody = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP2")
        .unwrap();
    assert_eq!(
        scope_keys(&workflow, melody),
        [
            "nous-open-ground",
            "logos-articulate",
            "anima-conduct",
            "psyche-continuity",
            "sophia-integrate"
        ]
    );
    assert_eq!(melody.binding.actor_ref, "agent/anima-nous");
    let receiving = melody
        .chain_inputs
        .iter()
        .map(|input| input.receiving_context_ref.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        receiving,
        [
            "context:expression-development/logos-from-nous",
            "context:expression-development/anima-from-logos",
            "context:expression-development/psyche-from-anima",
            "context:expression-development/sophia-from-psyche",
        ]
    );
    let chord = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP1")
        .unwrap();
    assert_eq!(
        scope_keys(&workflow, chord)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["eros-exchange", "mythos-image"])
    );
    let single = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP0")
        .unwrap();
    assert_eq!(scope_keys(&workflow, single), ["aletheia-disclose-return"]);
    assert_eq!(single.binding.actor_ref, "agent/aletheia");

    // Technē constellation: blueprint → aggregate → next form behind the
    // three fused readings at the readings-fuse barrier, then the return.
    let workflow = compiled_fixture("techne-constellation.workflow.ts");
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 3);
    let melody = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP2")
        .unwrap();
    assert_eq!(
        scope_keys(&workflow, melody),
        ["anansi-blueprint", "agora-aggregate", "zeithoven-next-form"]
    );
    assert_eq!(
        melody
            .chain_inputs
            .iter()
            .map(|input| input.receiving_context_ref.as_str())
            .collect::<Vec<_>>(),
        [
            "context:techne-constellation/agora-from-anansi",
            "context:techne-constellation/zeithoven-from-agora",
        ]
    );
    let fusion = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP3")
        .unwrap();
    assert_eq!(
        scope_keys(&workflow, fusion)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "moirai-distil",
            "mercurius-translate",
            "janus-temporal-seam"
        ])
    );
    assert_eq!(fusion.fusion_barrier_ref, Some("readings-fuse".into()));
    for scope in &fusion.units {
        assert_eq!(scope.subject_ref, "central:wiki:root");
    }
    assert!(derived
        .plans()
        .iter()
        .any(|candidate| candidate.binding.thread == "CFP0"
            && scope_keys(&workflow, candidate) == ["aletheia-return-revision"]));

    // Development and knowledge: one five-voice melody and the returning
    // single voice.
    let workflow = compiled_fixture("development-and-knowledge.workflow.ts");
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 2);
    let melody = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP2")
        .unwrap();
    assert_eq!(
        scope_keys(&workflow, melody),
        [
            "logos-scope-plan",
            "techne-implement",
            "eros-verify",
            "mythos-debug",
            "sophia-finish"
        ]
    );
    assert!(derived
        .plans()
        .iter()
        .any(|candidate| candidate.binding.thread == "CFP0"
            && scope_keys(&workflow, candidate) == ["aletheia-knowledge-return"]));

    // Opening resolution: each derived plan is found by exactly its own
    // opening launch set, and nothing else resolves.
    let melody = derived
        .plans()
        .iter()
        .find(|candidate| candidate.binding.thread == "CFP2")
        .unwrap();
    let opening = melody.units[0].unit_ref.clone();
    let second = melody.units[1].unit_ref.clone();
    assert_eq!(
        derived
            .plan_for_units(&BTreeSet::from([opening]))
            .map(|plan| plan.performance_ref.clone()),
        Some(melody.performance_ref.clone())
    );
    assert!(derived.plan_for_units(&BTreeSet::from([second])).is_none());

    // Determinism across the whole fixture set.
    assert_eq!(derive_conduct_plans(&workflow).unwrap(), derived);
}

#[test]
fn workflows_without_compositions_derive_nothing_and_the_default_path_stands() {
    let workflow = compiled();
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert!(derived.is_empty());
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let default_launch = launch(&orchestration, &inspect, "execution:default");
    let started = NativeVakPerformance::start_derived(
        &mut orchestration,
        "journey:vak",
        &derived,
        BTreeMap::from([(inspect.clone(), default_launch)]),
    )
    .unwrap();
    assert!(started.is_none());
    assert!(orchestration.leg(&inspect).is_none());
    // The plain bounded-coordination path is exactly as before.
    orchestration
        .start_serial(
            "journey:vak",
            &inspect,
            launch(&orchestration, &inspect, "execution:plain"),
        )
        .unwrap();
    assert_eq!(
        orchestration.leg(&inspect).unwrap().status,
        LegStatus::Active
    );

    // A mixed workflow: composed units derive plans; plain units stay
    // invisible to derivation and to derived starts.
    let workflow = composed(&[("inspect-source", "CFP0")], &[], &[]);
    let derived = derive_conduct_plans(&workflow).unwrap();
    assert_eq!(derived.plans().len(), 1);
    assert_eq!(
        scope_keys(&workflow, &derived.plans()[0]),
        ["inspect-source"]
    );
    let mut orchestration = engine(workflow.clone());
    let review = unit(&workflow, "review-adversarially");
    let plain_launch = launch(&orchestration, &review, "execution:plain-voice");
    let started = NativeVakPerformance::start_derived(
        &mut orchestration,
        "journey:vak",
        &derived,
        BTreeMap::from([(review.clone(), plain_launch)]),
    )
    .unwrap();
    assert!(started.is_none());
    assert!(orchestration.leg(&review).is_none());
}

#[test]
fn derived_conduct_starts_through_the_runtime_and_keeps_the_z_recompose_law() {
    let workflow = composed(&[("inspect-source", "CFP0")], &[], &[]);
    let derived = derive_conduct_plans(&workflow).unwrap();
    let derived_plan = derived.plans()[0].clone();
    let inspect = unit(&workflow, "inspect-source");
    let mut orchestration = engine(workflow.clone());
    let inspect_launch = launch(&orchestration, &inspect, "execution:derived");
    let performance = NativeVakPerformance::start_derived(
        &mut orchestration,
        "journey:vak",
        &derived,
        BTreeMap::from([(inspect.clone(), inspect_launch)]),
    )
    .unwrap()
    .expect("derived single voice starts");
    assert_eq!(
        performance.plan().performance_ref,
        derived_plan.performance_ref
    );
    let snapshot = performance.snapshot(&orchestration).unwrap();
    assert_eq!(snapshot.musical_role, "single-voice");
    assert_eq!(snapshot.attempts[0].status, LegStatus::Active);
    assert_eq!(snapshot.ql_binding_ref, derived_plan.binding.ql_binding_ref);

    // The derived plan drives the same Z cycle law: rehearsed evidence only
    // recomposes onto a changed binding.
    let mut z = VakZCycle::compose("z:derived", &derived_plan).unwrap();
    z.performing(&snapshot).unwrap();
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:derived",
        "execution:derived",
        "evidence:derived",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let snapshot = performance.snapshot(&orchestration).unwrap();
    z.record(&snapshot).unwrap();
    z.rehear(BTreeSet::from(["evidence:derived".into()]))
        .unwrap();
    assert!(z
        .recompose(&derived_plan.binding, &derived_plan.binding)
        .is_err());
    let mut next = derived_plan.binding.clone();
    next.ql_binding_revision = "ql-revision-2".into();
    z.recompose(&derived_plan.binding, &next).unwrap();
    assert_eq!(z.stage, ZStage::Recomposed);
}

#[test]
fn derived_melody_continues_through_its_authored_chain_inputs() {
    let workflow = composed(
        &[("inspect-source", "CFP2"), ("implement-compiler", "CFP2")],
        &[(
            "implement-compiler",
            "inspect-source",
            "receiving-context:inspect-source-to-implement-compiler",
        )],
        &[],
    );
    let derived = derive_conduct_plans(&workflow).unwrap();
    let inspect = unit(&workflow, "inspect-source");
    let implement = unit(&workflow, "implement-compiler");
    let mut orchestration = engine(workflow.clone());
    let opening_launch = launch(&orchestration, &inspect, "execution:derived-chain-a");
    let mut performance = NativeVakPerformance::start_derived(
        &mut orchestration,
        "journey:vak",
        &derived,
        BTreeMap::from([(inspect.clone(), opening_launch)]),
    )
    .unwrap()
    .expect("derived melody opens");
    let returned = artifact(
        &orchestration,
        &inspect,
        "artifact:derived-chain-a",
        "execution:derived-chain-a",
        "evidence:derived-chain-a",
    );
    orchestration.return_artifact(&inspect, returned).unwrap();
    let continuation = launch(&orchestration, &implement, "execution:derived-chain-b");
    let material = performance
        .continue_chain(
            &mut orchestration,
            "journey:vak",
            BTreeSet::from(["artifact:derived-chain-a".into()]),
            continuation,
        )
        .unwrap();
    assert_eq!(
        material.receiving_context_ref,
        "receiving-context:inspect-source-to-implement-compiler"
    );
    assert_eq!(material.successor_unit_ref, implement);
    assert_eq!(
        performance
            .snapshot(&orchestration)
            .unwrap()
            .chain_inputs
            .len(),
        1
    );
}
