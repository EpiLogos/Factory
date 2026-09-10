"""Bounded native Action repair; removed after this PR's product edits land."""
from pathlib import Path
root = Path(__file__).resolve().parents[1]
paths = ['factory/src/attempt_native_store.rs', 'factory/src/attempt_runtime.rs']
texts = {path: (root / path).read_text() for path in paths}
s = texts[paths[0]]
if 'view.action_receipts.get(&request.projection_ref)' in s:
    s = s.replace('view.action_receipts.get(&request.projection_ref)', 'view.action_receipts.get(&request_digest)')
    old = 'view.action_receipts.insert(\n                    request.projection_ref.clone(),'
    if old not in s:
        old = 'view.action_receipts.insert(\n                        request.projection_ref.clone(),'
    assert old in s, 'Action journal insertion changed'
    s = s.replace(old, old.replace('request.projection_ref.clone()', 'request_digest.clone()'), 1)
    texts[paths[0]] = s
    s = texts[paths[1]]
    old = 'reference != &applied.request.projection_ref\n            || reference != &applied.receipt.projection_ref'
    assert old in s, 'Action journal validation changed'
    s = s.replace(old, 'reference != &applied.request_digest\n            || applied.request.projection_ref != applied.receipt.projection_ref', 1)
    texts[paths[1]] = s
for path, content in texts.items():
    (root / path).write_text(content)

path = root / 'factory/tests/attempt_public_regressions.rs'
s = path.read_text()
if 'native_projection_reuse_and_exact_action_replay_are_distinct' not in s:
    s += r'''

#[test]
fn native_projection_reuse_and_exact_action_replay_are_distinct() {
    let world = World::new();
    let first = world.request(world.start_operation("projection-reuse", "inspect-source"));
    success(world.invoke(&first));
    let committed = fs::read(&world.state).unwrap();
    success(world.invoke(&first));
    assert_eq!(committed, fs::read(&world.state).unwrap());
    let mut second = world.request(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "projection-reuse".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:second-action-same-projection".into(), kind: "source-change".into(),
            owner_ref: "central".into(), subject_ref: "source:controlled-test".into(),
            source_revision: "source:revision-two".into(), evidence_refs: set(["evidence:source-two"]),
        },
    });
    second.projection_ref = first.projection_ref.clone();
    success(world.invoke(&second));
    let committed = fs::read(&world.state).unwrap();
    success(world.invoke(&second));
    assert_eq!(committed, fs::read(&world.state).unwrap());
    assert_eq!(world.reading().attempts[0].tracking.len(), 1);
}

#[test]
fn native_project_and_attempt_reads_share_one_canonical_run() {
    let world = World::new();
    world.start("canonical");
    let attempts = world.reading();
    let native = epilogos_factory::project_development_store::read_developmental_state(&world.state).unwrap();
    let run = native.build.run(&RUN.parse().unwrap()).unwrap();
    assert_eq!(run.revision().get(), attempts.run_revision);
    assert_eq!(run.map().topology_revision().get(), attempts.topology_revision);
    assert_eq!(native.build.revision().get(), attempts.revision);
    let result = success(command(&["development", "run", world.state.to_str().unwrap(), RUN, "--json"], None));
    assert!(String::from_utf8(result.stdout).unwrap().contains(RUN));
    let bytes: Value = serde_json::from_slice(&fs::read(&world.state).unwrap()).unwrap();
    assert!(bytes["state"]["attemptStates"][RUN].get("run").is_none());
    assert_eq!(bytes["state"]["build"]["runs"]["runs"].as_object().unwrap().len(), 1);
}

#[test]
fn changed_current_source_stays_readable_but_cannot_start_effects_from_old_basis() {
    let world = World::new();
    world.start("history");
    let before = world.reading();
    epilogos_factory::project_development_store::transact_developmental_state(&world.state, |native| {
        native.workflow_sources[0].source.revision = "source-revision-new-current".into();
        native.workflow_sources[0].source.digest = workflow_source_digest(&native.workflow_sources[0]).unwrap();
        Ok(())
    }).unwrap();
    let after = world.reading();
    assert!(!after.source_current);
    assert_eq!(before.workflow_source_revision, after.workflow_source_revision);
    assert_eq!(before.attempts, after.attempts);
    let blocked = world.refuses(FactoryAttemptOperation::BindDispatch {
        attempt_ref: "history".into(), execution_ref: "execution:new".into(),
        receipt: owner_receipt("history", "cannot-dispatch-stale-source", OwnerOperationPhase::Submitted),
    });
    assert!(blocked.contains("explicit re-resolution"));
    world.apply(FactoryAttemptOperation::RecordObservation {
        attempt_ref: "history".into(), receipt: owner_receipt("history", "historical-observation", OwnerOperationPhase::Observed),
    });
    assert!(!world.reading().source_current);
}

#[test]
fn attaching_an_existing_native_run_does_not_create_a_parallel_run_store() {
    use epilogos_factory::build::FactoryBuildState;
    use epilogos_factory::core::run::Project;
    use epilogos_factory::developmental_read::{FactoryDevelopmentalFileProvider, FactoryDevelopmentalState};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.json");
    let source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
    let source_ref = source.source.reference.to_string();
    let run = Run::new(RUN.parse().unwrap(), PROJECT.parse().unwrap(), "existing canonical Run", "retained-writer").unwrap();
    let original_authority = run.write_authority().clone();
    let state = FactoryDevelopmentalState::new(
        FactoryBuildState::new(Project::new(PROJECT.parse().unwrap()), run).unwrap(), vec![],
    ).unwrap().with_workflow_sources(vec![source]).unwrap();
    FactoryDevelopmentalFileProvider::create(&path, state).unwrap();
    let args = ["attempt", "attach", path.to_str().unwrap(), RUN, &source_ref, "--json"];
    success(command(&args, None));
    let committed = fs::read(&path).unwrap();
    success(command(&args, None));
    assert_eq!(committed, fs::read(&path).unwrap());
    let native = epilogos_factory::project_development_store::read_developmental_state(&path).unwrap();
    let run = native.build.run(&RUN.parse().unwrap()).unwrap();
    assert_eq!(run.write_authority(), &original_authority);
    assert_eq!(run.destination(), "existing canonical Run");
    assert_eq!(native.attempt_states.len(), 1);
}
'''
    path.write_text(s)
print('Native Action replay, canonical provider and source-current regressions retained.')
