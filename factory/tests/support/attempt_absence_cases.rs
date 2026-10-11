//! Real Factory processes and a controlled executable owner with a durable
//! SQLite delivery key. These prove orchestration, never live Pi inference.
use super::*;
use epilogos_factory::attempt_owner_dispatch::AIKIT_TASK_CONTRACT_REVISION;

const CONTROLLED_OWNER: &str = r#"#!/usr/bin/env python3
import json,pathlib,sqlite3,sys
r=pathlib.Path(__file__).resolve().parent
args=sys.argv[3:]
assert sys.argv[1]=='-C' and args[0]=='session-space'
source=json.loads((r/'controlled-native.json').read_text())
db=sqlite3.connect(r/'controlled-deliveries.sqlite')
db.execute('CREATE TABLE IF NOT EXISTS delivery(session TEXT,delivery TEXT,body TEXT,PRIMARY KEY(session,delivery))')
if args[1]=='encounter-task-read':
 print(json.dumps(source['task']));sys.exit(0)
if args[1]=='encounter-agency-read':
 print(json.dumps(source['binding']));sys.exit(0)
q=json.loads(args[args.index('--request-json')+1])
with (r/'controlled-calls.jsonl').open('a') as f:f.write(json.dumps(q)+'\n')
if q['action']=='status':
 calls=[json.loads(x) for x in (r/'controlled-calls.jsonl').read_text().splitlines()]
 status=source['status']
 if source.get('move_after_first_status') and sum(x['action']=='status' for x in calls)>1:
  status['data']['connection_generation']='controlled-generation-successor'
 print(json.dumps(status));sys.exit(0)
key=(q['agent_session'],q.get('delivery_ref',q.get('turn',{}).get('delivery_ref')))
if q['action']=='delivery':
 row=db.execute('SELECT body FROM delivery WHERE session=? AND delivery=?',key).fetchone()
 print(json.dumps({'ok':True,'data':json.loads(row[0]) if row else None}));sys.exit(0)
assert q['action']=='send'
native=json.loads((r/'state.json').read_text())
assert 'dispatching' in json.dumps(native),'Factory intent missing before native effect'
turn=q['turn'];allowed=set(source['binding']['binding']['allowed_packet_sources'])
if not set(turn['packet']['source_refs']).issubset(allowed):
 print(json.dumps({'ok':False,'error':{'code':'encounter.disclosure_denied',
  'message':'PRIVATE-CONTROLLED-REFUSAL-BODY'}}));sys.exit(0)
db.execute('INSERT INTO delivery VALUES(?,?,?)',(*key,json.dumps(q)))
db.commit()
(r/'controlled-provider-effects').write_text('actual controlled provider boundary reached')
print(json.dumps({'ok':True,'data':{'delivery_ref':key[1],'agent_session':key[0],'phase':'submitted',
 'request':{'submission':{'turn':turn}}}}))
"#;

fn digest(v: &Value) -> String {
    format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(v).unwrap()).to_hex()
    )
}

struct Missing {
    world: World,
    original: Value,
}
impl Missing {
    fn new() -> Self {
        let world = World::new();
        let unit = world.workflow.unit("inspect-source").unwrap();
        // Rebind the checked native compatibility fixture solely to this
        // controlled process. This is not an issued live model receipt.
        let mut selected: AikitModelRosterSelection =
            serde_json::from_str(include_str!("../fixtures/aikit-explicit-pi-selection.json"))
                .unwrap();
        let receipt = &mut selected.ranking_explanation;
        let b = &mut receipt["basis"];
        b["native"]["agent_session"] = json!(SESSION);
        b["native"]["native_session_id"] = json!("controlled-native-uuid");
        b["native"]["space"] = json!("space:test");
        b["harness_composition"]["agent"] =
            json!(unit.agent_requirements.agent_refs.iter().next().unwrap());
        b["harness_composition"]["agency"] = json!("agency:transport");
        b["harness_composition"]["session"] = json!(SESSION);
        let target = &mut b["composition_target_basis"];
        target["world_binding_ref"] = json!("world-binding:test");
        target["agency_source"] = json!({"source_ref":world.workflow.source.reference,
            "revision":world.workflow.source.revision,"content_digest":format!("blake3:{}",world.workflow.source.digest),
            "path":world.dir.path().join("controlled-source.json")});
        target["resident_body_basis"]["cwd"] = json!(world.dir.path());
        target["resident_body_basis"]["task_bound"] = json!(true);
        target["resident_body_basis"]["owner_launcher_provider_id"] =
            json!("controlled-task-launcher");
        let target_revision = digest(target);
        b["harness_composition"]["target_revision"] = json!(target_revision);
        receipt["basis_digest"] = json!(digest(&receipt["basis"]));
        epilogos_factory::execution_intelligence::explicit_selection_refs(&selected).unwrap();
        let demand = ExecutionDemand {
            project_ref: PROJECT.into(),
            run_ref: RUN.into(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some("agency:transport".into()),
            profile_ref: None,
            use_type: "controlled-only".into(),
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
        let selection =
            accept_aikit_selection(demand, selected.clone(), "controlled-time").unwrap();
        let refs = epilogos_factory::execution_intelligence::explicit_selection_refs(&selected)
            .unwrap()
            .unwrap();
        let source =
            &selected.ranking_explanation["basis"]["composition_target_basis"]["agency_source"];
        let placement = PlacementProtection {
            now_ref: "now:controlled-task".into(),
            now_path: world.dir.path().to_string_lossy().into(),
            policy_ref: "policy:controlled".into(),
            policy_revision: "policy:controlled-r1".into(),
            authority_ref: "authority:controlled".into(),
            writable_paths: BTreeSet::new(),
            protected_paths: BTreeSet::new(),
            required_coverage: BTreeSet::new(),
            effective_coverage: BTreeSet::new(),
            write_boundary_ref: Some("sha256:controlled-requirements".into()),
            material_receipt_ref: None,
        };
        let participant = SituatedParticipant {
            agent_ref: refs.agent_ref,
            agency_ref: refs.agency_ref,
            world_binding_ref: refs.world_binding_ref,
            profile_ref: None,
            position_ref: None,
            source_ref: refs.source_ref,
            source_revision: refs.source_revision,
            source_digest: refs.source_digest,
        };
        let expected = json!({"revision":"task-binding/controlled-r1","task_ref":"task:inspect",
            "now_ref":placement.now_ref,"now_revision":"now:controlled-r1","policy_revision":placement.policy_revision,
            "cwd":world.dir.path(),"agent_ref":participant.agent_ref,"agency_ref":participant.agency_ref,
            "world_binding_ref":participant.world_binding_ref,"source_ref":participant.source_ref,
            "source_revision":participant.source_revision,"source_digest":participant.source_digest});
        let native = json!({"binding":{"schema":"aikit.encounter-agency-source/v1","agent_session":SESSION,
            "source_verified":true,"binding":{"active":true,"revision":"binding-r1","agent_ref":participant.agent_ref,
                "agency_ref":participant.agency_ref,"world_binding_ref":participant.world_binding_ref,"agency_source":source,
                "allowed_senders":[caller().caller_ref],"allowed_packet_sources":["source:allowed"]}},
            "status":{"ok":true,"data":{"agent_session":SESSION,"native_session_id":"controlled-native-uuid",
                "connection_generation":"controlled-generation-r1","state":"Resident","error":null,
                "provider":{"id":"controlled-task-launcher"}}},
            "task":{"schema":"aikit.encounter-task/v1","ready":true,"revision":expected["revision"],
                "agency_revision":"binding-r1","request":{"central":{"task_ref":"task:inspect"},
                    "cwd":world.dir.path(),"authority_ref":placement.authority_ref},
                "allocation":{"allocation":{"now_ref":placement.now_ref,"writable_destination":placement.now_path,
                    "revision":{"revision":"now:controlled-r1"},"policy":{"revision":placement.policy_revision}}},
                "requirements":{"schema":"workcell.write-boundary/v1","policy_ref":placement.policy_ref,
                    "policy_revision":placement.policy_revision,"authority_ref":placement.authority_ref,
                    "writable_paths":[world.dir.path()],"protected_paths":[],"required_coverage":[]},
                "material":null,"prepared_run":null}});
        let mut native = native;
        native["task"]["inspection"] = json!({"schema":"workcell.prepared-write-boundary/v1",
            "requirements":native["task"]["requirements"],"requirements_digest":"sha256:controlled-requirements"});
        fs::write(
            world.dir.path().join("controlled-native.json"),
            native.to_string(),
        )
        .unwrap();
        fs::write(world.dir.path().join("owner.py"), CONTROLLED_OWNER).unwrap();
        let disposition = SituatedExecutionDisposition {
            selection,
            participant,
            selected_inputs: vec![],
            context_refs: BTreeSet::new(),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            praxis_receipt: (!unit.praxis_refs.is_empty()).then(|| {
                super::praxis_receipt_fixture::resolved_receipt(
                    &unit.reference.to_string(),
                    &unit.praxis_refs,
                )
            }),
            now_context_ref: (!unit.praxis_refs.is_empty()).then(|| placement.now_ref.clone()),
            body: ExecutionBody {
                model_ref: selected.model_ref,
                provider_ref: selected.provider_ref,
                route_ref: refs.route_ref,
                harness_ref: refs.harness_ref,
                harness_composition_ref: refs.harness_composition_ref,
                agent_session_ref: SESSION.into(),
                session_space_ref: "space:test".into(),
                material_world_ref: None,
                workcell_ref: None,
            },
            placement: Some(placement),
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(30_000),
                retry_grant_ref: Some("grant:controlled-absence".into()),
                maximum_attempts: Some(2),
            },
        };
        success(
            world.action(FactoryAttemptOperation::StartSerial {
                attempt_ref: ATTEMPT.into(),
                task_ref: "task:inspect".into(),
                parent_journey_ref: "journey:test".into(),
                workflow_unit_ref: unit.reference.clone(),
                disposition,
                retry_grant: Some(
                    epilogos_factory::orchestration::RetryGrant::new("grant:controlled-absence", 2)
                        .unwrap(),
                ),
                tracking: vec![],
                place_grant: None,
            }),
        );
        let mut original = world.request("send:controlled-disclosure-refusal", "send");
        original["invocation"]["contract_revision"] = json!(AIKIT_TASK_CONTRACT_REVISION);
        original["invocation"]["request"]["turn"]["expected_task"] = expected;
        original["invocation"]["request"]["turn"]["packet"]["source_refs"] =
            json!(["source:not-admitted"]);
        let failure = world.owner(&original);
        failed_owner_call(&world, "send:controlled-disclosure-refusal", failure);
        Self { world, original }
    }
    fn recover(&self, id: &str) -> Value {
        let mut request = self.world.request(id, "delivery");
        request["invocation"]["contract_revision"] = json!(AIKIT_TASK_CONTRACT_REVISION);
        request["invocation"]["request"] = json!({"action":"reconcile-no-delivery","agent_session":SESSION,
            "delivery_ref":"delivery:test","original_request":self.original,
            "expected_connection_generation":"controlled-generation-r1",
            "generation_observation_ref":"evidence:controlled-initial-body-observation"});
        request
    }
    fn change(&self, f: impl FnOnce(&mut Value)) {
        let p = self.world.dir.path().join("controlled-native.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        f(&mut value);
        fs::write(p, value.to_string()).unwrap();
    }
    fn sends(&self) -> usize {
        fs::read_to_string(self.world.dir.path().join("controlled-calls.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|v| v["action"] == "send")
            .count()
    }
}

#[test]
fn semantic_refusal_and_exact_absence_reconcile_without_execution_or_resend() {
    let fixture = Missing::new();
    let reading = fixture.world.reading();
    let failure = reading.attempts[0].observations.last().unwrap();
    assert_eq!(
        failure.payload["detail"]["nativeFailure"]["nativeCode"],
        "encounter.disclosure_denied"
    );
    assert_eq!(failure.payload["detail"]["nativeFailure"]["exitCode"], 0);
    assert!(!serde_json::to_string(&reading)
        .unwrap()
        .contains("PRIVATE-CONTROLLED-REFUSAL-BODY"));
    assert!(success(fixture.world.owner(&fixture.original))["replayed"] == true);
    assert_eq!(fixture.sends(), 1);
    assert!(!fixture
        .world
        .dir
        .path()
        .join("controlled-provider-effects")
        .exists());
    let mut request = fixture.recover("read:exact-absent");
    let observer = fixture.world.dir.path().join("observer.py");
    fs::copy(fixture.world.dir.path().join("owner.py"), &observer).unwrap();
    request["invocation"]["binary"] = json!(observer);
    let result = success(fixture.world.owner(&request));
    assert_eq!(result["needsReconciliation"], false);
    assert_eq!(result["ownerReceipt"]["phase"], "reconciled-no-replay");
    assert_eq!(
        result["ownerReceipt"]["payload"]["readOnlyObservationBinary"],
        json!(observer)
    );
    assert!(fixture.world.reading().attempts[0].execution_ref.is_none());
    assert!(fixture.world.reading().attempts[0]
        .readable_return
        .is_none());
    let bytes = fs::read(fixture.world.state()).unwrap();
    assert_eq!(success(fixture.world.owner(&request))["replayed"], true);
    assert_eq!(bytes, fs::read(fixture.world.state()).unwrap());
    success(
        fixture.world.action(FactoryAttemptOperation::Fail {
            attempt_ref: ATTEMPT.into(),
            reason: "source disclosure refused before reservation".into(),
            evidence_refs: BTreeSet::from([result["ownerReceipt"]["receiptRef"]
                .as_str()
                .unwrap()
                .into()]),
        }),
    );
    assert_eq!(fixture.sends(), 1);
}

#[test]
fn missing_delivery_cannot_settle_changed_or_admissible_source_body_task() {
    for case in [
        "generation",
        "uuid",
        "binding",
        "source",
        "task",
        "requirements",
        "admissible",
        "moving",
    ] {
        let fixture = Missing::new();
        fixture.change(|v| match case {
            "generation" => v["status"]["data"]["connection_generation"] = json!("new-generation"),
            "uuid" => v["status"]["data"]["native_session_id"] = json!("other-native-uuid"),
            "binding" => v["binding"]["binding"]["revision"] = json!("other-binding"),
            "source" => v["binding"]["source_verified"] = json!(false),
            "task" => v["task"]["revision"] = json!("other-task"),
            "requirements" => {
                v["task"]["inspection"]["requirements_digest"] = json!("other-digest")
            }
            "admissible" => {
                v["binding"]["binding"]["allowed_packet_sources"] = json!(["source:not-admitted"])
            }
            "moving" => v["move_after_first_status"] = json!(true),
            _ => unreachable!(),
        });
        failed_owner_call(
            &fixture.world,
            "read:unsafe-absence",
            fixture.world.owner(&fixture.recover("read:unsafe-absence")),
        );
        assert!(fixture.world.reading().attempts[0].execution_ref.is_none());
        assert_eq!(fixture.sends(), 1, "{case}");
        assert!(!fixture
            .world
            .dir
            .path()
            .join("controlled-provider-effects")
            .exists());
    }
}

#[test]
fn original_request_tampering_and_retained_delivery_cannot_become_absence_proof() {
    let fixture = Missing::new();
    let mut changed = fixture.recover("read:tampered-absence");
    changed["invocation"]["request"]["original_request"]["invocation"]["request"]["turn"]
        ["packet"]["text"] = json!("changed original intent");
    let bytes = fs::read(fixture.world.state()).unwrap();
    assert!(!fixture.world.owner(&changed).status.success());
    assert_eq!(bytes, fs::read(fixture.world.state()).unwrap());
    // A genuine controlled durable row rules out absence regardless of phase.
    let code="import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('INSERT INTO delivery VALUES(?,?,?)',(sys.argv[2],sys.argv[3],'{\"phase\":\"uncertain\"}')); c.commit()";
    assert!(Command::new("python3")
        .args(["-c", code])
        .arg(
            fixture
                .world
                .dir
                .path()
                .join("controlled-deliveries.sqlite")
        )
        .args([SESSION, "delivery:test"])
        .status()
        .unwrap()
        .success());
    failed_owner_call(
        &fixture.world,
        "read:retained-not-absent",
        fixture
            .world
            .owner(&fixture.recover("read:retained-not-absent")),
    );
    assert!(fixture.world.reading().attempts[0].execution_ref.is_none());
    assert_eq!(fixture.sends(), 1);
}
