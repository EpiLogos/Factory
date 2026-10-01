//! Real Factory and pinned Central processes over an isolated native World.
//! The bounded principals below are controlled authority fixtures, not a claim
//! that a person answered in the installed personal World. No provider double,
//! generated executable, model result or private credential is used.

use epilogos_factory::attempt_runtime::{
    AttemptTrackingFact, ExecutionBody, ExecutionBudget, FactoryAttemptReading, FactoryAttemptSeed,
    ReresolutionRecord, SituatedExecutionDisposition, SituatedParticipant,
};
use epilogos_factory::core::run::Run;
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use epilogos_factory::workflow_inputs::{SelectedWorkflowInput, WorkflowInputSource};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const HUMAN: &str = "controlled-native-receiving-human-credential-not-personal";
const OTHER_HUMAN: &str = "controlled-native-receiving-other-human-credential-not-personal";
const AGENT: &str = "controlled-native-receiving-agent-credential-not-personal";
const REVIEWER: &str = "controlled-native-receiving-reviewer-credential-not-personal";
const HUMAN_REF: &str = "human:controlled-native-receiving-owner";
const AGENT_REF: &str = "agent:controlled-native-receiving-producer";
const REVIEWER_REF: &str = "agent:controlled-native-receiving-reviewer";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const DECISION: &str = "human-request:controlled-native-receiving";

fn value(output: Output) -> Value {
    assert!(
        output.status.success(),
        "native command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("native structured output")
}

fn native_ctrl() -> PathBuf {
    let path = PathBuf::from(std::env::var_os("FACTORY_TEST_CTRL").expect(
        "supply the actual pinned ctrl binary; these tests have no provider-double fallback",
    ))
    .canonicalize()
    .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes.starts_with(b"\x7fELF")
            || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe]),
        "ctrl must be an actual native executable"
    );
    let sha = format!("{:x}", Sha256::digest(&bytes));
    if let Ok(expected) = std::env::var("FACTORY_TEST_CTRL_SHA256") {
        assert_eq!(sha, expected, "pinned native Central bytes changed");
    }
    let output = Command::new(&path).arg("--version").output().unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(
        version.starts_with("ctrl "),
        "configured executable is not Central"
    );
    eprintln!(
        "native lifecycle receiving: {} sha256:{}",
        version.trim(),
        sha
    );
    path
}

struct World {
    _dir: TempDir,
    root: PathBuf,
    state: PathBuf,
    ctrl: PathBuf,
    run: Run,
    workflow: CompiledWorkflow,
    now: Value,
    document: Value,
}

impl World {
    fn new(closure: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let ctrl = native_ctrl();
        let init = Command::new(&ctrl)
            .args(["--json", "--root"])
            .arg(&root)
            .arg("init")
            .env_remove("CENTRAL_NATIVE_TOKEN")
            .output()
            .unwrap();
        value(init);
        fs::create_dir_all(root.join("Work/native-receiving")).unwrap();
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let actions = [
            "central.now.allocate",
            "central.document.create",
            "central.receiving.submit",
            "central.receiving.review",
        ];
        let grants = [
            (HUMAN, HUMAN_REF, "human"),
            (OTHER_HUMAN, "human:controlled-other", "human"),
            (AGENT, AGENT_REF, "agent"),
            (REVIEWER, REVIEWER_REF, "agent"),
        ]
        .into_iter()
        .map(|(token, principal, kind)| {
            json!({
                "principal_ref":principal,"actor_kind":kind,
                "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),
                "scope_refs":["control:root"],"actions":actions,"expires_at_unix_seconds":expires
            })
        })
        .collect::<Vec<_>>();
        let sources = [
            (
                "placement.json",
                "work-placement-policy",
                json!({"schema":"central.work-placement-policy/v1",
                "scope_ref":"control:root","writable":[{"path":"Work/native-receiving","class":"repository"}],
                "enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300}),
            ),
            (
                "time.json",
                "civil-time-policy",
                json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root",
                    "timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}),
            ),
            (
                "authority.json",
                "native-action-authority",
                json!({"schema":"central.native-action-authority/v1",
                "scope_ref":"control:root","grants":grants}),
            ),
        ];
        let relations = sources.into_iter().map(|(name, role, source)| {
            let path = format!("Control/user/{name}");
            fs::write(root.join(&path), serde_json::to_vec_pretty(&source).unwrap()).unwrap();
            json!({"ref":format!("central:source:control:root:{path}"),"path":path,"roles":[role],
                "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
                "recognition":"controlled-native-authority-fixture-not-personal-adoption","recorded_at_unix_seconds":1})
        }).collect::<Vec<_>>();
        fs::create_dir_all(root.join("Control/relations")).unwrap();
        fs::write(root.join("Control/relations/source-relations.json"), serde_json::to_vec_pretty(
            &json!({"schema":"central.control.ground-relations/v1","project_id":"control:root","relations":relations})).unwrap()).unwrap();
        let policy = ctrl_call(&ctrl, &root, "central.work.policy", &json!({}), None);
        let now = ctrl_call(
            &ctrl,
            &root,
            "central.now.allocate",
            &json!({
            "task_ref":"task:native-lifecycle-receiving","purpose":"Controlled real-owner decision and closure regression",
            "participant_refs":[AGENT_REF,REVIEWER_REF],"source_refs":["source:controlled-native-receiving-contract"],
            "expected_policy_revision":policy["revision"]}),
            Some(AGENT),
        );
        let document = ctrl_call(
            &ctrl,
            &root,
            "central.document.create",
            &json!({
            "kind":"flow","document_id":"flow:controlled-native-receiving","title":"Controlled native receiving",
            "expected_policy_revision":policy["revision"],"template_payload":{},"fields":[]}),
            Some(AGENT),
        );
        let source = source(closure);
        let workflow = compile_workflow(source.clone()).unwrap();
        let run = Run::new(
            RUN.parse().unwrap(),
            PROJECT.parse().unwrap(),
            "Controlled native receiving regression",
            "factory",
        )
        .unwrap();
        let state = root.join("factory-development.json");
        let world = Self {
            _dir: dir,
            root,
            state,
            ctrl,
            run,
            workflow,
            now,
            document,
        };
        value(
            world.factory(
                &[
                    "attempt",
                    "init",
                    world.state.to_str().unwrap(),
                    "-",
                    "--json",
                ],
                Some(
                    &serde_json::to_value(FactoryAttemptSeed {
                        run: world.run.clone(),
                        workflow_source: source,
                    })
                    .unwrap(),
                ),
                None,
            ),
        );
        world.transition("active", None);
        world
    }

    fn factory(&self, args: &[&str], input: Option<&Value>, token: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
        command
            .args(args)
            .env("FACTORY_NATIVE_CENTRAL_BINARY", &self.ctrl)
            .env("FACTORY_NATIVE_CENTRAL_ROOT", &self.root)
            .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
            .env_remove("CENTRAL_NATIVE_TOKEN")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(token) = token {
            command.env("CENTRAL_NATIVE_TOKEN", token);
        }
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().unwrap();
        if let Some(input) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }

    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_value(value(self.factory(
            &["attempt", "read", self.state.to_str().unwrap(), "--json"],
            None,
            None,
        )))
        .unwrap()
    }

    /// Preserve actual owner outputs for downstream consumer replay. This is
    /// opt-in evidence retention, never a manufactured positive reading.
    fn closure_readings(&self, label: &str) -> (Value, Value) {
        let before = fs::read(&self.state).unwrap();
        let arguments = [
            vec!["attempt", "read", self.state.to_str().unwrap(), "--json"],
            vec![
                "development",
                "run",
                self.state.to_str().unwrap(),
                RUN,
                "--json",
            ],
            vec![
                "development",
                "build",
                self.state.to_str().unwrap(),
                RUN,
                "--json",
            ],
        ];
        let outputs = arguments
            .iter()
            .map(|args| self.factory(args, None, None))
            .collect::<Vec<_>>();
        let readings = outputs
            .iter()
            .map(|output| {
                assert!(
                    output.status.success(),
                    "native closure read failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_slice::<Value>(&output.stdout).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            fs::read(&self.state).unwrap(),
            before,
            "native closure projections are read-only"
        );
        assert_eq!(readings[1]["nativeAttempts"], readings[0]);
        assert_eq!(readings[2]["view"]["nativeAttempts"], readings[0]);
        if let Some(directory) = std::env::var_os("FACTORY_NATIVE_EVIDENCE_DIR") {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let destination =
                PathBuf::from(directory).join(format!("{label}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&destination).unwrap();
            let factory = PathBuf::from(env!("CARGO_BIN_EXE_factory"))
                .canonicalize()
                .unwrap();
            let measure = |binary: &Path| {
                let version = Command::new(binary).arg("--version").output().unwrap();
                assert!(version.status.success());
                json!({"path":binary,
                    "sha256":format!("{:x}",Sha256::digest(fs::read(binary).unwrap())),
                    "reportedVersion":String::from_utf8(version.stdout).unwrap().trim()})
            };
            let names = ["attempt-reading", "run-reading", "build-snapshot"];
            let commands = arguments
                .iter()
                .zip(outputs.iter())
                .zip(names)
                .map(|((args, output), name)| {
                    fs::write(destination.join(format!("{name}.json")), &output.stdout).unwrap();
                    fs::write(destination.join(format!("{name}.stderr")), &output.stderr).unwrap();
                    let mut argv = vec![factory.display().to_string()];
                    argv.extend(args.iter().map(|argument| (*argument).to_owned()));
                    json!({"argv":argv,"exitCode":output.status.code(),
                        "stdout":format!("{name}.json"),"stderr":format!("{name}.stderr"),
                        "stdoutSha256":format!("{:x}",Sha256::digest(&output.stdout))})
                })
                .collect::<Vec<_>>();
            let basis = json!({"contract":"factory.native-lifecycle-receiving-evidence/v1",
                "label":label,"factory":measure(&factory),"central":measure(&self.ctrl),
                "declaredSourceCut":std::env::var("FACTORY_NATIVE_EVIDENCE_SOURCE_CUT").ok(),
                "ownerStatePath":self.state,"ownerStateSha256":format!("{:x}",Sha256::digest(&before)),
                "nativeCentralRoot":self.root,"commands":commands,
                "standing":"controlled isolated native Factory effects and genuine Central authority",
                "modelInvoked":false,"agentBodyLaunched":false,"actualHumanWorldReply":false,
                "credentialsIncluded":false});
            fs::write(
                destination.join("invocation.json"),
                serde_json::to_vec_pretty(&basis).unwrap(),
            )
            .unwrap();
            eprintln!("native closure evidence: {}", destination.display());
        }
        (readings[0].clone(), readings[2].clone())
    }

    fn request(&self, operation: Value) -> Value {
        let id = blake3::hash(operation.to_string().as_bytes())
            .to_hex()
            .to_string();
        let caller = if operation["operation"] == "resolve-unit-decision" {
            json!({"callerRef":HUMAN_REF,"projectionKind":"desktop-human","lineage":[HUMAN_REF]})
        } else {
            json!({"callerRef":AGENT_REF,"projectionKind":"headless","lineage":[AGENT_REF]})
        };
        json!({"contract":"factory.attempt-action/v1","projectionRef":format!("projection:native-receiving:{id}"),
            "caller":caller,
            "runRef":RUN,"expectedRevision":self.reading().revision,
            "authority":{"authorityRef":"authority:controlled-native-factory","nativeOwner":"factory",
                "capabilityRef":"capability/factory/operate-attempt","capabilityGranted":true,"actionAuthorised":true},
            "operation":operation})
    }

    fn raw_action(&self, request: &Value) -> Output {
        self.factory(
            &[
                "attempt",
                "action",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(request),
            None,
        )
    }

    fn action(&self, operation: Value) -> Value {
        value(self.raw_action(&self.request(operation)))
    }

    fn refuse(&self, operation: Value) {
        let before = fs::read(&self.state).unwrap();
        let output = self.raw_action(&self.request(operation));
        assert!(
            !output.status.success(),
            "unadmitted owner proof must be refused"
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("unknown variant"),
            "exercise an implemented native operation"
        );
        assert_eq!(
            fs::read(&self.state).unwrap(),
            before,
            "refused native Action must not publish partially"
        );
    }

    fn transition_operation(&self, lifecycle: &str, closure: Option<Value>) -> Value {
        let state: Value = serde_json::from_slice(&fs::read(&self.state).unwrap()).unwrap();
        let run = &state["state"]["build"]["runs"]["runs"][RUN];
        let mut authority = run["writeAuthority"].clone();
        authority["runRef"] = json!(RUN);
        json!({"operation":"transition-run","command":{"commandId":format!("native:{lifecycle}:{}",run["revision"]),
            "expectedRevision":run["revision"],"lifecycle":lifecycle},"authority":authority,"closure":closure})
    }

    fn transition(&self, lifecycle: &str, closure: Option<Value>) {
        self.action(self.transition_operation(lifecycle, closure));
    }

    fn start(&self, key: &str) -> String {
        let unit = self.workflow.unit(key).unwrap();
        let reading = self.reading();
        let mut disposition = disposition(&self.run, &self.workflow, key);
        let grant =
            epilogos_factory::orchestration::RetryGrant::new(format!("grant:native-{key}"), 2)
                .unwrap();
        disposition.budget.retry_grant_ref = Some(grant.grant_ref.clone());
        disposition.budget.maximum_attempts = Some(grant.attempts_allowed);
        disposition.selected_inputs = unit
            .inputs
            .iter()
            .map(|input| {
                let leg = &reading.legs[&input.predecessor];
                disposition
                    .context_refs
                    .insert(input.receiving_context_ref.clone());
                disposition
                    .selection
                    .demand
                    .independence_from
                    .insert(leg.execution_ref.clone());
                SelectedWorkflowInput {
                    predecessor: input.predecessor.clone(),
                    execution_ref: leg.execution_ref.clone(),
                    receiving_context_ref: input.receiving_context_ref.clone(),
                    artifacts: leg.artifacts.clone(),
                }
            })
            .collect();
        let attempt = format!("attempt:native-{key}");
        self.action(json!({"operation":"start-serial","attempt_ref":attempt,"task_ref":format!("task:native-{key}"),
            "parent_journey_ref":"journey:controlled-native-receiving","workflow_unit_ref":unit.reference,
            "disposition":disposition,"retry_grant":grant,"tracking":[AttemptTrackingFact {
                fact_ref: format!("fact:now:{key}"), kind: "now".into(), owner_ref: "central".into(),
                subject_ref: self.now["now_ref"].as_str().unwrap().into(),
                source_revision: self.now["revision"]["revision"].as_str().unwrap().into(),
                evidence_refs: BTreeSet::from([self.now["record"]["source_ref"].as_str().unwrap().into()]),
            }]}));
        attempt
    }

    fn request_decision(&self) {
        let unit = self.workflow.unit("inspect-source").unwrap();
        let reading = self.reading();
        let leg = &reading.legs[&unit.reference];
        self.action(json!({"operation":"request-unit-decision","request":{
            "humanRequestRef":DECISION,"decisionRef":"decision:controlled-native-receiving",
            "question":"Resume this bounded controlled native unit?",
            "whyHuman":"Controlled protocol proof only; this fixture does not represent a personal human act.",
            "basis":{"workflowUnitRef":unit.reference,"attemptRef":"attempt:native-inspect-source",
                "executionRef":leg.execution_ref,"subjectRef":unit.subject_ref,"subjectRevision":unit.basis_revision,
                "workflowSourceRef":self.workflow.source.reference,"workflowSourceRevision":self.workflow.source.revision,
                "workflowSourceDigest":self.workflow.source.digest,"resolverRef":HUMAN_REF,"controlled":true},
            "evidenceRefs":["evidence:controlled-native-decision-contract"]}}));
    }

    fn submit_decision(&self) -> Value {
        let state: Value = serde_json::from_slice(&fs::read(&self.state).unwrap()).unwrap();
        let mut request = state["state"]["attemptStates"][RUN]["actionReceipts"]
            .as_object()
            .unwrap()
            .values()
            .find(|saved| saved["request"]["operation"]["operation"] == "request-unit-decision")
            .unwrap()["request"]
            .clone();
        request["expectedRevision"] = json!(self.reading().revision);
        value(self.factory(
            &[
                "attempt",
                "decision",
                "submit",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&request),
            Some(AGENT),
        ))
    }

    fn review(
        &self,
        received: &Value,
        disposition: &str,
        answer: Option<&str>,
        token: &str,
    ) -> Value {
        let mut input = json!({"return_ref":received["return_ref"],"expected_return_revision":received["revision"],"disposition":disposition});
        if let Some(answer) = answer {
            input["answer"] = json!(answer);
        }
        ctrl_call(
            &self.ctrl,
            &self.root,
            "central.receiving.review",
            &input,
            Some(token),
        )
    }

    fn resolve(&self, received: &Value) -> Value {
        let revision = received["revision"].as_str().unwrap();
        let output = self.factory(
            &[
                "attempt",
                "decision",
                "read",
                self.state.to_str().unwrap(),
                RUN,
                DECISION,
                received["return_ref"].as_str().unwrap(),
                "--json",
            ],
            None,
            None,
        );
        let mut response = if output.status.success() {
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["decisionResponse"].clone()
        } else {
            Value::Null
        };
        if response.is_null() {
            response = json!({
            "responseRef":format!("response:native:{}",blake3::hash(revision.as_bytes()).to_hex()),
            "resolverRef":HUMAN_REF,"channelReceiptRef":received["return_ref"],"sourceRevision":revision,
            "outcome":"resume","evidenceRefs":[received["return_ref"]],"controlled":true});
        }
        // A stale selection deliberately keeps its observed channel revision,
        // even when a subsequent native read supplies a newer exact response.
        response["sourceRevision"] = json!(revision);
        json!({"operation":"resolve-unit-decision","human_request_ref":DECISION,"response":response})
    }

    /// Every retained dispatch/result below comes from an actual Central process.
    /// These are controlled owner-operation legs, not model-backed Agent workers.
    fn native_contribution(&self, key: &str) -> String {
        let attempt = self.start(key);
        self.complete_native_contribution(key, &attempt, key);
        attempt
    }

    fn complete_native_contribution(&self, key: &str, attempt: &str, effect: &str) -> Value {
        let unit = self.workflow.unit(key).unwrap();
        let reading = self.reading();
        let current = reading
            .attempts
            .iter()
            .find(|record| record.attempt_ref == attempt)
            .unwrap();
        let token = if key == "inspect-source" {
            AGENT
        } else {
            REVIEWER
        };
        let actual_input = json!({"producer_key":format!("native-effect:{effect}"),
            "source_ref":self.document["source"]["ref"],"document_id":self.document["document_id"],
            "expected_source_revision":self.document["revision"]["revision"],
            "now_ref":self.now["now_ref"],"run_ref":RUN,"task_ref":current.task_ref,
            "session_ref":current.disposition.body.agent_session_ref,
            "proposal":{"operation":"entry.add","entry_id":format!("native-effect:{effect}"),
                "contribution_id":format!("native-effect:{effect}:body"),"html":"<p>Actual bounded native owner operation.</p>"}});
        let returned = ctrl_call(
            &self.ctrl,
            &self.root,
            "central.receiving.submit",
            &actual_input,
            Some(token),
        );
        let independently_read = ctrl_call(
            &self.ctrl,
            &self.root,
            "central.receiving.read",
            &json!({"return_ref":returned["return_ref"]}),
            None,
        );
        assert_eq!(
            returned, independently_read,
            "actual owner result survives a fresh process"
        );
        let execution = format!("execution:native-{effect}");
        let evidence = json!([returned["return_ref"]]);
        self.action(json!({"operation":"bind-dispatch","attempt_ref":attempt,"execution_ref":execution,
            "receipt":{"ownerRef":"central","contract":"central.receiving-reading/v1",
                "operationRef":returned["return_ref"],"receiptRef":returned["return_ref"],"sourceRevision":returned["revision"],
                "phase":"returned","evidenceRefs":evidence,"partialEffectRefs":[],"payload":returned}}));
        self.action(json!({"operation":"record-verification","attempt_ref":attempt,
            "verification":{"verificationRef":format!("verification:native:{effect}"),"ownerRef":"factory/native-owner-readback",
                "sourceRevision":returned["revision"],"outcome":"passed","obligations":unit.verification_obligations,"evidenceRefs":evidence}}));
        let difference = format!(
            "Native Central returned and independently reopened {} on {}",
            returned["return_ref"], returned["revision"]
        );
        let artifact = format!("artifact:native:{effect}");
        self.action(json!({"operation":"return-artifact","attempt_ref":attempt,
            "artifact":{"artifactRef":artifact,"subjectRef":unit.subject_ref,"subjectRevision":unit.basis_revision,
                "producingExecutionRef":execution,"evidenceRefs":evidence,"semanticDifference":difference},
            "readable_return":{"returnRef":format!("return:native:{effect}"),"summary":difference,
                "artifactRefs":[artifact],"evidenceRefs":evidence,"receivingRef":null,"receivingSourceRevision":null,
                "archiveRefs":[],"regressionObservationRefs":[]}}));
        if key == "review-adversarially" {
            self.action(
                json!({"operation":"register-independent-review","attempt_ref":attempt,
                "review_of":[self.workflow.unit("inspect-source").unwrap().reference]}),
            );
        }
        returned
    }

    fn return_to_central(&self, attempt: &str) -> Value {
        let mut request = self.request(json!({}));
        request["contract"] = json!("factory.attempt-receiving-action/v1");
        request["requestRef"] = json!("request:native-final-receiving");
        request["attemptRef"] = json!(attempt);
        request["central"] = json!({"binary":self.ctrl,"root":self.root,
            "contractRevision":epilogos_factory::attempt_receiving::CENTRAL_CONTRACT_REVISION,"project":null});
        request["target"] = json!({"sourceRef":self.document["source"]["ref"],"documentId":self.document["document_id"],
            "sourceRevision":self.document["revision"]["revision"],"expectedAuthorityRevision":null});
        request["occurredAtUnixSeconds"] = Value::Null;
        request["recover"] = json!(false);
        request.as_object_mut().unwrap().remove("operation");
        value(self.factory(
            &[
                "attempt",
                "receiving",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&request),
            Some(REVIEWER),
        ))
    }
}

fn ctrl_call(ctrl: &Path, root: &Path, action: &str, input: &Value, token: Option<&str>) -> Value {
    let mut command = Command::new(ctrl);
    command
        .args(["--json", "--root"])
        .arg(root)
        .args(["action", "run", action])
        .arg(input.to_string())
        .env_remove("CENTRAL_NATIVE_TOKEN");
    if let Some(token) = token {
        command.env("CENTRAL_NATIVE_TOKEN", token);
    }
    let response = value(command.output().unwrap());
    assert_eq!(response["ok"], true);
    assert_eq!(response["action"], action);
    response["data"].clone()
}

fn source(closure: bool) -> WorkflowSource {
    let mut source: WorkflowSource = serde_json::from_str(include_str!(
        "../../contracts/factory/fixtures/agent-workflow-source.json"
    ))
    .unwrap();
    source.units.retain(|unit| {
        unit.key == "inspect-source" || (closure && unit.key == "review-adversarially")
    });
    source.barriers.clear();
    source.nesting.clear();
    for unit in &mut source.units {
        unit.subject_ref = format!("subject:controlled-native-{}", unit.key)
            .parse()
            .unwrap();
        unit.agent_requirements.agent_refs = vec![if unit.key == "inspect-source" {
            AGENT_REF
        } else {
            REVIEWER_REF
        }
        .into()];
        unit.agent_requirements.agency_refs =
            vec![format!("agency:controlled-native-{}", unit.key)];
        unit.agent_requirements.agent_set_refs.clear();
        unit.permitted_effects = vec!["read and submit bounded native Central receipt".into()];
        unit.verification_obligations =
            vec!["exact native Central receiving result reopens unchanged".into()];
        unit.independence_from.clear();
        unit.dependencies = if unit.key == "inspect-source" {
            vec![]
        } else {
            vec!["inspect-source".into()]
        };
        unit.inputs = unit
            .dependencies
            .iter()
            .map(|key| WorkflowInputSource {
                predecessor: key.clone(),
                receiving_context_ref: format!("context:native-{}-from-{key}", unit.key),
            })
            .collect();
    }
    source.source.digest = workflow_source_digest(&source).unwrap();
    source
}

fn disposition(run: &Run, workflow: &CompiledWorkflow, key: &str) -> SituatedExecutionDisposition {
    let unit = workflow.unit(key).unwrap();
    let agency = unit
        .agent_requirements
        .agency_refs
        .iter()
        .next()
        .unwrap()
        .clone();
    let demand = ExecutionDemand {
        project_ref: run.project_ref().to_string(),
        run_ref: run.reference().to_string(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some(agency.clone()),
        profile_ref: None,
        use_type: "controlled-native-owner-contract".into(),
        required_capabilities: unit.capability_refs.clone(),
        required_modalities: BTreeSet::from(["text".into()]),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::from(["isolated-native-Central-contract".into()]),
        independence_from: BTreeSet::new(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    };
    let selection = accept_aikit_selection(
        demand,
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:controlled-owner-contract-not-invoked".into(),
            provider_ref: "provider:controlled-owner-contract-not-invoked".into(),
            ranking_policy: "controlled-native-contract".into(),
            ranking_explanation: json!({"modelInvoked":false}),
            provenance: vec!["source:controlled-native-owner-contract".into()],
        },
        "2026-10-01T00:00:00Z",
    )
    .unwrap();
    SituatedExecutionDisposition {
        selected_inputs: vec![],
        selection,
        participant: SituatedParticipant {
            agent_ref: unit
                .agent_requirements
                .agent_refs
                .iter()
                .next()
                .unwrap()
                .clone(),
            agency_ref: agency,
            world_binding_ref: "world-binding:controlled-native-Central".into(),
            profile_ref: None,
            position_ref: None,
            source_ref: workflow.source.reference.to_string(),
            source_revision: workflow.source.revision.clone(),
            source_digest: format!("blake3:{}", workflow.source.digest),
        },
        context_refs: BTreeSet::from(["context:controlled-native-owner".into()]),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        body: ExecutionBody {
            model_ref: "model:controlled-owner-contract-not-invoked".into(),
            provider_ref: "provider:controlled-owner-contract-not-invoked".into(),
            route_ref: "route:controlled-native-contract".into(),
            harness_ref: "harness:actual-ctrl-native-process".into(),
            harness_composition_ref: "composition:controlled-native-contract".into(),
            agent_session_ref: format!("session:native-{key}"),
            session_space_ref: "session-space:controlled-native-contract".into(),
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
            wall_clock_timeout_ms: Some(30_000),
            retry_grant_ref: None,
            maximum_attempts: None,
        },
    }
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn native_human_review_is_required_and_the_exact_reply_resumes_once() {
    let world = World::new(false);
    world.start("inspect-source");
    world.request_decision();
    let submitted = world.submit_decision();
    let bytes = fs::read(&world.state).unwrap();
    let replayed = world.submit_decision();
    assert_eq!(replayed["centralResponse"], submitted["centralResponse"]);
    assert_eq!(
        fs::read(&world.state).unwrap(),
        bytes,
        "submission retry retains one native channel identity"
    );
    let pending = submitted["centralResponse"]["data"].clone();
    let agent_review = Command::new(&world.ctrl)
        .args(["--json", "--root"])
        .arg(&world.root)
        .args(["action", "run", "central.receiving.review"])
        .arg(json!({"return_ref":pending["return_ref"],"expected_return_revision":pending["revision"],
            "disposition":"answered","answer":"Resume bounded work","actor_kind":"human","author":"H"}).to_string())
        .env("CENTRAL_NATIVE_TOKEN", AGENT)
        .output().unwrap();
    assert!(
        !agent_review.status.success(),
        "public human labels cannot promote an Agent credential"
    );
    let still_pending = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.read",
        &json!({"return_ref":pending["return_ref"]}),
        None,
    );
    assert_eq!(
        still_pending, pending,
        "denied review must retain native question bytes"
    );
    world.refuse(world.resolve(&pending));
    let acknowledged = world.review(&pending, "acknowledged", None, HUMAN);
    world.refuse(world.resolve(&acknowledged));
    let foreign = world.review(
        &acknowledged,
        "answered",
        Some("Resume bounded work"),
        OTHER_HUMAN,
    );
    world.refuse(world.resolve(&foreign));
    let ambiguous = world.review(
        &foreign,
        "answered",
        Some("Maybe, without choosing either option"),
        HUMAN,
    );
    world.refuse(world.resolve(&ambiguous));
    let accepted = world.review(&ambiguous, "answered", Some("Resume bounded work"), HUMAN);
    world.refuse(world.resolve(&ambiguous));
    let mut request = world.request(world.resolve(&accepted));
    request["caller"] =
        json!({"callerRef":HUMAN_REF,"projectionKind":"desktop-human","lineage":[HUMAN_REF]});
    value(world.raw_action(&request));
    assert_eq!(
        world.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::Active
    );
    let bytes = fs::read(&world.state).unwrap();
    value(world.raw_action(&request));
    assert_eq!(
        fs::read(&world.state).unwrap(),
        bytes,
        "exact reply replay does not resume twice"
    );
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn public_human_strings_and_foreign_request_bindings_never_authenticate_a_reply() {
    let world = World::new(false);
    world.start("inspect-source");
    world.request_decision();
    let nonexistent = json!({"return_ref":"central:return:controlled-nonexistent","revision":"controlled-nonexistent"});
    let mut forged = world.request(world.resolve(&nonexistent));
    forged["caller"] =
        json!({"callerRef":HUMAN_REF,"projectionKind":"desktop-human","lineage":[HUMAN_REF]});
    let before = fs::read(&world.state).unwrap();
    assert!(!world.raw_action(&forged).status.success());
    assert_eq!(fs::read(&world.state).unwrap(), before);
    let foreign = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.submit",
        &json!({
        "producer_key":"controlled-foreign-question","now_ref":world.now["now_ref"],"run_ref":RUN,
        "task_ref":"task:native-inspect-source","session_ref":"session:native-inspect-source",
        "reply_to":"human-request:another-unit","evidence_refs":["evidence:foreign-source-basis"],
        "request":{"kind":"question","subject":"A different native request","options":["Resume bounded work","Cancel bounded work"]}}),
        Some(AGENT),
    );
    let answered = world.review(&foreign, "answered", Some("Resume bounded work"), HUMAN);
    world.refuse(world.resolve(&answered));
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn an_actual_native_reply_cannot_resume_a_changed_subject_candidate() {
    let world = World::new(false);
    world.start("inspect-source");
    world.request_decision();
    let submitted = world.submit_decision();
    let accepted = world.review(
        &submitted["centralResponse"]["data"],
        "answered",
        Some("Resume bounded work"),
        HUMAN,
    );
    world.action(json!({"operation":"advance-subject","subject_ref":world.workflow.unit("inspect-source").unwrap().subject_ref,
        "revision":"controlled-new-subject-candidate"}));
    world.refuse(world.resolve(&accepted));
    assert_eq!(
        world.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::WaitingHuman
    );
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn actual_native_final_receiving_can_close_and_foreign_identity_cannot() {
    let world = World::new(true);
    world.native_contribution("inspect-source");
    let final_attempt = world.native_contribution("review-adversarially");
    world.transition("finishing", None);
    let receiving = world.return_to_central(&final_attempt);
    assert_eq!(receiving["needsReconciliation"], false);
    let received = &receiving["centralResponse"]["data"];
    let closure = json!({"finalAttemptRef":final_attempt,"reviewerAttemptRefs":[final_attempt],"receivingRef":received["return_ref"]});
    let mut changed = closure.clone();
    changed["receivingRef"] = json!("central:return:foreign-final-receiving");
    world.refuse(world.transition_operation("finished", Some(changed)));
    world.transition("finished", Some(closure));
    assert_eq!(
        world.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::Finished
    );
    assert_eq!(
        world.reading().whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    let (finished, finished_build) = world.closure_readings("native-finished");
    assert_eq!(finished["completionVerified"], true);
    assert_eq!(finished_build["view"]["run"]["status"], "success");
    assert_eq!(finished_build["view"]["frontier"]["closureState"], "closed");
    world.transition("archived", None);
    let (archived, archived_build) = world.closure_readings("native-finished-archived");
    assert_eq!(
        archived["completionVerified"], true,
        "admitted completion remains distinct from archival"
    );
    assert_eq!(archived["archivedFrom"], "finished");
    assert_eq!(archived_build["view"]["run"]["status"], "success");
    assert_eq!(archived_build["view"]["frontier"]["closureState"], "closed");
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn a_native_human_rejection_of_final_receiving_prevents_closure() {
    let world = World::new(true);
    world.native_contribution("inspect-source");
    let final_attempt = world.native_contribution("review-adversarially");
    world.transition("finishing", None);
    let receiving = world.return_to_central(&final_attempt);
    assert_eq!(receiving["needsReconciliation"], false);
    let received = &receiving["centralResponse"]["data"];
    let rejected = world.review(received, "rejected", None, HUMAN);
    assert_ne!(rejected["revision"], received["revision"]);
    assert_eq!(rejected["record"]["review"]["reviewer_ref"], HUMAN_REF);
    world.refuse(world.transition_operation(
        "finished",
        Some(json!({"finalAttemptRef":final_attempt,
        "reviewerAttemptRefs":[final_attempt],"receivingRef":received["return_ref"]})),
    ));
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn an_actual_native_reply_does_not_follow_a_replaced_affected_execution() {
    let world = World::new(false);
    let original = world.start("inspect-source");
    world.request_decision();
    let submitted = world.submit_decision();
    let accepted = world.review(
        &submitted["centralResponse"]["data"],
        "answered",
        Some("Resume bounded work"),
        HUMAN,
    );
    // This is an unlaunched read-only native unit: cancellation/quiescence are
    // owner transitions and make no claim that an external process was killed.
    world.action(json!({"operation":"request-cancellation","attempt_ref":original}));
    world.action(json!({"operation":"accept-cancellation","attempt_ref":original}));
    world.action(json!({"operation":"mark-quiescent","attempt_ref":original}));
    let unit = world.workflow.unit("inspect-source").unwrap();
    let mut selected = disposition(&world.run, &world.workflow, "inspect-source");
    selected.budget.retry_grant_ref = Some("grant:native-inspect-source".into());
    selected.budget.maximum_attempts = Some(2);
    world.action(json!({"operation":"retry","attempt_ref":"attempt:native-replacement",
        "task_ref":"task:native-inspect-source","parent_journey_ref":"journey:controlled-native-receiving",
        "workflow_unit_ref":unit.reference,"grant_ref":"grant:native-inspect-source",
        "disposition":selected,"tracking":[],"reresolution":null}));
    world.refuse(world.resolve(&accepted));
    assert_eq!(
        world.reading().legs[&unit.reference].execution_ref,
        "factory-attempt:attempt:native-replacement"
    );
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn opaque_receiving_labels_cannot_close_real_native_returned_units() {
    let world = World::new(true);
    world.native_contribution("inspect-source");
    let final_attempt = world.native_contribution("review-adversarially");
    world.transition("finishing", None);
    world.action(json!({"operation":"attach-receiving","attempt_ref":final_attempt,
        "receiving_ref":"central:return:unadmitted-receiving-label","source_revision":"unadmitted-source-revision",
        "evidence_refs":["evidence:unadmitted-receiving-label"]}));
    world.refuse(world.transition_operation("finished", Some(json!({"finalAttemptRef":final_attempt,
        "reviewerAttemptRefs":[final_attempt],"receivingRef":"central:return:unadmitted-receiving-label"}))));
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn abort_then_archive_preserves_returns_without_manufacturing_verified_completion() {
    let world = World::new(true);
    world.native_contribution("inspect-source");
    world.native_contribution("review-adversarially");
    assert_eq!(
        world.reading().whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    world.transition("aborted", None);
    world.transition("archived", None);
    let (archived, archived_build) = world.closure_readings("native-aborted-archived");
    assert_eq!(archived["lifecycle"], "archived");
    assert_eq!(archived["wholeRunState"], "complete");
    assert_eq!(
        archived["completionVerified"], false,
        "historical returned rows cannot establish completed undertaking"
    );
    assert_eq!(
        archived["currentReturnedUnits"].as_array().unwrap().len(),
        2
    );
    assert_eq!(archived["archivedFrom"], "aborted");
    assert_eq!(archived_build["view"]["run"]["status"], "fail");
    assert_eq!(
        archived_build["view"]["frontier"]["closureState"],
        "aborted"
    );
}

#[test]
#[ignore = "requires actual pinned ctrl: FACTORY_TEST_CTRL; no provider-double fallback"]
fn genuine_failed_return_retries_and_latest_current_assessment_preserves_its_basis() {
    let world = World::new(false);
    let unit = world.workflow.unit("inspect-source").unwrap();
    let original = world.start("inspect-source");
    let original_received =
        world.complete_native_contribution("inspect-source", &original, "inspect-source");
    let rejected = world.review(&original_received, "rejected", None, HUMAN);
    assert_eq!(rejected["record"]["review"]["reviewer_ref"], HUMAN_REF);
    world.action(
        json!({"operation":"record-verification","attempt_ref":original,
        "verification":{"verificationRef":"verification:native-rejected-original",
            "ownerRef":"factory/native-reviewed-result","sourceRevision":rejected["revision"],
            "outcome":"failed","obligations":unit.verification_obligations,
            "evidenceRefs":[rejected["return_ref"]]}}),
    );
    let (failed, _) = world.closure_readings("native-old-attempt-failed");
    assert_eq!(
        failed["legs"][unit.reference.to_string()]["status"],
        "failed"
    );

    // The genuine finite Ctrl subprocess exited and a fresh native read
    // reopened its result before the Return. Failed→Retry is the legal owner
    // path; this makes no provider cancellation or Failed→Quiescent claim.
    let prior = world.reading();
    let record = prior
        .attempts
        .iter()
        .find(|record| record.attempt_ref == original)
        .unwrap();
    let mut evidence = BTreeSet::new();
    for receipt in record.dispatch.iter().chain(record.observations.iter()) {
        evidence.extend(receipt.partial_effect_refs.iter().cloned());
    }
    for verification in &record.verifications {
        evidence.insert(verification.verification_ref.clone());
        evidence.extend(verification.evidence_refs.iter().cloned());
    }
    for artifact in &prior.legs[&unit.reference].artifacts {
        evidence.insert(artifact.artifact_ref.clone());
        evidence.extend(artifact.evidence_refs.iter().cloned());
    }
    let mut selected = disposition(&world.run, &world.workflow, "inspect-source");
    selected.budget.retry_grant_ref = Some("grant:native-inspect-source".into());
    selected.budget.maximum_attempts = Some(2);
    selected.body.agent_session_ref = "session:native-inspect-source-retry".into();
    let resolution = ReresolutionRecord {
        resolution_ref: "resolution:native-rejected-original".into(),
        reason: "Re-resolve the actual rejected native Central result on the same current source"
            .into(),
        source_revision: selected.participant.source_revision.clone(),
        evidence_refs: evidence,
        replacement_now_ref: None,
        replacement_material_ref: None,
        replacement_harness_ref: None,
    };
    let replacement = "attempt:native-inspect-source-retry";
    world.action(json!({"operation":"retry","attempt_ref":replacement,
        "task_ref":record.task_ref,"parent_journey_ref":"journey:controlled-native-receiving",
        "workflow_unit_ref":unit.reference,"grant_ref":"grant:native-inspect-source",
        "disposition":selected,"tracking":[],"reresolution":resolution}));
    let actual =
        world.complete_native_contribution("inspect-source", replacement, "inspect-source-retry");
    let (passed, _) = world.closure_readings("native-current-retry-passed");
    assert_eq!(
        passed["legs"][unit.reference.to_string()]["executionRef"],
        "execution:native-inspect-source-retry"
    );
    assert_eq!(passed["wholeRunState"], "complete");
    assert_eq!(passed["completionVerified"], false);
    assert_eq!(passed["attempts"].as_array().unwrap().len(), 2);

    for outcome in ["unknown", "failed"] {
        // Explicitly controlled negative assessments over a genuine current
        // native effect; no successful provider receipt is generated here.
        let current = ctrl_call(
            &world.ctrl,
            &world.root,
            "central.receiving.read",
            &json!({"return_ref":actual["return_ref"]}),
            None,
        );
        assert_eq!(current, actual);
        world.action(json!({"operation":"record-verification","attempt_ref":replacement,
            "verification":{"verificationRef":format!("verification:native-current-{outcome}"),
                "ownerRef":"factory/controlled-current-assessment","sourceRevision":current["revision"],
                "outcome":outcome,"obligations":unit.verification_obligations,
                "evidenceRefs":[current["return_ref"]]}}));
        let (changed, _) = world.closure_readings(&format!("native-current-retry-{outcome}"));
        assert_eq!(
            changed["legs"][unit.reference.to_string()]["status"],
            "failed"
        );
        assert_eq!(changed["wholeRunState"], "failed");
        let retained = changed["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["attemptRef"] == replacement)
            .unwrap();
        assert_eq!(
            retained["verifications"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["outcome"],
            outcome
        );
        assert!(retained["readableReturn"].is_object());
    }
}
