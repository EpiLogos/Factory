use epilogos_factory::build::FactoryBuildState;
use epilogos_factory::core::identity::Revision;
use epilogos_factory::core::run::*;
use std::cell::Cell;
use std::collections::BTreeMap;

fn source(id: &str) -> ThoughtSource {
    ThoughtSource {
        owner: "native-test-owner".into(),
        reference: id.into(),
        revision: "r1".into(),
    }
}
fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAY".parse().unwrap(),
        "Consume actual findings",
        "owner",
    )
    .unwrap()
}
fn retain(run: &mut Run, id: &str) {
    let thought = RunThought {
        id: RunThoughtId::new(id).unwrap(),
        run_ref: run.reference().clone(),
        anchor_ref: format!("now/report/{id}"),
        anchor_revision: Some("r1".into()),
        passage: None,
        producer: ThoughtProducer {
            agent_ref: Some("agent:epii".into()),
            execution_ref: Some(format!("execution:{id}")),
            ..Default::default()
        },
        run_map_subject_refs: vec!["subject:concern".into()],
        related_refs: vec![],
        relation_evidence_refs: vec![],
        lifecycle: RunThoughtLifecycle::Active,
    };
    run.apply_thought_command(
        &run.mutation_authority(),
        RunThoughtCommand {
            command_id: format!("retain-{id}"),
            expected_revision: run.revision(),
            thought,
        },
    )
    .unwrap();
}
fn consumption(run: &Run, ids: &[&str]) -> ThoughtConsumption {
    ThoughtConsumption {
        consumption_id: RunThoughtId::new("integrate-first").unwrap(),
        run_ref: run.reference().clone(),
        consumer_ref: "agent:epii".into(),
        working_field: source("central/now/current"),
        inputs: ids
            .iter()
            .map(|id| ThoughtConsumptionInput {
                thought_id: RunThoughtId::new(*id).unwrap(),
                anchor: source(&format!("now/report/{id}")),
                interpretation: Some(source(&format!("ql/thought-meaning/{id}"))),
            })
            .collect(),
        actual_evidence: vec![source("activity/actual-output")],
        human_response: vec![source("human/response")],
        assessment: source("evaluation/actual-against-intent"),
        uses: vec![ThoughtUse {
            kind: ThoughtUseKind::WikiReading,
            source: source("wiki/changed-reading"),
            receiving: source("knowledge/receiving"),
        }],
        retention_policy: source("central/retention-policy"),
        resulting_lifecycle: RunThoughtLifecycle::Integrated,
    }
}
fn command(run: &Run, ids: &[&str]) -> RunThoughtConsumptionCommand {
    RunThoughtConsumptionCommand {
        command_id: "consume-first".into(),
        expected_revision: run.revision(),
        consumption: consumption(run, ids),
    }
}
// A controlled native owner, not a provider which certifies arbitrary requests.
struct Sources {
    values: BTreeMap<String, ThoughtSource>,
    calls: Cell<usize>,
    denied: Option<String>,
}
impl Sources {
    fn new(c: &ThoughtConsumption) -> Self {
        let mut all = vec![
            c.working_field.clone(),
            c.assessment.clone(),
            c.retention_policy.clone(),
        ];
        for input in &c.inputs {
            all.push(input.anchor.clone());
            all.extend(input.interpretation.clone());
        }
        all.extend(c.actual_evidence.clone());
        all.extend(c.human_response.clone());
        for output in &c.uses {
            all.extend([output.source.clone(), output.receiving.clone()]);
        }
        Self {
            values: all.into_iter().map(|s| (s.reference.clone(), s)).collect(),
            calls: Cell::new(0),
            denied: None,
        }
    }
}
impl ThoughtConsumptionSources for Sources {
    fn observe(
        &self,
        requested: &ThoughtSource,
    ) -> Result<ThoughtSourceObservation, ThoughtFieldError> {
        self.calls.set(self.calls.get() + 1);
        if self.denied.as_deref() == Some(&requested.reference) {
            return Ok(ThoughtSourceObservation::Denied {
                reason: "outside disclosure aperture".into(),
            });
        }
        Ok(match self.values.get(&requested.reference) {
            Some(value) if value == requested => ThoughtSourceObservation::Current {
                source: value.clone(),
                receipt: source(&format!("observation/{}", value.reference)),
            },
            Some(value) => ThoughtSourceObservation::Changed {
                observed: value.clone(),
            },
            None => ThoughtSourceObservation::Unavailable {
                reason: "native object unavailable".into(),
            },
        })
    }
}

#[test]
fn all_twelve_source_qualified_reports_are_consumed_without_twelve_stores() {
    let ids = [
        "t0-question",
        "t0-prime-assumption",
        "t1-trace",
        "t1-prime-lacuna",
        "t2-challenge",
        "t2-prime-affordance",
        "t3-pattern",
        "t3-prime-anomaly",
        "t4-discovery",
        "t4-prime-concealment",
        "t5-insight",
        "t5-prime-integration",
    ];
    let mut run = run();
    for id in ids {
        retain(&mut run, id);
    }
    let topology = run.map().clone();
    let command = command(&run, &ids);
    let provider = Sources::new(&command.consumption);
    run.apply_thought_consumption(&run.mutation_authority(), command.clone(), &provider)
        .unwrap();
    assert_eq!(run.thought_field().active().count(), 0);
    assert_eq!(run.thought_field().thoughts().len(), 12);
    assert_eq!(run.map(), &topology);
    let receipt = run.thought_field().consumptions().next().unwrap();
    assert_eq!(receipt.consumption, command.consumption);
    assert!(receipt.observations.len() >= 12);
    for id in ids {
        assert!(run
            .thought_field()
            .consumed_by(&RunThoughtId::new(id).unwrap())
            .is_some());
    }
    let mut registry = RunRegistry::default();
    registry.insert(run.clone()).unwrap();
    let restored = RunRegistry::from_json(&registry.to_json().unwrap()).unwrap();
    assert_eq!(restored.get(run.reference()).unwrap(), &run);
}

#[test]
fn build_path_advances_once_and_replay_does_not_reobserve_changed_world() {
    let mut run = run();
    retain(&mut run, "finding");
    let command = command(&run, &["finding"]);
    let provider = Sources::new(&command.consumption);
    let mut build =
        FactoryBuildState::new(Project::new(run.project_ref().clone()), run.clone()).unwrap();
    let authority = build.run_mutation_authority(run.reference()).unwrap();
    assert!(matches!(
        build
            .apply_run_thought_consumption(run.reference(), &authority, command.clone(), &provider)
            .unwrap(),
        RunThoughtOutcome::Applied { .. }
    ));
    let revision = build.revision();
    let calls = provider.calls.get();
    assert!(matches!(
        build
            .apply_run_thought_consumption(run.reference(), &authority, command, &provider)
            .unwrap(),
        RunThoughtOutcome::AlreadyApplied { .. }
    ));
    assert_eq!(build.revision(), revision);
    assert_eq!(provider.calls.get(), calls);
}

#[test]
fn denied_changed_and_missing_sources_do_not_retire_any_input() {
    for mode in 0..3 {
        let mut run = run();
        retain(&mut run, "one");
        retain(&mut run, "two");
        let command = command(&run, &["one", "two"]);
        let mut provider = Sources::new(&command.consumption);
        let key = "wiki/changed-reading";
        match mode {
            0 => provider.denied = Some(key.into()),
            1 => provider.values.get_mut(key).unwrap().revision = "r2".into(),
            _ => {
                provider.values.remove(key);
            }
        }
        let before = run.clone();
        assert!(run
            .apply_thought_consumption(&run.mutation_authority(), command, &provider)
            .is_err());
        assert_eq!(run, before);
    }
}

#[test]
fn expected_run_revision_and_current_mutation_authority_are_required() {
    let mut run = run();
    retain(&mut run, "one");
    let command = command(&run, &["one"]);
    let provider = Sources::new(&command.consumption);
    let stale = run.mutation_authority();
    let current = run
        .transfer_write_authority(&stale, run.revision(), "new-owner")
        .unwrap();
    let before = run.clone();
    assert!(run
        .apply_thought_consumption(&stale, command.clone(), &provider)
        .is_err());
    assert!(run
        .apply_thought_consumption(&current, command, &provider)
        .is_err());
    assert_eq!(run, before);
    assert_eq!(provider.calls.get(), 0);
}

#[test]
fn reused_identity_cannot_launder_different_sources_or_output() {
    let mut run = run();
    retain(&mut run, "one");
    let mut command = command(&run, &["one"]);
    let provider = Sources::new(&command.consumption);
    run.apply_thought_consumption(&run.mutation_authority(), command.clone(), &provider)
        .unwrap();
    command.consumption.uses[0].source.reference = "wiki/forged".into();
    let before = run.clone();
    assert!(run
        .apply_thought_consumption(&run.mutation_authority(), command, &provider)
        .is_err());
    assert_eq!(run, before);
}

#[test]
fn exact_anchor_no_duplicate_inputs_no_empty_outputs_no_metadata_only_refresh() {
    let mutations: [fn(&mut ThoughtConsumption); 6] = [
        |c| c.inputs[0].anchor.revision = "old".into(),
        |c| c.inputs.push(c.inputs[0].clone()),
        |c| c.uses.clear(),
        |c| c.actual_evidence.clear(),
        |c| c.uses[0].source = c.inputs[0].anchor.clone(),
        |c| c.resulting_lifecycle = RunThoughtLifecycle::Active,
    ];
    for mutate in mutations {
        let mut run = run();
        retain(&mut run, "one");
        let mut command = command(&run, &["one"]);
        mutate(&mut command.consumption);
        let provider = Sources::new(&command.consumption);
        let before = run.clone();
        assert!(run
            .apply_thought_consumption(&run.mutation_authority(), command, &provider)
            .is_err());
        assert_eq!(run, before);
        assert_eq!(provider.calls.get(), 0);
    }
}

#[test]
fn consumed_lineage_cannot_disappear_or_be_reconsumed_after_roundtrip() {
    let mut run = run();
    retain(&mut run, "one");
    let command = command(&run, &["one"]);
    let provider = Sources::new(&command.consumption);
    run.apply_thought_consumption(&run.mutation_authority(), command.clone(), &provider)
        .unwrap();
    let mut registry = RunRegistry::default();
    registry.insert(run.clone()).unwrap();
    let mut data: serde_json::Value = serde_json::from_str(&registry.to_json().unwrap()).unwrap();
    let source_run = &mut data["runs"][run.reference().to_string()];
    source_run["thoughtField"]["thoughts"]["one"]["lifecycle"] = "active".into();
    assert!(RunRegistry::from_json(&data.to_string()).is_err());
    let mut second = command;
    second.command_id = "consume-again".into();
    second.expected_revision = run.revision();
    second.consumption.consumption_id = RunThoughtId::new("again").unwrap();
    assert!(run
        .apply_thought_consumption(&run.mutation_authority(), second, &provider)
        .is_err());
}

#[test]
fn a_meaningful_continuing_question_is_a_valid_use_without_fabricated_human_feedback() {
    let mut run = run();
    retain(&mut run, "one");
    let mut command = command(&run, &["one"]);
    command.consumption.human_response.clear();
    command.consumption.uses[0].kind = ThoughtUseKind::ContinuingQuestion;
    command.consumption.resulting_lifecycle = RunThoughtLifecycle::Retained;
    let provider = Sources::new(&command.consumption);
    run.apply_thought_consumption(&run.mutation_authority(), command, &provider)
        .unwrap();
    assert_eq!(run.thought_field().active().count(), 0);
    assert_eq!(run.thought_field().consumptions().count(), 1);
}

#[test]
fn foreign_run_and_conflicting_native_observation_fail_atomically() {
    let mut run = run();
    retain(&mut run, "one");
    let mut command = command(&run, &["one"]);
    command.consumption.run_ref = "run:01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap();
    let provider = Sources::new(&command.consumption);
    let before = run.clone();
    assert!(run
        .apply_thought_consumption(&run.mutation_authority(), command, &provider)
        .is_err());
    assert_eq!(run, before);
    let mut command = super_command(&run);
    command.expected_revision = Revision::INITIAL;
    assert!(run
        .apply_thought_consumption(&run.mutation_authority(), command, &provider)
        .is_err());
}
fn super_command(run: &Run) -> RunThoughtConsumptionCommand {
    command(run, &["one"])
}

#[test]
fn native_cognitive_reading_exposes_consumption_without_widening_a_focused_report() {
    use epilogos_factory::build::FactoryBuildSelection;
    use epilogos_factory::build_cognitive::{
        FactoryBuildCognitiveFocus, FactoryBuildCognitiveViewProvider,
    };
    let mut run = run();
    retain(&mut run, "one");
    retain(&mut run, "two");
    let command = command(&run, &["one", "two"]);
    let sources = Sources::new(&command.consumption);
    run.apply_thought_consumption(&run.mutation_authority(), command, &sources)
        .unwrap();
    let selection = FactoryBuildSelection {
        project_ref: run.project_ref().clone(),
        run_ref: run.reference().clone(),
    };
    let build = FactoryBuildState::new(Project::new(run.project_ref().clone()), run).unwrap();
    let provider = FactoryBuildCognitiveViewProvider;
    let full = provider.snapshot(&build, &selection).unwrap();
    assert_eq!(full.view.consumptions.len(), 1);
    assert_eq!(full.view.consumption_refs.len(), 2);
    let focus = FactoryBuildCognitiveFocus {
        anchor_ref: Some("now/report/one".into()),
        ..Default::default()
    };
    let narrow = provider
        .focused_snapshot(&build, &selection, &focus)
        .unwrap();
    assert_eq!(narrow.view.consumption_refs.len(), 1);
    assert!(narrow.view.consumptions.is_empty());
}
