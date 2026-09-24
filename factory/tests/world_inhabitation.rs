//! World inhabitation contract v1 §3 through the installed command surface:
//! Position custody, authoritative current work, and the joined inhabitation
//! reading over a real developmental state document.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::developmental_read::FactoryDevelopmentalFileProvider;
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::RetryGrant;
use epilogos_factory::project_development_store::transact_developmental_state;
use epilogos_factory::sensing::{self, Observation, Signal, WorkRelation};
use epilogos_factory::work_custody::{self, AssignRequest, CustodyState, UpdateRequest};
use epilogos_factory::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const P: &str = "central:position:project:O-I:factory-guardian";
const OTHER: &str = "central:position:project:O-I:reviewer";

fn factory(args: &[&str], cwd: Option<&Path>, body: Option<Value>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().unwrap();
    if let Some(body) = body {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.to_string().as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A lawful refusal: exit 2, a three-part JSON document on stdout.
fn refused(output: Output, code: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let refusal: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refusal["schema"], "factory.refusal/v1");
    assert_eq!(refusal["code"], code, "{refusal:#}");
    for part in ["fact", "consequence", "action"] {
        assert!(
            !refusal[part].as_str().unwrap().is_empty(),
            "{part} is empty"
        );
    }
    refusal
}

fn current_work(state: &str, position: &str) -> Value {
    ok(factory(
        &[
            "development",
            "current-work",
            state,
            "--position",
            position,
            "--json",
        ],
        None,
        None,
    ))
}

fn assign(state: &str, args: &[&str]) -> Value {
    let mut all = vec!["development", "custody", "assign", state];
    all.extend_from_slice(args);
    all.push("--json");
    ok(factory(&all, None, None))
}

fn conformance_state(dir: &Path) -> String {
    let path = dir.join("state.json").display().to_string();
    ok(factory(
        &["conformance", "developmental-state", &path, "--json"],
        None,
        None,
    ));
    path
}

struct World {
    dir: tempfile::TempDir,
    workflow: CompiledWorkflow,
    disposition: SituatedExecutionDisposition,
}

impl World {
    /// A native attempt field with one running attempt whose participant
    /// occupies the Position `P`.
    fn new() -> Self {
        Self::new_with_attempt(true)
    }

    fn new_without_attempt() -> Self {
        Self::new_with_attempt(false)
    }

    fn new_with_attempt(start_attempt: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "world inhabitation test",
            "factory-test",
        )
        .unwrap();
        let source: WorkflowSource = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/agent-workflow-source.json"
        ))
        .unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        let unit = workflow.unit("inspect-source").unwrap();
        let selection = accept_aikit_selection(
            ExecutionDemand {
                project_ref: PROJECT.into(),
                run_ref: RUN.into(),
                workflow_unit_ref: Some(unit.reference.to_string()),
                agency_ref: Some("agency:guardian".into()),
                profile_ref: None,
                use_type: "test-only".into(),
                required_capabilities: unit.capability_refs.clone(),
                required_modalities: BTreeSet::from(["text".into()]),
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
                model_ref: "model:test".into(),
                provider_ref: "provider:test".into(),
                ranking_policy: "test".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:test".into()],
            },
            "2026-09-23T20:00:00+01:00",
        )
        .unwrap();
        let disposition = SituatedExecutionDisposition {
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
                agency_ref: "agency:guardian".into(),
                world_binding_ref: "binding:test".into(),
                profile_ref: None,
                position_ref: Some(P.into()),
                source_ref: workflow.source.reference.to_string(),
                source_revision: workflow.source.revision.clone(),
                source_digest: format!("blake3:{}", workflow.source.digest),
            },
            context_refs: BTreeSet::from(["context:test".into()]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:test".into(),
                provider_ref: "provider:test".into(),
                route_ref: "route:test".into(),
                harness_ref: "harness:test".into(),
                harness_composition_ref: "composition:test".into(),
                agent_session_ref: "session:guardian".into(),
                session_space_ref: "space:guardian".into(),
                material_world_ref: None,
                workcell_ref: None,
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
                wall_clock_timeout_ms: Some(10_000),
                retry_grant_ref: Some("grant:guardian".into()),
                maximum_attempts: Some(2),
            },
        };
        let world = Self {
            dir,
            workflow,
            disposition,
        };
        ok(factory(
            &["attempt", "init", &world.state(), "-", "--json"],
            None,
            Some(
                serde_json::to_value(FactoryAttemptSeed {
                    run,
                    workflow_source: source,
                })
                .unwrap(),
            ),
        ));
        if start_attempt {
            world.act(FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:first".into(),
                task_ref: "task:guardian".into(),
                parent_journey_ref: "journey:test".into(),
                workflow_unit_ref: world.unit(),
                disposition: world.disposition.clone(),
                retry_grant: Some(RetryGrant::new("grant:guardian", 2).unwrap()),
                tracking: vec![],
                place_grant: None,
            });
        }
        world
    }

    fn unit(&self) -> epilogos_factory::core::run::WorkflowUnitRef {
        self.workflow
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone()
    }

    fn state(&self) -> String {
        self.dir.path().join("state.json").display().to_string()
    }

    fn act(&self, operation: FactoryAttemptOperation) {
        let reading = ok(factory(
            &["attempt", "read", &self.state(), "--json"],
            None,
            None,
        ));
        let revision = reading["revision"].as_u64().unwrap();
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:test:{revision}"),
            caller: FactoryActionCaller {
                caller_ref: "agent:test".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["agent:test".into()],
            },
            run_ref: RUN.parse().unwrap(),
            expected_revision: revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:test".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        };
        ok(factory(
            &["attempt", "action", &self.state(), "-", "--json"],
            None,
            Some(serde_json::to_value(request).unwrap()),
        ));
    }
}

#[test]
fn a_running_occupant_and_custody_join_on_one_node_and_distinct_work_is_ambiguous() {
    let world = World::new();
    let state = world.state();
    let unit = world.unit().to_string();
    let node = format!("{RUN}/{unit}");

    let reading = current_work(&state, P);
    assert_eq!(reading["schema"], "factory.current-work/v1");
    assert_eq!(reading["outcome"], "one", "{reading:#}");
    assert_eq!(reading["current"]["node_ref"], node);
    assert_eq!(reading["current"]["kind"], "workflow-unit");
    assert_eq!(reading["current"]["attempt_refs"], json!(["attempt:first"]));
    // #263: the attempt's Run is disclosed on the node itself, not only in candidates.
    assert_eq!(reading["current"]["run_refs"], json!([RUN]));
    assert_eq!(reading["candidates"][0]["status"], "active");

    // Custody naming the same WorkflowUnit is the same node, not a second one.
    let custody = assign(
        &state,
        &[
            "--position",
            P,
            "--work",
            "github:EpiLogos/Factory#195",
            "--run",
            RUN,
            "--workflow-unit",
            &unit,
            "--reason",
            "the guardian carries the inspection",
        ],
    );
    let unit_custody = custody["custody"]["custody_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "one", "{reading:#}");
    assert_eq!(reading["current"]["custody_refs"], json!([unit_custody]));
    assert_eq!(reading["current"]["attempt_refs"], json!(["attempt:first"]));
    // #263: custody and attempt naming the same Run de-duplicate in run_refs.
    assert_eq!(reading["current"]["run_refs"], json!([RUN]));
    assert_eq!(reading["considered"], 2);

    // Distinct in-progress work makes the answer ambiguous, never a guess.
    let other = assign(
        &state,
        &[
            "--position",
            P,
            "--work",
            "github:EpiLogos/O-I#220",
            "--reason",
            "second obligation",
        ],
    );
    let other_ref = other["custody"]["custody_ref"].as_str().unwrap().to_owned();
    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "ambiguous", "{reading:#}");
    assert!(reading["current"].is_null());
    assert_eq!(reading["candidates"].as_array().unwrap().len(), 3);

    // Blocked custody is not current work.
    ok(factory(
        &[
            "development",
            "custody",
            "update",
            &state,
            "--custody",
            &other_ref,
            "--state",
            "blocked",
            "--reason",
            "waiting on review",
            "--json",
        ],
        None,
        None,
    ));
    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "one", "{reading:#}");
    assert_eq!(reading["considered"], 3);

    // A historical attempt is not current: fail and retry on the same leg.
    world.act(FactoryAttemptOperation::Fail {
        attempt_ref: "attempt:first".into(),
        reason: "controlled failure".into(),
        evidence_refs: BTreeSet::from(["evidence:failure".into()]),
    });
    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "one");
    assert_eq!(reading["current"]["attempt_refs"], json!([]));
    world.act(FactoryAttemptOperation::Retry {
        attempt_ref: "attempt:second".into(),
        task_ref: "task:guardian".into(),
        parent_journey_ref: "journey:test".into(),
        workflow_unit_ref: world.unit(),
        grant_ref: "grant:guardian".into(),
        disposition: world.disposition.clone(),
        tracking: vec![],
        reresolution: None,
        place_grant: None,
    });
    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "one");
    assert_eq!(
        reading["current"]["attempt_refs"],
        json!(["attempt:second"])
    );
    assert_eq!(reading["considered"], 4);

    // Another Position holds nothing here.
    assert_eq!(current_work(&state, OTHER)["outcome"], "none");
}

#[test]
fn the_inhabitation_reading_carries_occupant_relations_verbatim_and_marks_absence() {
    let world = World::new();
    let state = world.state();
    let unit = world.unit().to_string();
    assign(
        &state,
        &[
            "--position",
            P,
            "--work",
            "work:inspection",
            "--run",
            RUN,
            "--workflow-unit",
            &unit,
            "--reason",
            "custody of the unit",
        ],
    );
    assign(
        &state,
        &[
            "--position",
            OTHER,
            "--work",
            "work:unplaced",
            "--reason",
            "not tied to a Run",
        ],
    );
    let reading = ok(factory(
        &["development", "inhabitation", &state, "--json"],
        None,
        None,
    ));
    assert_eq!(reading["schema"], "factory.inhabitation-reading/v1");
    assert_eq!(reading["project_ref"], PROJECT);
    assert_eq!(reading["central_project_ref"]["state"], "absent");
    let run = &reading["runs"][0];
    assert_eq!(run["run_ref"], RUN);
    let occupant = &run["occupants"][0];
    assert_eq!(occupant["attempt_ref"], "attempt:first");
    assert_eq!(occupant["current_attempt"], true);
    assert_eq!(occupant["leg_status"], "active");
    assert_eq!(occupant["participant"]["position_ref"]["state"], "present");
    assert_eq!(occupant["participant"]["position_ref"]["value"], P);
    assert_eq!(
        occupant["participant"]["agency_ref"]["value"],
        "agency:guardian"
    );
    assert_eq!(
        occupant["body"]["agent_session_ref"]["value"],
        "session:guardian"
    );
    assert_eq!(occupant["body"]["workcell_ref"]["state"], "absent");
    assert!(occupant["body"]["workcell_ref"]["reason"].is_string());
    assert_eq!(occupant["placement"]["now_ref"]["state"], "absent");
    assert_eq!(occupant["participant"]["profile_ref"]["state"], "absent");
    assert_eq!(occupant["return_address"]["state"], "present");
    let position = &run["positions"][0];
    assert_eq!(position["position_ref"], P);
    assert_eq!(position["in_custody"], true);
    assert_eq!(position["current_attempt_refs"], json!(["attempt:first"]));
    assert_eq!(run["custody"].as_array().unwrap().len(), 1);
    assert_eq!(reading["custody_outside_runs"][0]["position_ref"], OTHER);

    let filtered = ok(factory(
        &[
            "development",
            "inhabitation",
            &state,
            "--position",
            OTHER,
            "--json",
        ],
        None,
        None,
    ));
    assert_eq!(filtered["runs"], json!([]));
    assert_eq!(
        filtered["custody_outside_runs"][0]["work_ref"],
        "work:unplaced"
    );

    let by_run = ok(factory(
        &[
            "development",
            "inhabitation",
            &state,
            "--run",
            RUN,
            "--json",
        ],
        None,
        None,
    ));
    assert_eq!(by_run["runs"].as_array().unwrap().len(), 1);
    assert_eq!(by_run["custody_outside_runs"], json!([]));

    refused(
        factory(
            &[
                "development",
                "inhabitation",
                &state,
                "--run",
                "run:01ARZ3NDEKTSV4RRFFQ69G5FZZ",
                "--json",
            ],
            None,
            None,
        ),
        "factory.inhabitation.unknown_run",
    );
}

fn insert_native_sensing_work(
    state_path: &str,
    source_ref: &str,
    custody_ref: &str,
    now_ref: &str,
) -> String {
    let world = PROJECT;
    let signal_ref = sensing::signal_ref(world, source_ref);
    transact_developmental_state(Path::new(state_path), |state| {
        state.sensing.project_world_ref = Some(world.into());
        state.sensing.signals.insert(
            signal_ref.clone(),
            Signal {
                signal_ref: signal_ref.clone(),
                project_world_ref: world.into(),
                observation: Observation {
                    source_ref: source_ref.into(),
                    provider_ref: "github".into(),
                    source_revision: "source-revision:inhabitation-test".into(),
                    occurred_at_unix_ms: Some(1_790_270_000_000),
                    observed_at_unix_ms: 1_790_270_000_000,
                    summary: "Native source-qualified work already has a child NOW".into(),
                    dimension: "product".into(),
                    standing: "provider-reported".into(),
                    relation_refs: vec![],
                },
                prior_source_revisions: vec![],
                prior_observations: vec![],
                first_observed_at_unix_ms: 1_790_270_000_000,
                decisions: vec![],
                work: Some(WorkRelation {
                    custody_ref: custody_ref.into(),
                    work_ref: signal_ref.clone(),
                    run_ref: Some(RUN.into()),
                    position_ref: P.into(),
                    now_ref: Some(now_ref.into()),
                    authority_ref: "authority:test".into(),
                    created_at_unix_ms: 1_790_270_000_000,
                }),
                returns: vec![],
            },
        );
        Ok(())
    })
    .unwrap();
    signal_ref
}

#[test]
fn source_qualified_work_child_now_is_visible_before_attempt_and_refuses_conflicts() {
    let world = World::new_without_attempt();
    let state = world.state();
    let source = "https://github.com/EpiLogos/Factory/issues/199";
    let signal_ref = sensing::signal_ref(PROJECT, source);
    let custody = assign(
        &state,
        &[
            "--position",
            P,
            "--work",
            &signal_ref,
            "--run",
            RUN,
            "--reason",
            "source-qualified defect work before the first Attempt",
        ],
    );
    let custody_ref = custody["custody"]["custody_ref"].as_str().unwrap();
    let child = "central:now:project:01ARZ3NDEKTSV4RRFFQ69G5FAW:original-child";
    assert_eq!(
        insert_native_sensing_work(&state, source, custody_ref, child),
        signal_ref
    );
    let read = || {
        ok(factory(
            &[
                "development",
                "inhabitation",
                &state,
                "--run",
                RUN,
                "--position",
                P,
                "--json",
            ],
            None,
            None,
        ))
    };
    let reading = read();
    assert_eq!(reading["runs"][0]["occupants"], json!([]));
    assert_eq!(
        reading["runs"][0]["positions"][0]["child_now_ref"]["state"],
        "present"
    );
    assert_eq!(
        reading["runs"][0]["positions"][0]["child_now_ref"]["value"],
        child
    );
    assert_eq!(
        reading["runs"][0]["positions"][0]["custody"][0]["child_now_ref"]["value"],
        child
    );
    assert!(
        reading["runs"][0]["positions"][0]["child_now_ref"]["source"]
            .as_str()
            .unwrap()
            .contains(&signal_ref)
    );

    // The native developmental store refuses broken custody and Run links
    // before an inhabitation reading can promote them.
    for (bad_custody, bad_run) in [(true, false), (false, true)] {
        let result = transact_developmental_state(Path::new(&state), |native| {
            let work = native
                .sensing
                .signals
                .get_mut(&signal_ref)
                .unwrap()
                .work
                .as_mut()
                .unwrap();
            if bad_custody {
                work.custody_ref = "factory:custody:foreign".into();
            }
            if bad_run {
                work.run_ref = Some("run:other".into());
            }
            Ok(())
        });
        assert!(
            result.is_err(),
            "broken native work relation must be refused"
        );
        assert_eq!(
            read()["runs"][0]["positions"][0]["child_now_ref"]["value"],
            child
        );
    }
    // A foreign Project NOW ref is syntactically retained but cannot be
    // presented as this Project's child.
    transact_developmental_state(Path::new(&state), |native| {
        native
            .sensing
            .signals
            .get_mut(&signal_ref)
            .unwrap()
            .work
            .as_mut()
            .unwrap()
            .now_ref = Some("central:now:project:Other:foreign-child".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        read()["runs"][0]["positions"][0]["child_now_ref"]["state"],
        "unavailable"
    );
    transact_developmental_state(Path::new(&state), |native| {
        native
            .sensing
            .signals
            .get_mut(&signal_ref)
            .unwrap()
            .work
            .as_mut()
            .unwrap()
            .now_ref = Some(child.into());
        Ok(())
    })
    .unwrap();

    let other_source = "https://github.com/EpiLogos/Factory/issues/200";
    let other_signal = sensing::signal_ref(PROJECT, other_source);
    let other_custody = assign(
        &state,
        &[
            "--position",
            P,
            "--work",
            &other_signal,
            "--run",
            RUN,
            "--reason",
            "another independently held source-qualified work item",
        ],
    );
    let other_custody_ref = other_custody["custody"]["custody_ref"].as_str().unwrap();
    insert_native_sensing_work(
        &state,
        other_source,
        other_custody_ref,
        "central:now:project:01ARZ3NDEKTSV4RRFFQ69G5FAW:other-child",
    );
    assert_eq!(
        read()["runs"][0]["positions"][0]["child_now_ref"]["state"],
        "ambiguous"
    );
    let each = read()["runs"][0]["positions"][0]["custody"]
        .as_array()
        .unwrap()
        .iter()
        .map(|custody| custody["child_now_ref"]["value"].as_str().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(each.len(), 2, "each exact custody retains its own child NOW");
}

#[test]
fn refusals_are_three_part_and_leave_the_state_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let state = conformance_state(dir.path());
    let before = std::fs::read(&state).unwrap();

    refused(
        factory(
            &[
                "development",
                "custody",
                "assign",
                &state,
                "--position",
                P,
                "--work",
                "work:a",
                "--json",
            ],
            None,
            None,
        ),
        "factory.custody.usage",
    );
    refused(
        factory(
            &[
                "development",
                "custody",
                "assign",
                &state,
                "--position",
                P,
                "--work",
                "work:a",
                "--run",
                "run:01ARZ3NDEKTSV4RRFFQ69G5FZZ",
                "--reason",
                "r",
                "--json",
            ],
            None,
            None,
        ),
        "factory.custody.unknown_run",
    );
    refused(
        factory(
            &[
                "development",
                "custody",
                "update",
                &state,
                "--custody",
                "factory:custody:0190f5c2-0000-7000-8000-000000000001",
                "--state",
                "completed",
                "--reason",
                "r",
                "--json",
            ],
            None,
            None,
        ),
        "factory.custody.not_found",
    );
    refused(
        factory(
            &[
                "development",
                "current-work",
                &state,
                "--position",
                "agent/guardian",
                "--json",
            ],
            None,
            None,
        ),
        "factory.custody.invalid_position",
    );
    assert_eq!(before, std::fs::read(&state).unwrap());

    // Human form: the same three parts on stderr, nothing on stdout.
    let human = factory(
        &[
            "development",
            "custody",
            "assign",
            &state,
            "--position",
            P,
            "--work",
            "work:a",
        ],
        None,
        None,
    );
    assert_eq!(human.status.code(), Some(2));
    assert!(human.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(stderr.contains("--reason is required"), "{stderr}");
    assert!(stderr.contains("consequence:") && stderr.contains("action:"));

    // Close, then refuse an implicit reopen, then reopen deliberately.
    let custody = assign(
        &state,
        &["--position", P, "--work", "work:a", "--reason", "assigned"],
    );
    let reference = custody["custody"]["custody_ref"].as_str().unwrap();
    let update = |args: &[&str]| {
        let mut all = vec![
            "development",
            "custody",
            "update",
            state.as_str(),
            "--custody",
            reference,
        ];
        all.extend_from_slice(args);
        all.push("--json");
        factory(&all, None, None)
    };
    ok(update(&["--state", "completed", "--reason", "done"]));
    let refusal = refused(
        update(&["--state", "in-progress", "--reason", "again"]),
        "factory.custody.reopen_required",
    );
    assert!(refusal["action"].as_str().unwrap().contains("--reopen"));
    let reopened = ok(update(&[
        "--state",
        "in-progress",
        "--reopen",
        "--reason",
        "again",
        "--expected-revision",
        "2",
    ]));
    assert_eq!(reopened["custody"]["revision"], 3);
    refused(
        update(&[
            "--state",
            "blocked",
            "--reason",
            "stale",
            "--expected-revision",
            "2",
        ]),
        "factory.custody.revision_conflict",
    );
    refused(
        update(&["--state", "handed-off", "--reason", "nobody"]),
        "factory.custody.handoff_target_required",
    );
    let handed = ok(update(&[
        "--state",
        "handed-off",
        "--to-position",
        OTHER,
        "--reason",
        "reviewer takes it",
    ]));
    assert_eq!(handed["successor"]["position_ref"], OTHER);
    let listing = ok(factory(
        &[
            "development",
            "custody",
            "list",
            &state,
            "--position",
            OTHER,
            "--json",
        ],
        None,
        None,
    ));
    assert_eq!(listing["count"], 1);
    assert_eq!(listing["custody"][0]["handed_off_from"], reference);
    assert_eq!(current_work(&state, OTHER)["outcome"], "one");
    assert_eq!(current_work(&state, P)["outcome"], "none");
}

/// The cap proof against a real state file: 150 closed or blocked relations
/// plus two in-progress ones far apart in insertion order.
#[test]
fn current_work_reads_every_relation_not_a_capped_page() {
    let dir = tempfile::tempdir().unwrap();
    let state = conformance_state(dir.path());
    let path = PathBuf::from(&state);
    let mut in_progress = Vec::new();
    for index in 0..152 {
        let parked = index % 2 == 0 && !matches!(index, 3 | 140);
        let receipt = work_custody::assign(
            &path,
            AssignRequest {
                position_ref: P.into(),
                work_ref: format!("work:{index:03}"),
                reason: "load".into(),
                initial_state: parked.then_some(CustodyState::Blocked),
                ..AssignRequest::default()
            },
        )
        .unwrap();
        let reference = receipt.custody.custody_ref;
        if matches!(index, 3 | 140) {
            in_progress.push(reference);
        } else if !parked {
            work_custody::update(
                &path,
                UpdateRequest {
                    custody_ref: reference,
                    state: Some(CustodyState::Completed),
                    reason: "done".into(),
                    ..UpdateRequest::default()
                },
            )
            .unwrap();
        }
    }
    let listing = ok(factory(
        &[
            "development",
            "custody",
            "list",
            &state,
            "--position",
            P,
            "--json",
        ],
        None,
        None,
    ));
    assert_eq!(listing["count"], 152);

    let reading = current_work(&state, P);
    assert_eq!(reading["outcome"], "ambiguous", "{reading:#}");
    assert_eq!(reading["considered"], 152);
    let mut listed = reading["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["source_ref"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    listed.sort();
    in_progress.sort();
    assert_eq!(listed, in_progress);
}

#[test]
fn states_without_custody_or_positions_round_trip_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let state = conformance_state(dir.path());
    let before = std::fs::read(&state).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains("workCustody"));

    for args in [
        vec!["development", "custody", "list", state.as_str(), "--json"],
        vec![
            "development",
            "current-work",
            state.as_str(),
            "--position",
            P,
            "--json",
        ],
        vec!["development", "inhabitation", state.as_str(), "--json"],
    ] {
        ok(factory(&args, None, None));
    }
    transact_developmental_state(Path::new(&state), |_| Ok(())).unwrap();
    assert_eq!(before, std::fs::read(&state).unwrap());

    // Re-publishing the loaded document produces the same bytes.
    let loaded = FactoryDevelopmentalFileProvider::open(&state).unwrap();
    assert!(loaded.state().work_custody.is_empty());
    let copy = dir.path().join("copy.json");
    FactoryDevelopmentalFileProvider::create(&copy, loaded.state().clone()).unwrap();
    assert_eq!(before, std::fs::read(&copy).unwrap());

    // The checked-in O:I self-hosting state predates custody and still loads.
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../contracts/factory/fixtures/oi-self-hosting-state.json"
    ))
    .unwrap();
    let fixture_path = dir.path().join("fixture.json");
    std::fs::write(&fixture_path, &fixture).unwrap();
    let provider = FactoryDevelopmentalFileProvider::open(&fixture_path).unwrap();
    assert!(provider.state().work_custody.is_empty());
    assert!(!serde_json::to_string(provider.state())
        .unwrap()
        .contains("workCustody"));

    // A participant without a Position keeps its exact encoding.
    let participant = json!({
        "agentRef": "agent:a", "agencyRef": "agency:a", "worldBindingRef": "binding:a",
        "sourceRef": "source:a", "sourceRevision": "r1", "sourceDigest": "blake3:00"
    });
    let decoded: SituatedParticipant = serde_json::from_value(participant.clone()).unwrap();
    assert_eq!(decoded.position_ref, None);
    assert_eq!(serde_json::to_value(&decoded).unwrap(), participant);
    let mut positioned = participant;
    positioned["positionRef"] = json!(P);
    let decoded: SituatedParticipant = serde_json::from_value(positioned.clone()).unwrap();
    assert_eq!(decoded.position_ref.as_deref(), Some(P));
    assert_eq!(serde_json::to_value(&decoded).unwrap(), positioned);

    // With custody present the document still round-trips exactly.
    assign(
        &state,
        &["--position", P, "--work", "work:a", "--reason", "assigned"],
    );
    let with_custody = std::fs::read(&state).unwrap();
    let reloaded = FactoryDevelopmentalFileProvider::open(&state).unwrap();
    let again = dir.path().join("again.json");
    FactoryDevelopmentalFileProvider::create(&again, reloaded.state().clone()).unwrap();
    assert_eq!(with_custody, std::fs::read(&again).unwrap());
}

#[test]
fn the_state_is_located_from_the_project_placement_when_omitted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    let located = ok(factory(
        &[
            "project",
            "setup",
            root.to_str().unwrap(),
            "inhabitation-test",
            "--json",
        ],
        None,
        None,
    ));
    let inside = root.join("src/deep");
    let assigned = ok(factory(
        &[
            "development",
            "custody",
            "assign",
            "--position",
            P,
            "--work",
            "work:located",
            "--reason",
            "from the placement",
            "--json",
        ],
        Some(&inside),
        None,
    ));
    assert_eq!(assigned["result"], "applied");
    let reading = ok(factory(
        &["development", "current-work", "--position", P, "--json"],
        Some(&inside),
        None,
    ));
    assert_eq!(reading["outcome"], "one");
    assert_eq!(reading["project_ref"], located["projectRef"]);

    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    refused(
        factory(
            &["development", "current-work", "--position", P, "--json"],
            Some(&outside),
            None,
        ),
        "factory.state.not_located",
    );
}
