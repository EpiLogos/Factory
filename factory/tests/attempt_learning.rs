use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::project_development::{
    DevelopmentObservation, DevelopmentObservationKind, ProjectDevelopmentLedger,
};
use epilogos_factory::project_development_store::{
    FileProjectDevelopmentStore, ProjectDevelopmentStore,
};
use epilogos_factory::workflow::{compile_workflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Child, Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
fn spawn(args: &[String], body: Option<Value>) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(body) = body {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.to_string().as_bytes())
            .unwrap();
    }
    child
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
struct World {
    dir: tempfile::TempDir,
}
impl World {
    fn state(&self) -> String {
        self.dir.path().join("state.json").display().to_string()
    }
    fn ledger(&self) -> String {
        self.dir.path().join("ledger").display().to_string()
    }
    fn call(&self, operation: &str, body: Option<Value>) -> Output {
        let mut args = vec!["attempt".into(), operation.into(), self.state()];
        if body.is_some() {
            args.push("-".into());
        }
        args.push("--json".into());
        spawn(&args, body).wait_with_output().unwrap()
    }
    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_value(success(self.call("read", None))).unwrap()
    }
    fn action(&self, operation: FactoryAttemptOperation) {
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:{}", self.reading().revision),
            caller: caller(),
            run_ref: RUN.parse().unwrap(),
            expected_revision: self.reading().revision,
            authority: authority(),
            operation,
        };
        success(self.call("action", Some(serde_json::to_value(request).unwrap())));
    }
    fn new() -> Self {
        let world = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        let run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "learning controlled world",
            "factory-test",
        )
        .unwrap();
        let source: WorkflowSource = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/agent-workflow-source.json"
        ))
        .unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        let unit = workflow.unit("inspect-source").unwrap();
        success(
            world.call(
                "init",
                Some(
                    serde_json::to_value(FactoryAttemptSeed {
                        run,
                        workflow_source: source,
                    })
                    .unwrap(),
                ),
            ),
        );
        let selection = accept_aikit_selection(
            ExecutionDemand {
                project_ref: PROJECT.into(),
                run_ref: RUN.into(),
                workflow_unit_ref: Some(unit.reference.to_string()),
                agency_ref: Some("agency:test".into()),
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
            "2026-09-10T20:00:00+01:00",
        )
        .unwrap();
        let disposition = SituatedExecutionDisposition {
            selection,
            participant: SituatedParticipant {
                agent_ref: unit
                    .agent_requirements
                    .agent_refs
                    .iter()
                    .next()
                    .unwrap()
                    .clone(),
                agency_ref: "agency:test".into(),
                world_binding_ref: "binding:test".into(),
                profile_ref: None,
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
                agent_session_ref: "session:test".into(),
                session_space_ref: "space:test".into(),
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
                retry_grant_ref: None,
                maximum_attempts: None,
            },
        };
        world.action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:learning".into(),
            task_ref: "task:learning".into(),
            parent_journey_ref: "journey:test".into(),
            workflow_unit_ref: unit.reference.clone(),
            disposition,
            retry_grant: None,
            tracking: vec![],
        });
        world
    }
    fn request(&self) -> Value {
        json!({"contract":"factory.attempt-learning-action/v1","requestRef":"learning-request:test","projectionRef":"projection:test","caller":caller(),"runRef":RUN,"expectedRevision":self.reading().revision,"authority":authority(),"attemptRef":"attempt:learning","ledgerRoot":self.ledger(),"observationRef":"observation:attempt-learning","kind":"insufficient-evidence","statement":"The controlled attempt did not produce enough evidence yet.","evidenceRefs":[],"ownerReturn":null,"recover":false})
    }
    fn observations(&self) -> Value {
        success(
            spawn(
                &[
                    "development".into(),
                    "observations".into(),
                    self.ledger(),
                    RUN.into(),
                    "--json".into(),
                ],
                None,
            )
            .wait_with_output()
            .unwrap(),
        )
    }
}
fn caller() -> FactoryActionCaller {
    FactoryActionCaller {
        caller_ref: "agent:test".into(),
        projection_kind: FactoryActionProjectionKind::Headless,
        lineage: vec!["agent:test".into()],
    }
}
fn authority() -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority {
        authority_ref: "authority:test".into(),
        native_owner: "factory".into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    }
}

#[test]
fn learning_is_visible_through_existing_intake_and_attempt_readback_without_promoting_it() {
    let world = World::new();
    let receipt = success(world.call("learn", Some(world.request())));
    assert_eq!(receipt["recordedInExistingLedger"], true);
    assert_eq!(receipt["needsReconciliation"], false);
    assert_eq!(receipt["automaticPromotionPerformed"], false);
    let observations = world.observations();
    assert_eq!(observations["observationCount"], 1);
    assert_eq!(
        observations["observations"][0]["kind"],
        "insufficient-evidence"
    );
    let subjects = observations["observations"][0]["subject_refs"]
        .as_array()
        .unwrap();
    for reference in [
        RUN,
        "task:learning",
        "attempt:learning",
        "context:test",
        "model:test",
    ] {
        assert!(subjects.iter().any(|value| value == reference));
    }
    assert!(world.reading().attempts[0]
        .tracking
        .iter()
        .any(|fact| fact.kind == "regression-observation"
            && fact.subject_ref == "observation:attempt-learning"));
}
#[test]
fn learning_replay_is_byte_stable_and_different_content_cannot_reuse_the_native_observation() {
    let world = World::new();
    let request = world.request();
    success(world.call("learn", Some(request.clone())));
    let bytes = std::fs::read(world.state()).unwrap();
    assert_eq!(
        success(world.call("learn", Some(request.clone())))["replayed"],
        true
    );
    assert_eq!(bytes, std::fs::read(world.state()).unwrap());
    assert_eq!(world.observations()["observationCount"], 1);
    let mut changed = request;
    changed["statement"] = json!("different conclusion");
    assert!(!world.call("learn", Some(changed)).status.success());
    assert_eq!(world.observations()["observationCount"], 1);
}
#[test]
fn learning_requires_retained_evidence_and_never_removes_owner_recognition() {
    let world = World::new();
    let before = std::fs::read(world.state()).unwrap();
    let mut request = world.request();
    request["evidenceRefs"] = json!(["invented:evidence"]);
    assert!(!world.call("learn", Some(request)).status.success());
    let mut request = world.request();
    request["kind"] = json!("praxis-fitness");
    assert!(!world.call("learn", Some(request)).status.success());
    let mut request = world.request();
    request["ownerReturn"] = json!({"owner_ref":"central","source_ref":"method:test","proposal_ref":"proposal:test","recognition_required":false});
    assert!(!world.call("learn", Some(request)).status.success());
    assert_eq!(before, std::fs::read(world.state()).unwrap());
    world.action(FactoryAttemptOperation::Fail {
        attempt_ref: "attempt:learning".into(),
        reason: "controlled failure".into(),
        evidence_refs: BTreeSet::from(["evidence:actual-failure".into()]),
    });
    let mut request = world.request();
    request["kind"] = json!("praxis-fitness");
    request["evidenceRefs"] = json!(["evidence:actual-failure"]);
    request["ownerReturn"] = json!({"owner_ref":"central","source_ref":"method:test","proposal_ref":"proposal:test","recognition_required":true});
    assert_eq!(
        success(world.call("learn", Some(request)))["needsReconciliation"],
        false
    );
    assert_eq!(
        world.observations()["observations"][0]["owner_return"]["recognition_required"],
        true
    );
}
#[test]
fn failed_intake_recovers_original_observation_without_duplicate_ledger() {
    let world = World::new();
    std::fs::write(world.ledger(), "controlled obstruction").unwrap();
    let request = world.request();
    assert_eq!(
        success(world.call("learn", Some(request.clone())))["needsReconciliation"],
        true
    );
    std::fs::remove_file(world.ledger()).unwrap();
    let mut recovery = request;
    recovery["recover"] = json!(true);
    recovery["expectedRevision"] = json!(world.reading().revision);
    let receipt = success(world.call("learn", Some(recovery.clone())));
    assert_eq!(receipt["needsReconciliation"], false);
    assert_eq!(world.observations()["observationCount"], 1);
    recovery["expectedRevision"] = json!(world.reading().revision);
    assert_eq!(
        success(world.call("learn", Some(recovery)))["needsReconciliation"],
        false
    );
    assert_eq!(world.observations()["observationCount"], 1);
}
#[test]
fn stale_and_unauthorised_learning_do_not_create_a_ledger() {
    let world = World::new();
    let mut request = world.request();
    request["authority"]["actionAuthorised"] = json!(false);
    assert!(!world.call("learn", Some(request)).status.success());
    let mut request = world.request();
    request["expectedRevision"] = json!(1);
    assert!(!world.call("learn", Some(request)).status.success());
    assert!(!std::path::Path::new(&world.ledger()).exists());
}
#[test]
fn ordinary_public_observation_writers_do_not_erase_one_another() {
    let world = World::new();
    let mut children = Vec::new();
    for index in 0..8 {
        let observation = json!({"observation_ref":format!("observation:concurrent-{index}"),"kind":"insufficient-evidence","statement":"controlled concurrent native intake","subject_refs":["source:test"],"evidence_refs":[],"owner_return":null});
        children.push(spawn(
            &[
                "development".into(),
                "observe".into(),
                world.ledger(),
                RUN.into(),
                "-".into(),
                "--json".into(),
            ],
            Some(observation),
        ));
    }
    for child in children {
        success(child.wait_with_output().unwrap());
    }
    assert_eq!(world.observations()["observationCount"], 8);
    success(world.call("learn", Some(world.request())));
    assert_eq!(world.observations()["observationCount"], 9);
}
#[test]
fn stale_ledger_save_cannot_erase_observations_and_conflicting_identity_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileProjectDevelopmentStore::new(dir.path());
    let run: RunRef = RUN.parse().unwrap();
    let mut first = ProjectDevelopmentLedger::new(run.clone());
    let mut second = first.clone();
    let entry = |reference: &str| DevelopmentObservation {
        run_ref: run.clone(),
        observation_ref: reference.into(),
        kind: DevelopmentObservationKind::InsufficientEvidence,
        statement: "retained report".into(),
        subject_refs: vec![],
        evidence_refs: vec![],
        owner_return: None,
    };
    first.add_observation(entry("observation:first")).unwrap();
    second.add_observation(entry("observation:second")).unwrap();
    store.save(&first).unwrap();
    store.save(&second).unwrap();
    assert_eq!(store.load(&run).unwrap().unwrap().observations.len(), 2);
    let before = store.load(&run).unwrap().unwrap();
    let mut conflict = before.clone();
    conflict.observations[0].statement = "rewritten report".into();
    assert!(store.save(&conflict).is_err());
    assert_eq!(store.load(&run).unwrap().unwrap(), before);
}
#[test]
fn learning_alias_and_native_discovery_share_the_public_contract() {
    let world = World::new();
    success(
        spawn(
            &[
                "development".into(),
                "attempt".into(),
                "learn".into(),
                world.state(),
                "-".into(),
                "--json".into(),
            ],
            Some(world.request()),
        )
        .wait_with_output()
        .unwrap(),
    );
    let capabilities = success(
        spawn(&["capabilities".into(), "--json".into()], None)
            .wait_with_output()
            .unwrap(),
    );
    assert!(capabilities["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "development.attempt.learn"));
    assert_eq!(world.observations()["observationCount"], 1);
}
#[test]
fn missing_learning_ledger_is_not_reported_as_a_completed_intake() {
    let world = World::new();
    let request = world.request();
    success(world.call("learn", Some(request.clone())));
    std::fs::remove_dir_all(world.ledger()).unwrap();
    let replay = success(world.call("learn", Some(request.clone())));
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["recordedInExistingLedger"], false);
    assert_eq!(replay["needsReconciliation"], true);
    let mut recovery = request;
    recovery["recover"] = json!(true);
    recovery["expectedRevision"] = json!(world.reading().revision);
    assert_eq!(
        success(world.call("learn", Some(recovery)))["needsReconciliation"],
        false
    );
    assert_eq!(world.observations()["observationCount"], 1);
}
