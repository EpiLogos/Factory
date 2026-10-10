//! Commissioned workflows conduct their authored C′ compositions (#217).
//! Every mutation below runs the real `factory workflow commission` admission,
//! the native attempt Actions and the real inspector against throwaway state.
//! No provider runs: the only evidence is native developmental state.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_native_store::FileAttemptStore;
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::ReturnedArtifact;
use epilogos_factory::project_development_store::read_developmental_state;
use epilogos_factory::vak_orchestration::{derive_conduct_plans, VakConductPlan};
use epilogos_factory::workflow::CompiledWorkflow;
use epilogos_factory::workflow_authoring::{cli, load_workflow};
use epilogos_factory::workflow_inputs::SelectedWorkflowInput;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const COMPOSED: &str = "expression-development.workflow.ts";
const JOURNEY: &str = "journey:commission-conduct";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ql-vak-workflows")
}

/// One commissioned Run over a real TypeScript source, admitted through the
/// actual `factory workflow commission` command into a throwaway state file.
struct World {
    /// Holds the throwaway world alive for the whole test.
    #[allow(dead_code)]
    dir: tempfile::TempDir,
    state: PathBuf,
    run: RunRef,
    compiled: CompiledWorkflow,
}

impl World {
    fn commission(source: &str, request: Value) -> Self {
        // macOS exposes its temp root through a symlink; canonicalize like the
        // other controlled worlds so native file admission never redirects.
        let base = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::TempDir::new_in(&base).unwrap();
        for name in ["NOW", "work", "Control"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        let state = dir.path().join("state.json");
        let request_path = dir.path().join("request.json");
        fs::write(&request_path, serde_json::to_vec_pretty(&request).unwrap()).unwrap();
        let source_path = if source == COMPOSED {
            fixtures().join(source)
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("workflow-sdk/examples")
                .join(source)
        };
        let receipt = cli::execute(&[
            "workflow".into(),
            "commission".into(),
            state.display().to_string(),
            request_path.display().to_string(),
            source_path.display().to_string(),
        ])
        .unwrap();
        assert_eq!(receipt["execution"], "not-requested");
        let run: RunRef = receipt["commission"]["commission"]["runRef"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let parent = source_path.parent().unwrap();
        let entry = source_path.file_name().unwrap();
        let compiled = load_workflow(parent, Path::new(entry)).unwrap().compiled;
        Self {
            dir,
            state,
            run,
            compiled,
        }
    }

    /// The QL-authored composed fixture: one melody, one chord, one single.
    fn composed() -> Self {
        Self::commission(
            COMPOSED,
            json!({
                "contract": "factory.commission-request/v1",
                "requestRef": "commission-request:expression-development-conduct",
                "projectKey": "expression-development-conduct",
                "purpose": "Admit the QL-authored Expression source and derive its conduct",
                "frontier": "Declared source; no execution has been requested",
                "runDestination": "expression-development/conduct-wiring",
                "writeOwner": "factory",
                "commissionedAt": "2026-10-10T00:00:00Z",
                "participantRequirements": [
                    {"ref": "agent/anima", "description": "Anima conducts the constitutional voices",
                     "sourceOwner": "central", "sourceRef": "central:agent-set/anima", "sourceRevision": "specimen-r1"},
                    {"ref": "agent-set/anima", "description": "Anima's constitutional voices",
                     "sourceOwner": "central", "sourceRef": "central:agent-set/anima", "sourceRevision": "specimen-r1"},
                    {"ref": "agent-set/aletheia", "description": "Aletheia's disclosure and Return",
                     "sourceOwner": "central", "sourceRef": "central:agent-set/aletheia", "sourceRevision": "specimen-r1"}
                ],
                "rootAct": {"actRef": "act:expression-development-conduct", "agentRef": "agent/anima",
                    "purpose": "Bound the Expression development conduct wiring proof", "scopeRefs": ["project:O-I"],
                    "standing": "commissioned-not-executed"}
            }),
        )
    }

    /// The generic single-voice SDK specimen: no adapter, no composition.
    fn plain() -> Self {
        Self::commission(
            "single.workflow.ts",
            serde_json::from_str(include_str!("../workflow-sdk/examples/commission.json")).unwrap(),
        )
    }

    fn unit(&self, key: &str) -> &WorkflowUnitRef {
        &self.compiled.unit(key).unwrap().reference
    }

    fn store(&self) -> FileAttemptStore {
        FileAttemptStore::open_run(&self.state, self.run.clone()).unwrap()
    }

    fn inspect(&self) -> Value {
        cli::execute(&[
            "workflow".into(),
            "inspect".into(),
            self.state.display().to_string(),
            self.run.to_string(),
            "--json".into(),
        ])
        .unwrap()
    }

    fn stored(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.state).unwrap()).unwrap()
    }

    fn apply(&self, operation: FactoryAttemptOperation) -> Value {
        let revision = self.store().reading().unwrap().revision;
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:conduct-{revision}"),
            caller: FactoryActionCaller {
                caller_ref: "caller:commission-conduct".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["caller:commission-conduct".into()],
            },
            run_ref: self.run.clone(),
            expected_revision: revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:commission-conduct".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        };
        serde_json::to_value(self.store().apply(request).unwrap()).unwrap()
    }

    /// The exact predecessor selections the unit's authored inputs demand,
    /// taken from the actual current Returns in the retained coordinator.
    fn selected_inputs(&self, key: &str) -> Vec<SelectedWorkflowInput> {
        let unit = self.compiled.unit(key).unwrap();
        let reading = self.store().reading().unwrap();
        unit.inputs
            .iter()
            .map(|declared| {
                let leg = reading.legs.get(&declared.predecessor).unwrap_or_else(|| {
                    panic!(
                        "authored predecessor {} has not started",
                        declared.predecessor
                    )
                });
                SelectedWorkflowInput {
                    predecessor: declared.predecessor.clone(),
                    execution_ref: leg.execution_ref.clone(),
                    receiving_context_ref: declared.receiving_context_ref.clone(),
                    artifacts: leg.artifacts.clone(),
                }
            })
            .collect()
    }

    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let unit = self.compiled.unit(key).unwrap();
        let agent = unit
            .agent_requirements
            .agent_refs
            .iter()
            .next()
            .cloned()
            .expect("every composed voice declares its actor");
        let agency = unit
            .agent_requirements
            .agency_refs
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "agency:commission-conduct".into());
        let demand = ExecutionDemand {
            project_ref: {
                let native = read_developmental_state(&self.state).unwrap();
                native.build.project().reference().to_string()
            },
            run_ref: self.run.to_string(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "commission-conduct-wiring".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::new(),
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
                model_ref: "model:commission-conduct".into(),
                provider_ref: "provider:commission-conduct".into(),
                ranking_policy: "commission-conduct".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:commission-conduct".into()],
            },
            "2026-10-10T00:00:00Z",
        )
        .unwrap();
        let mut context_refs = BTreeSet::from(["context:commission-conduct".to_string()]);
        context_refs.extend(
            unit.inputs
                .iter()
                .map(|input| input.receiving_context_ref.clone()),
        );
        SituatedExecutionDisposition {
            selected_inputs: self.selected_inputs(key),
            selection,
            participant: SituatedParticipant {
                agent_ref: agent,
                agency_ref: agency,
                world_binding_ref: "world-binding:commission-conduct".into(),
                profile_ref: None,
                position_ref: None,
                source_ref: unit.subject_ref.to_string(),
                source_revision: unit.basis_revision.clone(),
                source_digest: format!("blake3:{}", blake3::hash(key.as_bytes()).to_hex()),
            },
            context_refs,
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:commission-conduct".into(),
                provider_ref: "provider:commission-conduct".into(),
                route_ref: "route:commission-conduct".into(),
                harness_ref: "harness:commission-conduct".into(),
                harness_composition_ref: "composition:commission-conduct".into(),
                agent_session_ref: format!("agent-session/{key}"),
                session_space_ref: format!("session-space/{key}"),
                material_world_ref: None,
                workcell_ref: None,
            },
            placement: Some(PlacementProtection {
                now_ref: "now:commission-conduct".into(),
                now_path: self.dir.path().join("NOW").to_str().unwrap().into(),
                policy_ref: "policy:commission-conduct".into(),
                policy_revision: "fixture-policy-v1".into(),
                authority_ref: "authority:commission-conduct".into(),
                writable_paths: BTreeSet::from([
                    self.dir.path().join("NOW").to_str().unwrap().to_owned(),
                    self.dir.path().join("work").to_str().unwrap().to_owned(),
                ]),
                protected_paths: BTreeSet::from([self
                    .dir
                    .path()
                    .join("Control")
                    .to_str()
                    .unwrap()
                    .to_owned()]),
                required_coverage: BTreeSet::from(["wiring-proof".to_string()]),
                effective_coverage: BTreeSet::from(["wiring-proof".to_string()]),
                write_boundary_ref: Some("boundary:controlled-fixture-not-kernel-proof".into()),
                material_receipt_ref: Some("material-receipt:commission-conduct".into()),
            }),
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(5000),
                retry_grant_ref: None,
                maximum_attempts: Some(1),
            },
        }
    }

    fn start(&self, attempt_ref: &str, key: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::StartSerial {
            attempt_ref: attempt_ref.into(),
            task_ref: format!("task:{key}"),
            parent_journey_ref: JOURNEY.into(),
            workflow_unit_ref: self.unit(key).clone(),
            disposition: self.disposition(key),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        }
    }

    fn fork(&self, voices: &[(&str, &str)]) -> FactoryAttemptOperation {
        FactoryAttemptOperation::StartFork {
            parent_journey_ref: JOURNEY.into(),
            attempts: voices
                .iter()
                .map(|(attempt_ref, key)| AttemptStart {
                    attempt_ref: attempt_ref.to_string(),
                    task_ref: format!("task:{key}"),
                    workflow_unit_ref: self.unit(key).clone(),
                    disposition: self.disposition(key),
                    retry_grant: None,
                    tracking: vec![],
                    place_grant: None,
                })
                .collect(),
        }
    }

    /// The controlled native round trip that lets a later voice start:
    /// a Returned owner dispatch, then the artifact Return. Receipts are
    /// explicitly test-only.
    fn complete(&self, attempt_ref: &str, key: &str) {
        self.apply(FactoryAttemptOperation::BindDispatch {
            attempt_ref: attempt_ref.into(),
            execution_ref: format!("execution:{attempt_ref}"),
            receipt: OwnerOperationReceipt {
                owner_ref: "aikit".into(),
                contract: "controlled-owner-fixture/v1".into(),
                operation_ref: format!("delivery:{attempt_ref}"),
                receipt_ref: format!("receipt:{attempt_ref}:dispatch"),
                source_revision: "controlled-owner-fixture".into(),
                phase: OwnerOperationPhase::Returned,
                evidence_refs: BTreeSet::from(["evidence:controlled-owner".to_string()]),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"testOnly":true}),
            },
        });
        let unit = self.compiled.unit(key).unwrap();
        self.apply(FactoryAttemptOperation::RecordVerification {
            attempt_ref: attempt_ref.into(),
            verification: VerificationReceipt {
                verification_ref: format!("verification:{attempt_ref}"),
                owner_ref: "factory-verifier:controlled-test".into(),
                source_revision: "controlled-verifier-v1".into(),
                outcome: VerificationOutcome::Passed,
                obligations: unit.verification_obligations.clone(),
                evidence_refs: BTreeSet::from(["evidence:controlled-verification".to_string()]),
            },
        });
        let artifact_ref = format!("artifact:{attempt_ref}");
        self.apply(FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: attempt_ref.into(),
            artifact: ReturnedArtifact {
                artifact_ref: artifact_ref.clone(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: format!("execution:{attempt_ref}"),
                evidence_refs: BTreeSet::from(["evidence:controlled-owner".to_string()]),
                semantic_difference: "Controlled wiring proof; no provider proof".into(),
            },
            readable_return: ReadableReturn {
                return_ref: format!("return:{attempt_ref}"),
                summary: "Controlled native Return for the conduct wiring proof".into(),
                artifact_refs: BTreeSet::from([artifact_ref]),
                evidence_refs: BTreeSet::from(["evidence:controlled-owner".to_string()]),
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        });
    }

    fn leg_status(&self, inspection: &Value, key: &str) -> String {
        inspection["legs"][self.unit(key).to_string()]["status"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The expected plans, derived directly from the same compiled fixture.
    fn expected_plans(&self) -> Vec<VakConductPlan> {
        derive_conduct_plans(&self.compiled)
            .unwrap()
            .plans()
            .to_vec()
    }
}

#[test]
fn commissioned_compositions_derive_and_persist_their_conduct_plans() {
    let world = World::composed();
    let expected = world.expected_plans();
    assert_eq!(expected.len(), 3, "melody, chord and single derive");
    let forms = expected
        .iter()
        .map(|plan| (plan.binding.thread.as_str(), plan.units.len()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        forms.get("CFP2"),
        Some(&5),
        "the melody keeps its five voices"
    );
    assert_eq!(
        forms.get("CFP1"),
        Some(&2),
        "the chord keeps its two readings"
    );
    assert_eq!(
        forms.get("CFP0"),
        Some(&1),
        "the single keeps its one voice"
    );

    let inspection = world.inspect();
    let conduct = &inspection["conduct"];
    assert_eq!(conduct["standing"], "derived-at-admission");
    let readback: Vec<VakConductPlan> =
        serde_json::from_value(conduct["derivedPlans"].clone()).unwrap();
    assert_eq!(
        readback, expected,
        "the inspector reads exactly the derived plans"
    );
    assert_eq!(
        conduct["performedPerformances"].as_array().unwrap().len(),
        0,
        "admission derives conduct; it performs nothing"
    );

    // The plans are persisted in the native attempt field itself, so they
    // survive reopen exactly like the retained source and coordinator.
    let stored = world.stored();
    let field = &stored["state"]["attemptStates"][world.run.to_string()];
    let persisted = field["derivedConductPlans"].as_array().unwrap();
    assert_eq!(persisted.len(), 3);
    let reopened: Vec<VakConductPlan> =
        serde_json::from_value(serde_json::Value::Array(persisted.clone())).unwrap();
    assert_eq!(reopened, expected);
    let store = world.store();
    let field = FileAttemptStore::open_run(&world.state, world.run.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert_eq!(field.derived_conduct_plans, expected);
    drop(store);
}

#[test]
fn commissioned_start_conducts_the_derived_opening_and_records_the_performance() {
    let world = World::composed();
    world.apply(world.start("attempt:nous", "nous-open-ground"));

    let inspection = world.inspect();
    let performances = inspection["conduct"]["performedPerformances"]
        .as_array()
        .unwrap();
    assert_eq!(performances.len(), 1, "exactly the opened melody conducted");
    let melody = world
        .expected_plans()
        .into_iter()
        .find(|plan| plan.binding.thread == "CFP2")
        .unwrap();
    let performed = &performances[0];
    assert_eq!(performed["performanceRef"], melody.performance_ref);
    assert_eq!(performed["thread"], "CFP2");
    assert_eq!(performed["qlBindingRef"], "ql/interpretation/c-prime");
    assert_eq!(
        performed["attempts"].as_array().unwrap().len(),
        1,
        "only the opening voice has executed"
    );
    assert_eq!(
        performed["attempts"][0]["unitRef"],
        world.unit("nous-open-ground").to_string()
    );
    assert_eq!(
        performed["attempts"][0]["modelRef"],
        "model:commission-conduct"
    );
    assert_eq!(
        performed["attempts"][0]["providerRef"],
        "provider:commission-conduct"
    );

    // The conducted opening is ordinary native work: the attempt record and
    // the coordinator leg exist exactly as a plain start would leave them.
    let attempts = inspection["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["attemptRef"], "attempt:nous");
    assert_eq!(
        attempts[0]["workflowUnitRef"],
        world.unit("nous-open-ground").to_string()
    );
    assert_eq!(world.leg_status(&inspection, "nous-open-ground"), "active");
    let stored = world.stored();
    assert_eq!(
        stored["state"]["attemptStates"][world.run.to_string()]["conductPerformances"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn derived_openings_conduct_once_and_continuations_stay_on_the_plain_path() {
    let world = World::composed();
    world.apply(world.start("attempt:nous", "nous-open-ground"));
    world.complete("attempt:nous", "nous-open-ground");
    // The melody's second voice opens no performance of its own: the derived
    // plan only matches its opening unit, so the continuation takes the same
    // plain bounded-coordination start it always took.
    world.apply(world.start("attempt:logos", "logos-articulate"));

    let inspection = world.inspect();
    let performances = inspection["conduct"]["performedPerformances"]
        .as_array()
        .unwrap();
    assert_eq!(
        performances.len(),
        1,
        "a continuation never opens a second derived performance"
    );
    assert_eq!(
        world.leg_status(&inspection, "nous-open-ground"),
        "returned"
    );
    assert_eq!(world.leg_status(&inspection, "logos-articulate"), "active");
}

#[test]
fn commissioned_fork_conducts_the_derived_chord_over_its_exact_set() {
    let world = World::composed();
    world.apply(world.start("attempt:nous", "nous-open-ground"));
    world.complete("attempt:nous", "nous-open-ground");
    world.apply(world.start("attempt:logos", "logos-articulate"));
    world.complete("attempt:logos", "logos-articulate");
    world.apply(world.fork(&[
        ("attempt:eros", "eros-exchange"),
        ("attempt:mythos", "mythos-image"),
    ]));

    let inspection = world.inspect();
    let performances = inspection["conduct"]["performedPerformances"]
        .as_array()
        .unwrap();
    assert_eq!(performances.len(), 2, "the melody and the chord conducted");
    let chord = &performances[1];
    assert_eq!(chord["thread"], "CFP1");
    let attempted = chord["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|attempt| attempt["unitRef"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        attempted,
        BTreeSet::from([
            world.unit("eros-exchange").to_string(),
            world.unit("mythos-image").to_string(),
        ]),
        "the chord forked its exact authored unit set"
    );
    assert_eq!(world.leg_status(&inspection, "eros-exchange"), "active");
    assert_eq!(world.leg_status(&inspection, "mythos-image"), "active");
}

#[test]
fn compositionless_commission_and_start_are_exactly_the_plain_path() {
    let world = World::plain();
    let inspection = world.inspect();
    assert_eq!(
        inspection["conduct"]["standing"],
        "no-authored-compositions"
    );
    assert!(inspection["conduct"]["derivedPlans"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(inspection["conduct"]["performedPerformances"]
        .as_array()
        .unwrap()
        .is_empty());
    // Nothing conduct-shaped is persisted for a plain workflow: the state
    // document keeps its earlier exact shape.
    let stored = world.stored();
    let field = &stored["state"]["attemptStates"][world.run.to_string()];
    assert!(field.get("derivedConductPlans").is_none());
    assert!(field.get("conductPerformances").is_none());

    world.apply(world.start("attempt:plain", "inspect-source"));
    let inspection = world.inspect();
    assert!(
        inspection["conduct"]["performedPerformances"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a composition-less start opens no performance"
    );
    assert_eq!(world.leg_status(&inspection, "inspect-source"), "active");
    let stored = world.stored();
    let field = &stored["state"]["attemptStates"][world.run.to_string()];
    assert!(field.get("derivedConductPlans").is_none());
    assert!(field.get("conductPerformances").is_none());
}
