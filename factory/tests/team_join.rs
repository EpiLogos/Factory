//! A team step combines its members' returns by its Central set's Join (O-I #684, DLW manual §19.15).
//!
//! A barrier whose awaited units all draw their agents from one Central agent set is that team's step. The set's `join` is
//! read at the Run's start and kept with the Run, so a later change to the set does not alter a running step. Factory's
//! barrier honours All and First; the manual names no count for Quorum and no member weights for Weighted, so those are
//! refused at the start with the reason, and nothing stands in for them.
use epilogos_factory::core::run::{ProjectRef, Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    BarrierJoin, BarrierState, ExecutableOrchestration, ExecutionLaunch, OrchestrationError,
    OrchestrationSnapshot, ReturnedArtifact, SetJoinReading, WholeRunState,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const TEAM: &str = "agent-set:01ARZ3NDEKTSV4RRFFQ69G5FB1";
const STEP: &str = "implementation-reviewed";

/// The fixture's fork, with both members drawing their agents from one Central set: the step gathers that team's returns.
fn team_source() -> WorkflowSource {
    let mut source: WorkflowSource = serde_json::from_str(FIXTURE).unwrap();
    for unit in source
        .units
        .iter_mut()
        .filter(|unit| unit.key == "implement-compiler" || unit.key == "review-adversarially")
    {
        unit.agent_requirements.agent_set_refs = vec![TEAM.into()];
    }
    source.source.digest = workflow_source_digest(&source).unwrap();
    source
}

fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse::<RunRef>().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW"
            .parse::<ProjectRef>()
            .unwrap(),
        "Factory team join acceptance",
        "factory-orchestrator",
    )
    .unwrap()
}

fn compiled(source: WorkflowSource) -> CompiledWorkflow {
    compile_workflow(source).unwrap()
}

fn engine() -> ExecutableOrchestration {
    ExecutableOrchestration::new(compiled(team_source()), run()).unwrap()
}

fn unit(workflow: &CompiledWorkflow, key: &str) -> WorkflowUnitRef {
    workflow.unit(key).unwrap().reference.clone()
}

fn read(join: Option<BarrierJoin>) -> SetJoinReading {
    SetJoinReading {
        agent_set_ref: TEAM.into(),
        revision: "r3".into(),
        join,
    }
}

fn launch(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
) -> ExecutionLaunch {
    let run = orchestration.run();
    ExecutionLaunch {
        execution_ref: execution_ref.into(),
        disposition: accept_aikit_selection(
            ExecutionDemand {
                project_ref: run.project_ref().to_string(),
                run_ref: run.reference().to_string(),
                workflow_unit_ref: Some(unit.to_string()),
                agency_ref: Some("agency:factory-test".into()),
                profile_ref: Some("profile:orchestration".into()),
                use_type: "factory-orchestration-test".into(),
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
                model_ref: "model:factory-test".into(),
                provider_ref: "provider:factory-test".into(),
                ranking_policy: "test-contract".into(),
                ranking_explanation: json!({"eligible": true}),
                provenance: vec!["selection:factory-test".into()],
            },
            "2026-10-30T10:00:00+00:00",
        )
        .unwrap(),
        retry_grant: None,
    }
}

fn returned(
    orchestration: &mut ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
) {
    let compiled = orchestration
        .workflow()
        .units
        .values()
        .find(|candidate| &candidate.reference == unit)
        .unwrap()
        .clone();
    orchestration
        .return_artifact(
            unit,
            ReturnedArtifact {
                artifact_ref: format!("artifact:{execution_ref}"),
                subject_ref: compiled.subject_ref.to_string(),
                subject_revision: compiled.basis_revision.clone(),
                producing_execution_ref: execution_ref.into(),
                evidence_refs: BTreeSet::from([format!("evidence:{execution_ref}")]),
                semantic_difference: format!("resolved {}", compiled.developmental_concern),
            },
        )
        .unwrap();
}

/// Inspect returns, then both members of the team step start together.
fn start_step(orchestration: &mut ExecutableOrchestration) -> (WorkflowUnitRef, WorkflowUnitRef) {
    let inspect = unit(orchestration.workflow(), "inspect-source");
    let first = launch(orchestration, &inspect, "execution:inspect");
    orchestration
        .start_serial("journey:factory-684", &inspect, first)
        .unwrap();
    returned(orchestration, &inspect, "execution:inspect");
    let implementation = unit(orchestration.workflow(), "implement-compiler");
    let review = unit(orchestration.workflow(), "review-adversarially");
    let launches = BTreeMap::from([
        (
            implementation.clone(),
            launch(orchestration, &implementation, "execution:implement"),
        ),
        (
            review.clone(),
            launch(orchestration, &review, "execution:review"),
        ),
    ]);
    orchestration
        .fork(
            "journey:factory-684",
            &[implementation.clone(), review.clone()],
            launches,
        )
        .unwrap();
    (implementation, review)
}

#[test]
fn a_step_whose_members_share_one_central_set_is_that_teams_step() {
    assert_eq!(
        engine().team_steps(),
        BTreeMap::from([(STEP.to_string(), TEAM.to_string())])
    );
    // In the plain fixture one member draws from the set and the other is a named agent: no one set's team.
    let plain: WorkflowSource = serde_json::from_str(FIXTURE).unwrap();
    let plain = ExecutableOrchestration::new(compiled(plain), run()).unwrap();
    assert!(plain.team_steps().is_empty());
    assert!(matches!(
        plain
            .clone()
            .record_step_join(STEP, read(Some(BarrierJoin::First))),
        Err(OrchestrationError::NotATeamStep(_))
    ));
}

#[test]
fn first_closes_the_step_on_the_first_acceptable_return() {
    let mut orchestration = engine();
    orchestration
        .record_step_join(STEP, read(Some(BarrierJoin::First)))
        .unwrap();
    let (implementation, review) = start_step(&mut orchestration);
    assert_eq!(
        orchestration.barrier_reading(STEP).unwrap().state,
        BarrierState::Pending
    );

    returned(&mut orchestration, &implementation, "execution:implement");
    let reading = orchestration.barrier_reading(STEP).unwrap();
    assert_eq!(reading.state, BarrierState::Complete);
    assert_eq!(reading.join, BarrierJoin::First);
    assert_eq!(reading.closed_by.as_ref(), Some(&implementation));
    assert_eq!(
        reading.joined_units,
        BTreeSet::from([implementation.clone()])
    );
    assert_eq!(reading.agent_set_ref.as_deref(), Some(TEAM));

    // The step released what it gates while the other member still works.
    let integrate = unit(orchestration.workflow(), "integrate-return");
    let next = launch(&orchestration, &integrate, "execution:integrate");
    orchestration
        .start_serial("journey:factory-684", &integrate, next)
        .unwrap();

    // A member that fails after the step closed does not reopen it or fail the Run.
    orchestration
        .fail(&review, "lost the race and stopped")
        .unwrap();
    assert_eq!(
        orchestration.barrier_reading(STEP).unwrap().state,
        BarrierState::Complete
    );
    assert_ne!(orchestration.whole_run_state(), WholeRunState::Failed);
    returned(&mut orchestration, &integrate, "execution:integrate");
    assert_eq!(orchestration.whole_run_state(), WholeRunState::Complete);
}

#[test]
fn first_fails_only_when_every_member_failed() {
    let mut orchestration = engine();
    orchestration
        .record_step_join(STEP, read(Some(BarrierJoin::First)))
        .unwrap();
    let (implementation, review) = start_step(&mut orchestration);
    orchestration
        .fail(&implementation, "compile error")
        .unwrap();
    assert_eq!(
        orchestration.barrier_reading(STEP).unwrap().state,
        BarrierState::Pending
    );
    orchestration.fail(&review, "no finding").unwrap();
    assert_eq!(
        orchestration.barrier_reading(STEP).unwrap().state,
        BarrierState::Failed
    );
}

#[test]
fn all_waits_for_every_member_recorded_or_not() {
    for join in [Some(BarrierJoin::All), None] {
        let mut orchestration = engine();
        orchestration.record_step_join(STEP, read(join)).unwrap();
        let (implementation, review) = start_step(&mut orchestration);
        returned(&mut orchestration, &implementation, "execution:implement");
        let reading = orchestration.barrier_reading(STEP).unwrap();
        assert_eq!(reading.state, BarrierState::Pending, "{join:?}");
        assert_eq!(reading.join, BarrierJoin::All);
        returned(&mut orchestration, &review, "execution:review");
        let reading = orchestration.barrier_reading(STEP).unwrap();
        assert_eq!(reading.state, BarrierState::Complete);
        assert_eq!(reading.joined_units, reading.required_units);
        assert_eq!(reading.closed_by, None);
    }
    // A team step whose join was never read at the start keeps the barrier's own rule: every member.
    let mut unread = engine();
    let (implementation, _) = start_step(&mut unread);
    returned(&mut unread, &implementation, "execution:implement");
    let reading = unread.barrier_reading(STEP).unwrap();
    assert_eq!(reading.state, BarrierState::Pending);
    assert_eq!(reading.agent_set_ref.as_deref(), Some(TEAM));
    assert_eq!(reading.agent_set_revision, None);
}

#[test]
fn quorum_weighted_and_a_shared_none_are_refused_at_the_start_with_the_reason() {
    for (join, says) in [
        (BarrierJoin::Quorum, "count"),
        (BarrierJoin::Weighted, "weight"),
        (BarrierJoin::None, "one return"),
    ] {
        let mut orchestration = engine();
        match orchestration.record_step_join(STEP, read(Some(join))) {
            Err(OrchestrationError::JoinNotHonoured {
                step,
                join: refused,
                reason,
            }) => {
                assert_eq!(step, STEP);
                assert_eq!(refused, join);
                assert!(reason.contains(says), "{reason}");
            }
            other => panic!("{join:?} must be refused, got {other:?}"),
        }
        assert_eq!(
            orchestration
                .barrier_reading(STEP)
                .unwrap()
                .agent_set_revision,
            None
        );
    }
}

#[test]
fn the_join_is_kept_from_the_runs_start() {
    let mut orchestration = engine();
    orchestration
        .record_step_join(STEP, read(Some(BarrierJoin::First)))
        .unwrap();
    // The same reading again changes nothing; another reading cannot replace it.
    orchestration
        .record_step_join(STEP, read(Some(BarrierJoin::First)))
        .unwrap();
    assert!(matches!(
        orchestration.record_step_join(STEP, read(Some(BarrierJoin::All))),
        Err(OrchestrationError::JoinAlreadyRecorded(_))
    ));
    // Once a member has started, the step's join is no longer read.
    let mut started = engine();
    start_step(&mut started);
    assert!(matches!(
        started.record_step_join(STEP, read(Some(BarrierJoin::First))),
        Err(OrchestrationError::JoinAfterStart(_))
    ));
    assert_eq!(
        started.barrier_reading(STEP).unwrap().join,
        BarrierJoin::All
    );
}

#[test]
fn the_recorded_join_and_its_closer_survive_restore() {
    let mut orchestration = engine();
    orchestration
        .record_step_join(STEP, read(Some(BarrierJoin::First)))
        .unwrap();
    let (implementation, _) = start_step(&mut orchestration);
    returned(&mut orchestration, &implementation, "execution:implement");
    let kept: OrchestrationSnapshot =
        serde_json::from_value(serde_json::to_value(orchestration.snapshot()).unwrap()).unwrap();
    let restored = kept
        .restore(compiled(team_source()), orchestration.run().clone())
        .unwrap();
    let reading = restored.barrier_reading(STEP).unwrap();
    assert_eq!(reading.join, BarrierJoin::First);
    assert_eq!(reading.agent_set_revision.as_deref(), Some("r3"));
    assert_eq!(reading.closed_by.as_ref(), Some(&implementation));
    assert_eq!(reading.state, BarrierState::Complete);
}

/// Central as the store sees it: the set's join, and how many times it was asked.
struct Central {
    join: std::cell::Cell<Option<BarrierJoin>>,
    asked: std::cell::Cell<usize>,
}

impl epilogos_factory::orchestration::StepJoinSource for Central {
    fn read_set_join(&self, agent_set_ref: &str) -> Result<Option<SetJoinReading>, String> {
        self.asked.set(self.asked.get() + 1);
        Ok((agent_set_ref == TEAM).then(|| SetJoinReading {
            agent_set_ref: TEAM.into(),
            revision: "r7".into(),
            join: self.join.get(),
        }))
    }
}

#[test]
fn the_store_reads_the_join_at_the_runs_start_and_keeps_it() {
    use epilogos_factory::attempt_native_store::FileAttemptStore;
    use epilogos_factory::attempt_runtime::FactoryAttemptSeed;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("factory-state.json");
    let central = Central {
        join: std::cell::Cell::new(Some(BarrierJoin::First)),
        asked: std::cell::Cell::new(0),
    };
    let store = FileAttemptStore::initialize_reading_joins(
        &path,
        FactoryAttemptSeed {
            run: run(),
            workflow_source: team_source(),
        },
        Some(&central),
    )
    .unwrap();
    assert_eq!(central.asked.get(), 1);
    // The set changes in Central after the start; the Run keeps what it read.
    central.join.set(Some(BarrierJoin::All));
    let reading = FileAttemptStore::open(&path).unwrap().reading().unwrap();
    assert_eq!(central.asked.get(), 1);
    let step = reading
        .barriers
        .iter()
        .find(|step| step.key == STEP)
        .unwrap();
    assert_eq!(step.join, BarrierJoin::First);
    assert_eq!(step.agent_set_ref.as_deref(), Some(TEAM));
    assert_eq!(step.agent_set_revision.as_deref(), Some("r7"));
    drop(store);

    // A set whose join no barrier can combine by refuses the start, and nothing is written.
    let refused = directory.path().join("refused.json");
    central.join.set(Some(BarrierJoin::Quorum));
    let error = FileAttemptStore::initialize_reading_joins(
        &refused,
        FactoryAttemptSeed {
            run: run(),
            workflow_source: team_source(),
        },
        Some(&central),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Quorum"), "{error}");
    assert!(!refused.exists());
}
