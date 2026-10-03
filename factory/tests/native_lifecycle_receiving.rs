//! Real Factory and pinned Central processes over an isolated native World.
//! The bounded principals below are controlled authority fixtures, not a claim
//! that a person answered in the installed personal World. No provider double,
//! generated owner response, model result or private credential is used.
//! New transport-fault scripts only exec the hash-pinned real Ctrl; they never
//! implement a provider, emit JSON or reconstruct an owner identity.

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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

trait FiniteNativeCommand {
    fn output_finite(&mut self) -> std::io::Result<Output>;
}
impl FiniteNativeCommand for Command {
    fn output_finite(&mut self) -> std::io::Result<Output> {
        epilogos_factory::native_process::output(self, std::time::Duration::from_secs(60))
    }
}

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
    let output = Command::new(&path)
        .arg("--version")
        .output_finite()
        .unwrap();
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

fn factory_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("FACTORY_NATIVE_FACTORY_BINARY") {
        let path = PathBuf::from(path);
        assert!(path.is_absolute());
        let path = path.canonicalize().unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(
            bytes.starts_with(b"\x7fELF")
                || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
                || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
        );
        let expected = std::env::var("FACTORY_NATIVE_FACTORY_SHA256")
            .expect("an explicit actual Factory executable requires its exact SHA256");
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected);
        path
    } else {
        PathBuf::from(env!("CARGO_BIN_EXE_factory"))
    }
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
    retained: bool,
}

impl World {
    fn new(closure: bool) -> Self {
        Self::from_directory(closure, tempfile::tempdir().unwrap(), false)
    }

    fn new_retained(closure: bool) -> Self {
        let ctrl_pin = std::env::var("FACTORY_TEST_CTRL_SHA256")
            .expect("native cancellation requires the actual qualified Central binary SHA256");
        assert!(ctrl_pin.len() == 64 && ctrl_pin.bytes().all(|b| b.is_ascii_hexdigit()));
        let base =
            PathBuf::from(std::env::var_os("FACTORY_NATIVE_EVIDENCE_DIR").expect(
                "native cancellation regression requires an admitted absolute evidence root",
            ));
        assert!(base.is_absolute());
        let metadata = fs::symlink_metadata(&base).unwrap();
        assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
        assert_eq!(base.canonicalize().unwrap(), base);
        let mut dir = tempfile::Builder::new()
            .prefix("native-cancellation-")
            .tempdir_in(&base)
            .unwrap();
        dir.disable_cleanup(true);
        fs::create_dir(dir.path().join("native-operations")).unwrap();
        Self::from_directory(closure, dir, true)
    }

    fn from_directory(closure: bool, dir: TempDir, retained: bool) -> Self {
        let root = dir.path().canonicalize().unwrap();
        let ctrl = native_ctrl();
        let mut init_command = Command::new(&ctrl);
        init_command
            .args(["--json", "--root"])
            .arg(&root)
            .arg("init")
            .env_remove("CENTRAL_NATIVE_TOKEN");
        let init = if retained {
            epilogos_factory::native_process::output(&mut init_command, Duration::from_secs(10))
                .unwrap()
        } else {
            init_command.output_finite().unwrap()
        };
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
            retained,
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
        self.factory_at_owner(args, input, token, &self.ctrl, &self.root)
    }

    fn factory_at_owner(
        &self,
        args: &[&str],
        input: Option<&Value>,
        token: Option<&str>,
        binary: &Path,
        root: &Path,
    ) -> Output {
        let mut command = Command::new(factory_binary());
        if self.retained {
            return self.retained_factory(command, args, input, token);
        }
        command
            .args(args)
            .env("FACTORY_NATIVE_CENTRAL_BINARY", binary)
            .env("FACTORY_NATIVE_CENTRAL_ROOT", root)
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
        // File-input is the same public CLI contract, allowing shared native
        // capture to own lifetime/output rather than an unbounded wait.
        static REQUEST_SEQUENCE: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        if let Some(input) = input {
            let sequence = REQUEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = self.root.join(format!(
                "factory-native-request-{}-{sequence}.json",
                std::process::id()
            ));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.write_all(input.to_string().as_bytes()).unwrap();
            file.sync_all().unwrap();
            let rewritten = args
                .iter()
                .map(|arg| {
                    if *arg == "-" {
                        path.to_str().unwrap()
                    } else {
                        *arg
                    }
                })
                .collect::<Vec<_>>();
            command = Command::new(factory_binary());
            command
                .args(rewritten)
                .env("FACTORY_NATIVE_CENTRAL_BINARY", binary)
                .env("FACTORY_NATIVE_CENTRAL_ROOT", root)
                .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
                .env_remove("CENTRAL_NATIVE_TOKEN");
            if let Some(token) = token {
                command.env("CENTRAL_NATIVE_TOKEN", token);
            }
        }
        epilogos_factory::native_process::output(&mut command, std::time::Duration::from_secs(60))
            .unwrap()
    }

    fn retained_factory(
        &self,
        mut command: Command,
        args: &[&str],
        input: Option<&Value>,
        token: Option<&str>,
    ) -> Output {
        use std::sync::atomic::{AtomicU64, Ordering};
        static OPERATION: AtomicU64 = AtomicU64::new(0);
        let directory = self.root.join("native-operations").join(format!(
            "factory-{}",
            OPERATION.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input_path = directory.join("input.json");
        if let Some(input) = input {
            fs::write(&input_path, serde_json::to_vec_pretty(input).unwrap()).unwrap();
        }
        let arguments = args
            .iter()
            .map(|argument| {
                if input.is_some() && *argument == "-" {
                    input_path.to_str().unwrap().to_owned()
                } else {
                    (*argument).to_owned()
                }
            })
            .collect::<Vec<_>>();
        command
            .args(&arguments)
            .env("FACTORY_NATIVE_CENTRAL_BINARY", &self.ctrl)
            .env("FACTORY_NATIVE_CENTRAL_ROOT", &self.root)
            .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
            .env_remove("CENTRAL_NATIVE_TOKEN");
        if let Some(token) = token {
            command.env("CENTRAL_NATIVE_TOKEN", token);
        }
        fs::write(
            directory.join("invocation.json"),
            serde_json::to_vec_pretty(&json!({
                "argv": std::iter::once(command.get_program().to_string_lossy().into_owned())
                    .chain(arguments).collect::<Vec<_>>(),
                "timeoutMs":10000,"ownedFixture":self.root,"personalCredentialSupplied":false
            }))
            .unwrap(),
        )
        .unwrap();
        match epilogos_factory::native_process::output(&mut command, Duration::from_secs(10)) {
            Ok(output) => {
                fs::write(directory.join("stdout"), &output.stdout).unwrap();
                fs::write(directory.join("stderr"), &output.stderr).unwrap();
                fs::write(
                    directory.join("result.json"),
                    serde_json::to_vec_pretty(&json!({
                        "actualExitCode":output.status.code(),
                        "stdoutSha256":format!("{:x}",Sha256::digest(&output.stdout)),
                        "stderrSha256":format!("{:x}",Sha256::digest(&output.stderr))
                    }))
                    .unwrap(),
                )
                .unwrap();
                output
            }
            Err(error) => {
                let observation = epilogos_factory::native_process::capture_failure(&error).map(|failure| {
                    fs::write(directory.join("stdout"), failure.stdout()).unwrap();
                    fs::write(directory.join("stderr"), failure.stderr()).unwrap();
                    json!({
                        "processStarted":failure.process_started(),
                        "statusObserved":failure.status().is_some(),
                        "actualExitCode":failure.status().and_then(|status| status.code()),
                        "clientReaped":failure.client_reaped(),
                        "stdoutEof":failure.stdout_eof(),"stderrEof":failure.stderr_eof(),
                        "stdoutTruncated":failure.stdout_truncated(),"stderrTruncated":failure.stderr_truncated(),
                        "timedOut":failure.timed_out(),"ownershipLost":failure.ownership_lost(),
                        "stopAttempted":failure.stop_attempted(),"stopRequested":failure.stop_requested(),
                        "stdoutSha256":format!("{:x}",Sha256::digest(failure.stdout())),
                        "stderrSha256":format!("{:x}",Sha256::digest(failure.stderr())),
                        "primaryCause":{"kind":format!("{:?}",failure.cause().kind()),
                            "rawOsError":failure.cause().raw_os_error(),"message":failure.cause().to_string()},
                        "cleanupCauses":failure.cleanup_errors().iter().map(|cause| json!({
                            "kind":format!("{:?}",cause.kind()),"rawOsError":cause.raw_os_error(),
                            "message":cause.to_string()})).collect::<Vec<_>>()
                    })
                });
                fs::write(directory.join("capture-failure.json"), serde_json::to_vec_pretty(&json!({
                    "kind":format!("{:?}",error.kind()),"rawOsError":error.raw_os_error(),
                    "cause":error.to_string(),"actualObservation":observation,"nativeQualification":false
                })).unwrap()).unwrap();
                panic!(
                    "native Factory capture failed; retained {}: {error}",
                    directory.display()
                );
            }
        }
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
            let factory = factory_binary().canonicalize().unwrap();
            let measure = |binary: &Path| {
                let version = Command::new(binary)
                    .arg("--version")
                    .output_finite()
                    .unwrap();
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

    fn refuse_with_reason(&self, operation: Value, expected: &[&str]) {
        let before = fs::read(&self.state).unwrap();
        let output = self.raw_action(&self.request(operation));
        assert!(!output.status.success());
        let diagnostic = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for exact_cause in expected {
            assert!(
                diagnostic.contains(exact_cause),
                "wrong actual refusal; expected {exact_cause}: {diagnostic}"
            );
        }
        assert_eq!(
            fs::read(&self.state).unwrap(),
            before,
            "a refused transition must preserve the actual owner state bytes"
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
        let (returned, operation) = self.prepare_native_contribution(key, attempt, effect);
        self.action(operation);
        if key == "review-adversarially" {
            self.action(
                json!({"operation":"register-independent-review","attempt_ref":attempt,
                "review_of":[self.workflow.unit("inspect-source").unwrap().reference]}),
            );
        }
        returned
    }

    fn prepare_native_contribution(
        &self,
        key: &str,
        attempt: &str,
        effect: &str,
    ) -> (Value, Value) {
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
        let operation = json!({"operation":"return-artifact","attempt_ref":attempt,
            "artifact":{"artifactRef":artifact,"subjectRef":unit.subject_ref,"subjectRevision":unit.basis_revision,
                "producingExecutionRef":execution,"evidenceRefs":evidence,"semanticDifference":difference},
            "readable_return":{"returnRef":format!("return:native:{effect}"),"summary":difference,
                "artifactRefs":[artifact],"evidenceRefs":evidence,"receivingRef":null,"receivingSourceRevision":null,
                "archiveRefs":[],"regressionObservationRefs":[]}});
        (returned, operation)
    }

    fn receiving_request(&self, attempt: &str) -> Value {
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
        request
    }
    fn return_to_central(&self, attempt: &str) -> Value {
        let request = self.receiving_request(attempt);
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
    let output = if root.join("native-operations").is_dir() {
        epilogos_factory::native_process::output(&mut command, Duration::from_secs(10)).unwrap()
    } else {
        command.output_finite().unwrap()
    };
    if root.join("native-operations").is_dir() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static OPERATION: AtomicU64 = AtomicU64::new(0);
        let directory = root.join("native-operations").join(format!(
            "central-{}",
            OPERATION.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("stdout"), &output.stdout).unwrap();
        fs::write(directory.join("stderr"), &output.stderr).unwrap();
        fs::write(
            directory.join("invocation.json"),
            serde_json::to_vec_pretty(&json!({
                "argv":[ctrl.to_str().unwrap(),"--json","--root",root.to_str().unwrap(),
                    "action","run",action,input.to_string()],
                "actualExitCode":output.status.code(),"timeoutMs":10000,
                "stdoutSha256":format!("{:x}",Sha256::digest(&output.stdout)),
                "stderrSha256":format!("{:x}",Sha256::digest(&output.stderr)),
                "personalCredentialSupplied":false
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let response = value(output);
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
    assert_eq!(
        replayed["centralResponse"]["data"]["record"],
        submitted["centralResponse"]["data"]["record"]
    );
    assert_eq!(
        replayed["centralResponse"]["action"],
        "central.receiving.read"
    );
    assert_eq!(
        replayed["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_ne!(
        fs::read(&world.state).unwrap(),
        bytes,
        "new live lookup evidence is retained, not rewritten submit bytes"
    );
    let pending = submitted["centralResponse"]["data"].clone();
    let agent_review = Command::new(&world.ctrl)
        .args(["--json", "--root"])
        .arg(&world.root)
        .args(["action", "run", "central.receiving.review"])
        .arg(json!({"return_ref":pending["return_ref"],"expected_return_revision":pending["revision"],
            "disposition":"answered","answer":"Resume bounded work","actor_kind":"human","author":"H"}).to_string())
        .env("CENTRAL_NATIVE_TOKEN", AGENT)
        .output_finite().unwrap();
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

// Real native publication/reconciliation gates: no owner-response fixture.
fn final_native_world() -> (World, String) {
    let expected = std::env::var("FACTORY_TEST_CTRL_SHA256")
        .expect("new recovery gates require exact qualified native Central pin");
    let selected = native_ctrl();
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&selected).unwrap())),
        expected
    );
    let world = World::new(true);
    world.native_contribution("inspect-source");
    let attempt = world.native_contribution("review-adversarially");
    (world, attempt)
}
fn receiving_output(world: &World, request: &Value) -> Output {
    world.factory(
        &[
            "attempt",
            "receiving",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ],
        Some(request),
        Some(REVIEWER),
    )
}
fn ledger_bytes(world: &World) -> std::collections::BTreeMap<String, Vec<u8>> {
    fs::read_dir(world.root.join(".central/source-returns/contributions"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}
#[cfg(unix)]
fn real_ctrl_fault(world: &World, name: &str, close_stdout: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    // Source-retained transparent OS boundary, not a native provider double.
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let script = world.root.join(name);
    let calls = world.root.join(format!("{name}.calls"));
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$6\" >> {}\n{}exec {} \"$@\"\n",
        quote(&calls),
        if close_stdout { "exec 1>&-\n" } else { "" },
        quote(&world.ctrl)
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    script
}
#[test]
#[ignore = "requires genuine current lookup Ctrl; no provider fallback"]
fn native_receiving_guarded_recovery_preserves_actual_owner_bytes_and_rejects_changed_input() {
    let (world, attempt) = final_native_world();
    let mut request = world.receiving_request(&attempt);
    let first = value(receiving_output(&world, &request));
    let before = ledger_bytes(&world);
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    let recovered = value(receiving_output(&world, &request));
    assert_eq!(
        recovered["centralResponse"]["action"],
        "central.receiving.read"
    );
    assert_eq!(
        recovered["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_eq!(
        first["centralResponse"]["data"]["record"],
        recovered["centralResponse"]["data"]["record"]
    );
    assert_eq!(
        before,
        ledger_bytes(&world),
        "lookup cannot publish a second record or cursor"
    );
    request["occurredAtUnixSeconds"] = json!(1);
    request["expectedRevision"] = json!(world.reading().revision);
    let state = fs::read(&world.state).unwrap();
    let refused = receiving_output(&world, &request);
    assert!(!refused.status.success());
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(before, ledger_bytes(&world));
    assert_eq!(
        world.ctrl,
        native_ctrl(),
        "actual selected native binary pin rechecked"
    );
}
#[test]
#[ignore = "requires genuine current Ctrl plus explicit FACTORY_TEST_CTRL_PRE_LOOKUP and SHA256"]
fn native_receiving_owner_replacement_uses_new_binary_only_for_original_lookup() {
    let (world, attempt) = final_native_world();
    let old = PathBuf::from(
        std::env::var_os("FACTORY_TEST_CTRL_PRE_LOOKUP")
            .expect("actual previous native Ctrl required"),
    )
    .canonicalize()
    .unwrap();
    let oldbytes = fs::read(&old).unwrap();
    assert!(
        oldbytes.starts_with(b"\x7fELF")
            || oldbytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || oldbytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&oldbytes)),
        std::env::var("FACTORY_TEST_CTRL_PRE_LOOKUP_SHA256").expect("actual old pin required")
    );
    assert_ne!(old, world.ctrl);
    assert_ne!(
        oldbytes,
        fs::read(&world.ctrl).unwrap(),
        "distinct physical qualified native images required"
    );
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(old);
    let first = value(receiving_output(&world, &request));
    let before = ledger_bytes(&world);
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    let unavailable = value(receiving_output(&world, &request));
    assert_eq!(unavailable["needsReconciliation"], true);
    assert!(
        unavailable["centralResponse"].is_null(),
        "old descriptor must actually refuse lookup capability"
    );
    assert_eq!(before, ledger_bytes(&world));
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(world.ctrl);
    request["expectedRevision"] = json!(world.reading().revision);
    let recovered = value(receiving_output(&world, &request));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(
        recovered["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_eq!(
        first["centralResponse"]["data"]["record"],
        recovered["centralResponse"]["data"]["record"]
    );
    assert_eq!(before, ledger_bytes(&world));
    assert_eq!(oldbytes, fs::read(old).unwrap());
    assert_eq!(world.ctrl, native_ctrl());
}
#[cfg(unix)]
#[test]
#[ignore = "requires actual Ctrl; transparent close-stdout boundary execs real pinned owner only"]
fn native_receiving_lost_ack_recovers_without_a_second_native_submit() {
    let (world, attempt) = final_native_world();
    let fault = real_ctrl_fault(&world, "real-ctrl-close-stdout", true);
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(fault);
    let unresolved = value(receiving_output(&world, &request));
    assert_eq!(unresolved["needsReconciliation"], true);
    let before = ledger_bytes(&world);
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(world.ctrl);
    let recovered = value(receiving_output(&world, &request));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(
        recovered["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_eq!(before, ledger_bytes(&world));
    assert_eq!(
        fs::read_to_string(world.root.join("real-ctrl-close-stdout.calls"))
            .unwrap()
            .lines()
            .filter(|line| *line == "central.receiving.submit")
            .count(),
        1
    );
    assert_eq!(world.ctrl, native_ctrl());
}
#[cfg(unix)]
#[test]
#[ignore = "requires actual Ctrl; real concurrent Factory clients and transparent native invocation count"]
fn native_receiving_aliases_have_one_canonical_submit_winner_across_restart() {
    let (world, attempt) = final_native_world();
    let fault = real_ctrl_fault(&world, "real-ctrl-count", false);
    let mut one = world.receiving_request(&attempt);
    one["central"]["binary"] = json!(fault);
    let mut two = one.clone();
    two["requestRef"] = json!("request:native-alias");
    two["projectionRef"] = json!("projection:native-alias");
    let outputs = std::thread::scope(|scope| {
        let a = scope.spawn(|| receiving_output(&world, &one));
        let b = scope.spawn(|| receiving_output(&world, &two));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(outputs.0.status.success() || outputs.1.status.success());
    assert_eq!(
        fs::read_to_string(world.root.join("real-ctrl-count.calls"))
            .unwrap()
            .lines()
            .filter(|line| *line == "central.receiving.submit")
            .count(),
        1,
        "same native key is fenced beyond Action digest"
    );
    let before = ledger_bytes(&world);
    two["recover"] = json!(true);
    two["expectedRevision"] = json!(world.reading().revision);
    let resumed = value(receiving_output(&world, &two));
    assert_eq!(resumed["needsReconciliation"], false);
    assert_eq!(before, ledger_bytes(&world));
    assert_eq!(world.ctrl, native_ctrl());
}
#[test]
#[ignore = "requires actual Ctrl and Factory owner; public JSON cannot mint private delivery admission"]
fn native_receiving_reserved_call_json_is_refused_without_owner_effects() {
    let (world, attempt) = final_native_world();
    let state = fs::read(&world.state).unwrap();
    let ledger = ledger_bytes(&world);
    for contract in [
        "factory.attempt-receiving-call/v1",
        "factory.attempt-unit-decision-call/v1",
    ] {
        let request=world.request(json!({"operation":"record-observation","attempt_ref":attempt,
            "receipt":{"ownerRef":"factory","contract":contract,"operationRef":"forged:delivery","receiptRef":"forged:receipt",
                "sourceRevision":"forged:basis","phase":"observed","evidenceRefs":[],"partialEffectRefs":[],"payload":{}}}));
        assert!(!world.raw_action(&request).status.success());
        assert_eq!(state, fs::read(&world.state).unwrap());
        assert_eq!(ledger, ledger_bytes(&world));
    }
}

#[test]
#[ignore = "requires actual Ctrl; real missing executable and actual guarded native absence"]
fn native_receiving_intent_before_failed_launch_never_submits_during_recovery() {
    let (world, attempt) = final_native_world();
    let before = ledger_bytes(&world);
    let mut request = world.receiving_request(&attempt);
    let missing = world.root.join("actual-missing-native-ctrl");
    assert!(!missing.exists());
    request["central"]["binary"] = json!(missing);
    let failed = value(receiving_output(&world, &request));
    assert_eq!(failed["needsReconciliation"], true);
    assert_eq!(
        failed["transportObservation"]["payload"]["detail"]["nativeCapture"]["errorKind"],
        "NotFound"
    );
    assert_eq!(before, ledger_bytes(&world));
    request["recover"] = json!(true);
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(world.ctrl);
    request["expectedRevision"] = json!(world.reading().revision);
    let unresolved = value(receiving_output(&world, &request));
    assert_eq!(unresolved["needsReconciliation"], true);
    let actual = unresolved["centralResponse"]
        .as_object()
        .expect("native owner absence response retained");
    assert_eq!(actual["action"], "central.receiving.read");
    assert_eq!(actual["ok"], false);
    assert_eq!(
        before,
        ledger_bytes(&world),
        "missing receipt never permits implicit submit"
    );
    // A same-owner replacement cannot silently switch to another root.
    request["lookupEndpoint"]["root"] = json!(world.root.join("Work/native-receiving"));
    request["expectedRevision"] = json!(world.reading().revision);
    let wrong = value(receiving_output(&world, &request));
    assert_eq!(wrong["needsReconciliation"], true);
    assert_eq!(before, ledger_bytes(&world));
    assert_eq!(world.ctrl, native_ctrl());
}

fn optional_ledger_bytes(world: &World) -> Option<std::collections::BTreeMap<String, Vec<u8>>> {
    match fs::symlink_metadata(world.root.join(".central/source-returns/contributions")) {
        Ok(metadata) => {
            assert!(metadata.is_dir());
            Some(ledger_bytes(world))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("actual native ledger observation failed: {error}"),
    }
}
#[cfg(unix)]
fn native_shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
#[cfg(unix)]
fn native_script(world: &World, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = world.root.join(name);
    fs::write(&script, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    script
}
#[cfg(unix)]
#[test]
#[ignore = "requires actual qualified current/previous native Ctrl; physical selector replacement"]
fn native_receiving_client_replacement_after_actual_descriptor_refuses_before_read() {
    let (world, attempt) = final_native_world();
    let mut request = world.receiving_request(&attempt);
    let original = value(receiving_output(&world, &request));
    assert_eq!(original["needsReconciliation"], false);
    let ledger = ledger_bytes(&world);
    let old = PathBuf::from(
        std::env::var_os("FACTORY_TEST_CTRL_PRE_LOOKUP").expect("actual previous Ctrl required"),
    )
    .canonicalize()
    .unwrap();
    let old_bytes = fs::read(&old).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&old_bytes)),
        std::env::var("FACTORY_TEST_CTRL_PRE_LOOKUP_SHA256")
            .expect("exact old binary pin required")
    );
    assert!(
        old_bytes.starts_with(b"\x7fELF")
            || old_bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || old_bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
    );
    assert_ne!(old_bytes, fs::read(&world.ctrl).unwrap());
    let selector = world.root.join("descriptor-then-real-client-replacement");
    let calls = world.root.join("client-selector.calls");
    let replacement = world.root.join("real-native-client-replacement");
    fs::copy(&old, &replacement).unwrap();
    let body=format!("printf '%s\\n' \"$6\" >> {}\n{} \"$@\"\nstatus=$?\nif [ \"$6\" = 'action.describe' ]; then /bin/mv {} {}; fi\nexit \"$status\"\n",
        native_shell_quote(&calls),native_shell_quote(&world.ctrl),native_shell_quote(&replacement),native_shell_quote(&selector));
    assert_eq!(
        native_script(&world, "descriptor-then-real-client-replacement", &body),
        selector
    );
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(selector);
    let refused = value(receiving_output(&world, &request));
    assert_eq!(refused["needsReconciliation"], true);
    let capture = &refused["transportObservation"]["payload"]["detail"]["nativeCapture"];
    assert_eq!(capture["stage"], "afterNativeCall");
    assert_eq!(capture["centralResponse"]["action"], "action.describe");
    assert_eq!(capture["centralResponse"]["ok"], true);
    assert!(capture["qualificationFailure"]
        .as_str()
        .unwrap()
        .contains("client identity"));
    assert_eq!(
        fs::read_to_string(calls)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        vec!["action.describe"]
    );
    assert_eq!(ledger, ledger_bytes(&world));
    assert_eq!(old_bytes, fs::read(selector).unwrap());
    assert_eq!(old_bytes, fs::read(old).unwrap());
    assert_eq!(world.ctrl, native_ctrl());
}

#[cfg(unix)]
#[test]
#[ignore = "requires two genuine initialized native Roots and current Ctrl; original locator retarget"]
fn native_receiving_same_original_root_locator_cannot_claim_a_replacement_owner() {
    use std::os::unix::fs::symlink;
    let (world, attempt) = final_native_world();
    let other = World::new(true);
    let locator = world.root.join("original-native-owner-locator");
    symlink(&world.root, &locator).unwrap();
    let mut request = world.receiving_request(&attempt);
    request["central"]["root"] = json!(locator);
    let first = value(receiving_output(&world, &request));
    assert_eq!(first["needsReconciliation"], false);
    let before_a = ledger_bytes(&world);
    let before_b = optional_ledger_bytes(&other);
    let state = fs::read(&world.state).unwrap();
    let retained = world
        .reading()
        .attempts
        .into_iter()
        .find(|record| record.attempt_ref == attempt)
        .unwrap()
        .observations;
    let original = retained
        .iter()
        .find(|receipt| {
            receipt.contract == "factory.attempt-receiving-call/v1"
                && receipt.phase
                    == epilogos_factory::attempt_runtime::OwnerOperationPhase::Dispatching
        })
        .unwrap();
    assert_eq!(original.payload["hostEndpoint"]["root"], json!(locator));
    assert_eq!(
        original.payload["ownerClaim"]["canonicalRoot"],
        json!(world.root)
    );
    assert!(original.payload["ownerClaim"]["rootIdentity"]["inode"]
        .as_u64()
        .is_some());
    fs::remove_file(&locator).unwrap();
    symlink(&other.root, &locator).unwrap();
    // Fresh alias and current CAS must not fall through as a new first sender.
    request["requestRef"] = json!("request:native-root-retarget");
    request["projectionRef"] = json!("projection:native-root-retarget");
    request["expectedRevision"] = json!(world.reading().revision);
    let refused = receiving_output(&world, &request);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("root physical identity changed"));
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(before_a, ledger_bytes(&world));
    assert_eq!(before_b, optional_ledger_bytes(&other));
    fs::remove_file(&locator).unwrap();
    symlink(&world.root, &locator).unwrap();
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    let recovered = value(receiving_output(&world, &request));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(
        first["centralResponse"]["data"]["record"],
        recovered["centralResponse"]["data"]["record"]
    );
    assert_eq!(before_a, ledger_bytes(&world));
    assert_eq!(before_b, optional_ledger_bytes(&other));
    assert_eq!(world.ctrl, native_ctrl());
}

#[cfg(unix)]
struct OwnedReceivingProcess {
    child: std::process::Child,
    group: i32,
    retired: bool,
    signal_forbidden: Option<String>,
}
#[cfg(unix)]
impl OwnedReceivingProcess {
    fn observe_running(&mut self) -> Result<(), String> {
        if let Some(cause) = &self.signal_forbidden {
            return Err(cause.clone());
        }
        if self.retired {
            return Err("owned coordinator already retired; no running authority remains".into());
        }
        // Record the first exit/reap or wait error before a caller can unwind.
        // Readiness and cleanup share this permanent numeric signal fence.
        let refusal = match self.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(format!("owned coordinator already exited ({status}); group cleanup remains uncertain; no signal sent")),
            Err(error) => Some(format!("owned coordinator wait authority lost ({error}); no signal sent")),
        };
        if let Some(cause) = refusal {
            self.signal_forbidden = Some(cause.clone());
            return Err(cause);
        }
        Ok(())
    }
    fn retire(&mut self) -> Result<(), String> {
        if self.retired {
            return Ok(());
        }
        self.observe_running()?;
        let actual_group = unsafe { libc::getpgid(self.child.id() as libc::pid_t) };
        if actual_group != self.group {
            let os_error = (actual_group == -1).then(|| std::io::Error::last_os_error());
            let cause = format!("owned coordinator group changed/unavailable ({actual_group}, OS {:?}); no signal sent", os_error.as_ref().and_then(std::io::Error::raw_os_error));
            self.signal_forbidden = Some(cause.clone());
            return Err(cause);
        }
        // This fixture has exclusive Child reaping. A hostile external waiter
        // racing after this finite check is not an atomically fenced faculty.
        // This exact process group was created for this isolated Factory call.
        // No ambient native service or provider is a member or targeted.
        let delivered = unsafe { libc::kill(-self.group, libc::SIGKILL) };
        if delivered != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(format!(
                "owned receiving group termination: {}",
                std::io::Error::last_os_error()
            ));
        }
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut status = None;
        while std::time::Instant::now() < until {
            if status.is_none() {
                status = match self.child.try_wait() {
                    Ok(status) => status,
                    Err(error) => {
                        let cause = format!("owned receiving reap authority lost: {error}");
                        self.signal_forbidden = Some(cause.clone());
                        return Err(cause);
                    }
                };
            }
            let absent = unsafe { libc::kill(-self.group, 0) } != 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            if let Some(status) = status {
                if absent {
                    use std::os::unix::process::ExitStatusExt;
                    self.retired = true;
                    if status.signal() != Some(libc::SIGKILL) {
                        return Err(format!("owned coordinator did not retain actual abrupt SIGKILL status: {status}"));
                    }
                    return Ok(());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err("owned receiving coordinator/group termination did not confirm reap and absence".into())
    }
}
#[cfg(unix)]
impl Drop for OwnedReceivingProcess {
    fn drop(&mut self) {
        if !self.retired {
            if let Err(secondary) = self.retire() {
                eprintln!("owned receiving cleanup uncertainty: {secondary}");
                if !std::thread::panicking() {
                    panic!("owned receiving cleanup uncertainty: {secondary}");
                }
            }
        }
    }
}
#[cfg(unix)]
#[test]
#[ignore = "requires actual Ctrl; real durable-intent/pre-native-Ctrl-exec coordinator death"]
fn native_receiving_abrupt_death_after_durable_intent_never_resubmits() {
    use std::os::unix::process::CommandExt;
    let (world, attempt) = final_native_world();
    let ledger = ledger_bytes(&world);
    let marker = world.root.join("held-pre-ctrl-exec.pid");
    let calls = world.root.join("actual-native-submit.calls");
    let release = world.root.join("release-pre-ctrl-exec");
    assert!(!release.exists());
    let body=format!("if [ \"$6\" = 'central.receiving.submit' ]; then printf '%s\\n' \"$$\" > {}; remaining=1000; while [ ! -f {} ]; do if [ \"$remaining\" -le 0 ]; then exit 75; fi; remaining=$((remaining - 1)); /bin/sleep 0.01; done; printf '%s\\n' \"$6\" >> {}; fi\nexec {} \"$@\"\n",
        native_shell_quote(&marker),native_shell_quote(&release),native_shell_quote(&calls),native_shell_quote(&world.ctrl));
    let boundary = native_script(&world, "native-pre-exec-barrier", &body);
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(boundary);
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args([
            "attempt",
            "receiving",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ])
        .env("FACTORY_NATIVE_CENTRAL_BINARY", &world.ctrl)
        .env("FACTORY_NATIVE_CENTRAL_ROOT", &world.root)
        .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
        .env("CENTRAL_NATIVE_TOKEN", REVIEWER)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(
            fs::File::create(world.root.join("abrupt-coordinator.stdout")).unwrap(),
        ))
        .stderr(Stdio::from(
            fs::File::create(world.root.join("abrupt-coordinator.stderr")).unwrap(),
        ))
        .process_group(0);
    let child = command.spawn().unwrap();
    let group = i32::try_from(child.id()).unwrap();
    let mut owned = OwnedReceivingProcess {
        child,
        group,
        retired: false,
        signal_forbidden: None,
    };
    owned
        .child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        owned.observe_running().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        marker.is_file(),
        "actual transparent boundary reached before native Ctrl exec"
    );
    let barrier_pid = fs::read_to_string(&marker)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_eq!(unsafe { libc::getpgid(barrier_pid) }, group);
    assert_ne!(barrier_pid, group);
    let reading = world.reading();
    let record = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt)
        .unwrap();
    let intent = record
        .observations
        .iter()
        .find(|receipt| {
            receipt.contract == "factory.attempt-receiving-call/v1"
                && receipt.phase
                    == epilogos_factory::attempt_runtime::OwnerOperationPhase::Dispatching
        })
        .unwrap()
        .clone();
    assert!(intent.payload["nativeRequest"].is_object());
    assert!(intent.payload["ownerClaim"]["rootIdentity"].is_object());
    assert!(!calls.exists());
    assert_eq!(ledger, ledger_bytes(&world));
    owned.retire().unwrap();
    assert!(!release.exists());
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(world.ctrl);
    let recovered = value(receiving_output(&world, &request));
    assert_eq!(recovered["needsReconciliation"], true);
    assert_eq!(
        recovered["centralResponse"]["action"],
        "central.receiving.read"
    );
    assert_eq!(recovered["centralResponse"]["ok"], false);
    assert_eq!(ledger, ledger_bytes(&world));
    assert!(
        !calls.exists(),
        "no actual native submit ever crossed the held boundary"
    );
    let resumed = world.reading();
    let record = resumed
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt)
        .unwrap();
    assert!(
        record
            .observations
            .iter()
            .any(|retained| retained == &intent),
        "original durable intent survives exact restart"
    );
    assert_eq!(world.ctrl, native_ctrl());
}

#[test]
#[ignore = "requires genuine Ctrl submit/read; same visible receipt cannot certify a different full request digest"]
fn native_receiving_byref_same_visible_result_refuses_changed_original_digest() {
    let (world, attempt) = final_native_world();
    let reading = world.reading();
    let record = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt)
        .unwrap();
    let native_ref = &record.dispatch.first().unwrap().receipt_ref;
    let native = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.read",
        &json!({"return_ref":native_ref}),
        None,
    );
    let authority = native["record"]["authority_revision"]
        .as_str()
        .expect("actual authenticated current authority basis");
    let mut request = world.receiving_request(&attempt);
    request["target"]["expectedAuthorityRevision"] = json!(authority);
    request["central"]["binary"] = json!(world.root.join("actual-missing-client-before-submit"));
    let pending = value(receiving_output(&world, &request));
    assert_eq!(pending["needsReconciliation"], true);
    let input_b = pending["transportObservation"]["payload"]["nativeRequest"].clone();
    assert_eq!(input_b["expected_authority_revision"], authority);
    let mut input_a = input_b.clone();
    input_a
        .as_object_mut()
        .unwrap()
        .remove("expected_authority_revision");
    // Both native authentication forms are genuinely admitted. The control has
    // a distinct native key; the actual A publication uses B's exact target/key.
    let mut control = input_b.clone();
    control["producer_key"] = json!("native-auth-revision-positive-control");
    let authorized = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.submit",
        &control,
        Some(REVIEWER),
    );
    assert_eq!(authorized["record"]["authority_revision"], authority);
    let actual = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.submit",
        &input_a,
        Some(REVIEWER),
    );
    let fresh = ctrl_call(
        &world.ctrl,
        &world.root,
        "central.receiving.read",
        &json!({"return_ref":actual["return_ref"]}),
        None,
    );
    assert_eq!(actual, fresh);
    for (field, original) in [
        ("source_ref", "source_ref"),
        ("document_id", "document_id"),
        ("proposed_source_revision", "expected_source_revision"),
        ("proposal", "proposal"),
        ("run_ref", "run_ref"),
        ("task_ref", "task_ref"),
        ("session_ref", "session_ref"),
    ] {
        assert_eq!(actual["record"][field], input_b[original]);
    }
    assert_eq!(
        actual["record"]["request_digest"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_string(&input_a).unwrap().as_bytes())
        )
    );
    assert_ne!(
        actual["record"]["request_digest"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_string(&input_b).unwrap().as_bytes())
        )
    );
    world.action(json!({"operation":"attach-receiving","attempt_ref":attempt,"receiving_ref":actual["return_ref"],"source_revision":actual["revision"],"evidence_refs":[actual["return_ref"]]}));
    world.transition("finishing", None);
    let ledger = ledger_bytes(&world);
    let state = fs::read(&world.state).unwrap();
    let closure = json!({"finalAttemptRef":attempt,"reviewerAttemptRefs":[attempt],"receivingRef":actual["return_ref"]});
    let refused =
        world.raw_action(&world.request(world.transition_operation("finished", Some(closure))));
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("exact original request digest"),
        "must reach exact digest admission, not another unrelated refusal"
    );
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(ledger, ledger_bytes(&world));
    assert_eq!(world.ctrl, native_ctrl());
}

#[cfg(unix)]
fn actual_previous_ctrl() -> (PathBuf, Vec<u8>) {
    let path = PathBuf::from(
        std::env::var_os("FACTORY_TEST_CTRL_PRE_LOOKUP")
            .expect("actual previous native Ctrl required"),
    )
    .canonicalize()
    .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes.starts_with(b"\x7fELF")
            || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        std::env::var("FACTORY_TEST_CTRL_PRE_LOOKUP_SHA256")
            .expect("exact previous native Ctrl pin required")
    );
    (path, bytes)
}

#[cfg(unix)]
#[test]
#[ignore = "requires genuine old/current Ctrl; original alias, current physical owner and actual final receiving"]
fn native_receiving_original_alias_can_finish_through_same_physical_host_and_new_client() {
    use std::os::unix::fs::symlink;
    let (previous, previous_bytes) = actual_previous_ctrl();
    let (world, attempt) = final_native_world();
    assert_ne!(previous_bytes, fs::read(&world.ctrl).unwrap());
    let alias = world.root.join("original-return-root-alias");
    symlink(&world.root, &alias).unwrap();
    let mut request = world.receiving_request(&attempt);
    request["central"]["root"] = json!(alias);
    request["central"]["binary"] = json!(previous);
    let received = value(receiving_output(&world, &request));
    assert_eq!(received["needsReconciliation"], false);
    let reading = world.reading();
    let intent = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt)
        .unwrap()
        .observations
        .iter()
        .find(|record| {
            record.contract == "factory.attempt-receiving-call/v1"
                && record.phase
                    == epilogos_factory::attempt_runtime::OwnerOperationPhase::Dispatching
        })
        .unwrap();
    assert_eq!(intent.payload["hostEndpoint"]["root"], json!(alias));
    assert_eq!(intent.payload["hostEndpoint"]["binary"], json!(previous));
    assert_eq!(
        intent.payload["ownerClaim"]["canonicalRoot"],
        json!(world.root)
    );
    world.transition("finishing", None);
    let closure = json!({"finalAttemptRef":attempt,"reviewerAttemptRefs":[attempt],
        "receivingRef":received["centralResponse"]["data"]["return_ref"]});
    let other = World::new(true);
    let state = fs::read(&world.state).unwrap();
    let ledger_a = ledger_bytes(&world);
    let ledger_b = optional_ledger_bytes(&other);
    fs::remove_file(&alias).unwrap();
    symlink(&other.root, &alias).unwrap();
    let refused = world
        .raw_action(&world.request(world.transition_operation("finished", Some(closure.clone()))));
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("root physical identity changed"));
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(ledger_a, ledger_bytes(&world));
    assert_eq!(ledger_b, optional_ledger_bytes(&other));
    fs::remove_file(&alias).unwrap();
    symlink(&world.root, &alias).unwrap();
    // Actual host uses canonical R and genuine CURRENT binary. Original Alias
    // and OLD client remain retained; no explicit lookupEndpoint is supplied.
    world.transition("finished", Some(closure));
    let (finished, _) = world.closure_readings("native-original-alias-finished");
    assert_eq!(finished["completionVerified"], true);
    assert_eq!(finished["lifecycle"], "finished");
    assert_eq!(ledger_a, ledger_bytes(&world));
    assert_eq!(ledger_b, optional_ledger_bytes(&other));
    assert_eq!(previous_bytes, fs::read(previous).unwrap());
    assert_eq!(world.ctrl, native_ctrl());
}

#[cfg(unix)]
#[test]
#[ignore = "requires genuine old/current Ctrl and controlled authenticated human review; original alias host bridge"]
fn native_question_original_alias_replays_reads_and_resolves_through_same_physical_host() {
    use std::os::unix::fs::symlink;
    // Require the exact current qualified client even though this case uses an
    // actual previous client for the first native question submission.
    let current = native_ctrl();
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&current).unwrap())),
        std::env::var("FACTORY_TEST_CTRL_SHA256").expect("exact current native Ctrl pin required")
    );
    let (previous, previous_bytes) = actual_previous_ctrl();
    assert_ne!(previous_bytes, fs::read(&current).unwrap());
    let world = World::new(false);
    world.start("inspect-source");
    world.request_decision();
    let alias = world.root.join("original-question-root-alias");
    symlink(&world.root, &alias).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&world.state).unwrap()).unwrap();
    let mut request = saved["state"]["attemptStates"][RUN]["actionReceipts"]
        .as_object()
        .unwrap()
        .values()
        .find(|saved| saved["request"]["operation"]["operation"] == "request-unit-decision")
        .unwrap()["request"]
        .clone();
    request["expectedRevision"] = json!(world.reading().revision);
    let submit_args = [
        "attempt",
        "decision",
        "submit",
        world.state.to_str().unwrap(),
        "-",
        "--json",
    ];
    let submitted =
        value(world.factory_at_owner(&submit_args, Some(&request), Some(AGENT), &previous, &alias));
    let pending = submitted["centralResponse"]["data"].clone();
    assert!(pending["return_ref"].as_str().is_some());
    let reading = world.reading();
    let intent = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == "attempt:native-inspect-source")
        .unwrap()
        .observations
        .iter()
        .find(|record| {
            record.contract == "factory.attempt-unit-decision-call/v1"
                && record.phase
                    == epilogos_factory::attempt_runtime::OwnerOperationPhase::Dispatching
        })
        .unwrap();
    assert_eq!(intent.payload["hostEndpoint"]["root"], json!(alias));
    assert_eq!(intent.payload["hostEndpoint"]["binary"], json!(previous));
    assert_eq!(
        intent.payload["ownerClaim"]["canonicalRoot"],
        json!(world.root)
    );
    let before_replay = ledger_bytes(&world);
    request["expectedRevision"] = json!(world.reading().revision);
    let replay = value(world.factory(&submit_args, Some(&request), Some(AGENT)));
    assert_eq!(
        replay["centralResponse"]["action"],
        "central.receiving.read"
    );
    assert_eq!(
        replay["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_eq!(
        replay["centralResponse"]["data"]["record"],
        pending["record"]
    );
    assert_eq!(before_replay, ledger_bytes(&world));
    // Genuine native review uses the bounded controlled HUMAN grant; no public
    // label or fabricated response supplies the decision authority.
    let reviewed = world.review(&pending, "answered", Some("Resume bounded work"), HUMAN);
    let read_args = [
        "attempt",
        "decision",
        "read",
        world.state.to_str().unwrap(),
        RUN,
        DECISION,
        reviewed["return_ref"].as_str().unwrap(),
        "--json",
    ];
    let other = World::new(true);
    let state = fs::read(&world.state).unwrap();
    let ledger_a = ledger_bytes(&world);
    let ledger_b = optional_ledger_bytes(&other);
    fs::remove_file(&alias).unwrap();
    symlink(&other.root, &alias).unwrap();
    let refused = world.factory(&read_args, None, None);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("root physical identity changed"));
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(ledger_a, ledger_bytes(&world));
    assert_eq!(ledger_b, optional_ledger_bytes(&other));
    fs::remove_file(&alias).unwrap();
    symlink(&world.root, &alias).unwrap();
    let actual = value(world.factory(&read_args, None, None));
    assert_eq!(actual["centralResponse"]["data"], reviewed);
    let response = actual["decisionResponse"].clone();
    assert!(
        response.is_object(),
        "actual native reviewed response required; no fallback"
    );
    assert_eq!(response["outcome"], "resume");
    let mut resolution = world.request(json!({"operation":"resolve-unit-decision",
        "human_request_ref":DECISION,"response":response}));
    resolution["caller"] =
        json!({"callerRef":HUMAN_REF,"projectionKind":"desktop-human","lineage":[HUMAN_REF]});
    value(world.raw_action(&resolution));
    assert_eq!(
        world.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::Active
    );
    let resumed = fs::read(&world.state).unwrap();
    value(world.raw_action(&resolution));
    assert_eq!(
        resumed,
        fs::read(&world.state).unwrap(),
        "exact reply cannot resume twice"
    );
    assert_eq!(ledger_a, ledger_bytes(&world));
    assert_eq!(ledger_b, optional_ledger_bytes(&other));
    assert_eq!(previous_bytes, fs::read(previous).unwrap());
    assert_eq!(world.ctrl, native_ctrl());
}

// Capture-consumer definitions. All scripts below execute the actual pinned
// Ctrl first. They add OS stream/lifetime adversity, never owner reply JSON.
#[cfg(unix)]
fn capture_ctrl_fault(world: &World, name: &str, tail: &str) -> PathBuf {
    let reply = world.root.join(format!("{name}.actual-reply"));
    let calls = world.root.join(format!("{name}.calls"));
    native_script(world,name,&format!(
        "printf '%s\\n' \"$6\" >> {}\n{} \"$@\" > {}\nstatus=$?\n/bin/cat {}\n{}\nexit \"$status\"\n",
        native_shell_quote(&calls),native_shell_quote(&world.ctrl),native_shell_quote(&reply),native_shell_quote(&reply),tail))
}
#[cfg(unix)]
fn recover_capture_without_submit(world: &World, request: &mut Value, name: &str) {
    let before = ledger_bytes(world);
    request["recover"] = json!(true);
    request["lookupEndpoint"] = request["central"].clone();
    request["lookupEndpoint"]["binary"] = json!(world.ctrl);
    request["expectedRevision"] = json!(world.reading().revision);
    let recovered = value(receiving_output(world, request));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(
        recovered["centralResponse"]["action"],
        "central.receiving.read"
    );
    assert_eq!(
        recovered["centralResponse"]["data"]["lookup"]["original_request_verified"],
        true
    );
    assert_eq!(
        before,
        ledger_bytes(world),
        "actual guarded lookup cannot add a native record/cursor"
    );
    assert_eq!(
        fs::read_to_string(world.root.join(format!("{name}.calls")))
            .unwrap()
            .lines()
            .filter(|line| *line == "central.receiving.submit")
            .count(),
        1,
        "exactly one actual producer submission"
    );
    assert_eq!(world.ctrl, native_ctrl());
}
#[cfg(unix)]
#[test]
#[ignore = "actual pinned Ctrl; real submit then owned exec sleep/pipe timeout, no response double"]
fn native_receiving_capture_deadline_never_admits_a_json_shaped_prefix_and_restart_only_looks_up() {
    let (world, attempt) = final_native_world();
    let baseline = ledger_bytes(&world);
    let name = "capture-native-ack-deadline";
    let fault = capture_ctrl_fault(&world, name, "exec /bin/sleep 35");
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(fault);
    let unresolved = value(receiving_output(&world, &request));
    let actual: Value =
        serde_json::from_slice(&fs::read(world.root.join(format!("{name}.actual-reply"))).unwrap())
            .unwrap();
    assert_eq!(actual["ok"], true);
    assert_eq!(actual["action"], "central.receiving.submit");
    assert_eq!(
        ledger_bytes(&world).len(),
        baseline.len() + 1,
        "real native producer committed before transport loss"
    );
    assert_eq!(unresolved["needsReconciliation"], true);
    assert!(unresolved["centralResponse"].is_null());
    let captured = &unresolved["transportObservation"]["payload"]["detail"]["nativeCapture"];
    assert_eq!(captured["processStarted"], true);
    assert_eq!(captured["timedOut"], true);
    assert_eq!(captured["centralResponse"], Value::Null);
    assert_eq!(captured["stdout"]["byteStanding"], "captured bytes only");
    assert!(captured["stdout"]["byteLength"].as_u64().unwrap() > 0);
    assert_eq!(unresolved["transportObservation"]["phase"], "uncertain");
    // New CLI process opens the retained canonical state; no cached public JSON
    // can attach receiving or claim a completed Run.
    assert!(!world.reading().completion_verified);
    recover_capture_without_submit(&world, &mut request, name);
}
#[cfg(unix)]
#[test]
#[ignore = "actual Ctrl plus real invalid UTF8 marker; exact private bytes and safe public diagnostic"]
fn native_receiving_capture_keeps_invalid_bytes_and_complete_status_without_debug_disclosure() {
    let (world, attempt) = final_native_world();
    let name = "capture-native-invalid-utf8";
    let fault=capture_ctrl_fault(&world,name,"printf '\\377capture-private-nonsecret-marker'\nprintf '\\376capture-private-stderr-marker' >&2");
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(fault);
    let output = receiving_output(&world, &request);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("capture-private"));
    let unresolved = value(output);
    assert!(unresolved["centralResponse"].is_null());
    let captured = &unresolved["transportObservation"]["payload"]["detail"]["nativeCapture"];
    assert_eq!(captured["stage"], "nativeReply");
    assert_eq!(captured["exitCode"], 0);
    assert_eq!(captured["stdoutEof"], true);
    assert_eq!(captured["stderrEof"], true);
    let stdout = captured["stdout"]["retainedPrefix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let stderr = captured["stderr"]["retainedPrefix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let mut actual = fs::read(world.root.join(format!("{name}.actual-reply"))).unwrap();
    actual.extend_from_slice(b"\xffcapture-private-nonsecret-marker");
    assert_eq!(stdout, &actual[..actual.len().min(16 * 1024)]);
    assert_eq!(stderr, b"\xfecapture-private-stderr-marker");
    assert_eq!(
        captured["stdout"]["sha256"],
        format!("{:x}", Sha256::digest(&actual))
    );
    let retained = world
        .reading()
        .attempts
        .into_iter()
        .find(|r| r.attempt_ref == attempt)
        .unwrap();
    assert!(!format!("{retained:?}").contains("capture-private"));
    assert!(!format!("{retained:?}").contains("retainedPrefix"));
    recover_capture_without_submit(&world, &mut request, name);
}
#[cfg(unix)]
#[test]
#[ignore = "actual Ctrl then real OS output beyond capture profile; bounded evidence and no retry"]
fn native_receiving_capture_overflow_retains_bounded_actual_prefix_and_guarded_original() {
    let (world, attempt) = final_native_world();
    let name = "capture-native-output-overflow";
    let fault = capture_ctrl_fault(
        &world,
        name,
        "while :; do printf '%8192s' capture-private-overflow; done",
    );
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(fault);
    let unresolved = value(receiving_output(&world, &request));
    assert!(unresolved["centralResponse"].is_null());
    let captured = &unresolved["transportObservation"]["payload"]["detail"]["nativeCapture"];
    assert_eq!(captured["processStarted"], true);
    assert_eq!(captured["stdoutTruncated"], true);
    assert!(captured["stdout"]["byteLength"].as_u64().unwrap() <= 4 * 1024 * 1024);
    assert!(
        captured["stdout"]["retainedPrefix"]
            .as_array()
            .unwrap()
            .len()
            <= 16 * 1024
    );
    assert!(captured["nativeTotalByteLength"].is_null());
    assert!(!world.reading().completion_verified);
    recover_capture_without_submit(&world, &mut request, name);
}
#[cfg(unix)]
#[test]
#[ignore = "actual Central authentication refusal and complete nonzero native ActionResult, no fabricated IO"]
fn native_receiving_complete_nonok_retains_exact_original_code_details_and_status() {
    let (world, attempt) = final_native_world();
    let baseline = ledger_bytes(&world);
    let name = "capture-native-nonok";
    let fault = capture_ctrl_fault(&world, name, "");
    let mut request = world.receiving_request(&attempt);
    request["central"]["binary"] = json!(fault);
    let output = world.factory(
        &[
            "attempt",
            "receiving",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ],
        Some(&request),
        Some("controlled-not-an-authorized-principal"),
    );
    let unresolved = value(output);
    let actual: Value =
        serde_json::from_slice(&fs::read(world.root.join(format!("{name}.actual-reply"))).unwrap())
            .unwrap();
    assert_eq!(actual["ok"], false);
    assert_eq!(actual["action"], "central.receiving.submit");
    assert!(actual["error"]["code"].as_str().is_some());
    assert_eq!(
        unresolved["centralResponse"], actual,
        "actual structured cause/detail preserved verbatim"
    );
    let captured = &unresolved["transportObservation"]["payload"]["detail"]["nativeCapture"];
    assert_ne!(captured["exitCode"], 0);
    assert_eq!(captured["stdoutEof"], true);
    assert_eq!(captured["clientReaped"], true);
    assert_eq!(baseline, ledger_bytes(&world));
    assert_eq!(unresolved["needsReconciliation"], true);
    assert_eq!(world.ctrl, native_ctrl());
}
#[cfg(unix)]
#[test]
#[ignore = "actual native receiving/closure and real host byref read failure; provider bytes unchanged"]
fn native_finished_prefetch_capture_failure_preserves_state_and_never_admits_partial_reply() {
    let (world, attempt) = final_native_world();
    let received = world.return_to_central(&attempt);
    world.transition("finishing", None);
    let closure = json!({"finalAttemptRef":attempt,"reviewerAttemptRefs":[attempt],"receivingRef":received["centralResponse"]["data"]["return_ref"]});
    let request = world.request(world.transition_operation("finished", Some(closure)));
    let state = fs::read(&world.state).unwrap();
    let ledger = ledger_bytes(&world);
    let fault = capture_ctrl_fault(&world, "capture-native-finished-read", "exec /bin/sleep 35");
    let output = world.factory_at_owner(
        &[
            "attempt",
            "action",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ],
        Some(&request),
        None,
        &fault,
        &world.root,
    );
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("retainedPrefix"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("original capture cause retained"));
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(ledger, ledger_bytes(&world));
    assert!(!world.reading().completion_verified);
    assert_eq!(world.ctrl, native_ctrl());
}

#[cfg(unix)]
#[test]
#[ignore = "actual native controlled question/review and real Resolve byref capture failure"]
fn native_resolve_prefetch_capture_failure_preserves_original_question_and_provider() {
    let current = native_ctrl();
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&current).unwrap())),
        std::env::var("FACTORY_TEST_CTRL_SHA256").unwrap()
    );
    let world = World::new(false);
    world.start("inspect-source");
    world.request_decision();
    let submitted = world.submit_decision();
    let pending = &submitted["centralResponse"]["data"];
    let reviewed = world.review(pending, "answered", Some("Resume bounded work"), HUMAN);
    let operation = world.resolve(&reviewed);
    let request = world.request(operation);
    let state = fs::read(&world.state).unwrap();
    let ledger = ledger_bytes(&world);
    let fault = capture_ctrl_fault(&world, "capture-native-resolve-read", "exec /bin/sleep 35");
    let output = world.factory_at_owner(
        &[
            "attempt",
            "action",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ],
        Some(&request),
        None,
        &fault,
        &world.root,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("original capture cause retained"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("retainedPrefix"));
    assert_eq!(state, fs::read(&world.state).unwrap());
    assert_eq!(ledger, ledger_bytes(&world));
    assert_eq!(world.ctrl, native_ctrl());
}

// Real Actuation process fixtures have no ambient socket, credential or store.
// The isolated server has no public shutdown verb; its exact owned Child/group
// is terminated and reaped explicitly, never reported as a graceful ACK.
#[cfg(unix)]
fn native_gateway_binary() -> (PathBuf, String) {
    let path = PathBuf::from(
        std::env::var_os("FACTORY_TEST_ACTUATION_GATEWAY")
            .expect("genuine native Actuation binary required"),
    )
    .canonicalize()
    .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes.starts_with(b"\x7fELF")
            || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
    );
    let sha = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        sha,
        std::env::var("FACTORY_TEST_ACTUATION_GATEWAY_SHA256")
            .expect("exact real Actuation pin required")
    );
    let version = Command::new(&path)
        .arg("--version")
        .output_finite()
        .unwrap();
    assert!(version.status.success());
    (path, sha)
}
#[cfg(unix)]
fn start_capture_gateway(
    root: &Path,
    binary: &Path,
    socket: &Path,
    store: &Path,
    policy: &Path,
) -> (OwnedReceivingProcess, Value) {
    use epilogos_factory::native_gateway::{
        demand_ok, GatewayConnection, GatewayError, GATEWAY_CONTRACT,
    };
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(binary);
    command
        .args(["serve", "--socket"])
        .arg(socket)
        .arg("--store")
        .arg(store)
        .arg("--policy")
        .arg(policy)
        .args([
            "--token-env",
            "FACTORY_CAPTURE_GATEWAY_TOKEN",
            "--max-wait-ms",
            "1000",
        ])
        .env(
            "FACTORY_CAPTURE_GATEWAY_TOKEN",
            "controlled-isolated-capture-gateway-credential",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    // This helper imposes RLIMIT_FSIZE only. Whole-test memory/task/CPU/output
    // caps and the external native deadline belong to the qualification caller.
    unsafe {
        command.pre_exec(|| {
            let files = libc::rlimit {
                rlim_cur: 16 * 1024 * 1024,
                rlim_max: 16 * 1024 * 1024,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &files) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    let group = i32::try_from(child.id()).unwrap();
    let mut owned = OwnedReceivingProcess {
        child,
        group,
        retired: false,
        signal_forbidden: None,
    };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut last_connection_refusal = None;
    // Native startup must confirm a listener every time, including a restart
    // whose dead predecessor left its UDS name. Path existence is no readiness.
    // Existing connect_addr does not give a hostile-backlog absolute timeout;
    // this fixture has one exclusively owned endpoint and an outer native cap.
    loop {
        owned
            .observe_running()
            .expect("native Gateway startup ownership; first wait-loss is permanent");
        assert!(std::time::Instant::now()<until,"native Gateway startup deadline; last actual connection refusal: {last_connection_refusal:?}");
        let attempt_deadline =
            until.min(std::time::Instant::now() + std::time::Duration::from_millis(250));
        let mut connection = match GatewayConnection::connect(socket, attempt_deadline) {
            Ok(connection) => connection,
            Err(GatewayError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                last_connection_refusal = Some(error);
                owned
                    .observe_running()
                    .expect("native Gateway bind pending must retain child custody");
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(error) => panic!("native Gateway startup connection refused: {error}"),
        };
        let hello=connection.call(&json!({"op":"hello","protocol":GATEWAY_CONTRACT,
            "token":"controlled-isolated-capture-gateway-credential","subject":"agent:factory-capture-native"}),attempt_deadline)
            .expect("actual native Gateway hello under the startup deadline");
        demand_ok(&hello, "capture Gateway startup hello")
            .expect("actual native authenticated hello required");
        assert_eq!(hello["gateway"], "actuation-gateway");
        assert_eq!(hello["contract"], GATEWAY_CONTRACT);
        assert_eq!(hello["subject"], "agent:factory-capture-native");
        assert_eq!(hello["store"], json!(store));
        drop(connection);
        owned
            .observe_running()
            .expect("native Gateway must remain owned/running after its actual hello");
        assert!(root.is_dir());
        // The hello proves actual protocol/subject/store, not server PID/birth
        // or atomic attribution across an external hostile namespace writer.
        return (owned, hello);
    }
}
#[cfg(unix)]
fn capture_gateway_stale_listener_is_closed(socket: &Path) {
    use epilogos_factory::native_gateway::{GatewayConnection, GatewayError};
    use std::os::unix::fs::FileTypeExt;
    assert!(
        fs::symlink_metadata(socket)
            .unwrap()
            .file_type()
            .is_socket(),
        "actual abrupt predecessor leaves its UDS pathname"
    );
    // The real OS connect, after exact owned group reap/absence, must refuse
    // before we launch the actual successor. No fake frame or listener is used.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    match GatewayConnection::connect(socket, deadline) {
        Err(GatewayError::Io(error)) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "actual stale UDS must have no listener"
        ),
        Err(error) => {
            panic!("stale Gateway endpoint refusal was not the native no-listener cause: {error}")
        }
        Ok(_) => panic!("retired Gateway endpoint still admitted a native connection"),
    }
}
#[cfg(unix)]
fn capture_gateway_policy(path: &Path) {
    fs::write(path,json!({"schema":"actuation.gateway-policy/v1","attach":[{"subject":"agent:factory-capture-native",
        "stream_ref":"stream:factory-capture-native","role":"agent","agency_ref":"agency:factory-capture-native",
        "agent_ref":"agent:factory-capture-native","locus_ref":"locus:factory-capture-native","may_invoke":false}],"invoke":[]}).to_string()).unwrap();
}
#[cfg(unix)]
fn capture_gateway_invocation(
    socket: &Path,
    program: &Path,
    args: Vec<String>,
    return_ref: &str,
) -> Value {
    json!({"operation":"actuation-gateway","socket_path":socket,"subject":"agent:factory-capture-native",
        "stream_ref":"stream:factory-capture-native","actuation_ref":"actuation:factory-capture-native",
        "agency_ref":"agency:factory-capture-native","agent_session_ref":"session:factory-capture-native","return_ref":return_ref,
        "contract_revision":epilogos_factory::native_owner::ACTUATION_GATEWAY_CONTRACT,"timeout_ms":1000,"executor_wait_ms":0,"program":program,"args":args})
}
#[cfg(unix)]
fn capture_gateway_child(root: &Path, input: &Value, standing: &str, label: &str) -> Value {
    use std::os::unix::fs::PermissionsExt;
    let request = root.join(format!("{label}.input.json"));
    let evidence = root.join(format!("{label}.actual-evidence.json"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request)
        .unwrap();
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    file.write_all(input.to_string().as_bytes()).unwrap();
    file.sync_all().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_capture_gateway_actual_child",
            "--ignored",
            "--nocapture",
        ])
        .env("FACTORY_CAPTURE_GATEWAY_CHILD_INPUT", &request)
        .env("FACTORY_CAPTURE_GATEWAY_CHILD_EVIDENCE", &evidence)
        .env("FACTORY_CAPTURE_GATEWAY_CHILD_STANDING", standing)
        .env(
            epilogos_factory::native_gateway::GATEWAY_TOKEN_ENV,
            "controlled-isolated-capture-gateway-credential",
        )
        .output_finite()
        .unwrap();
    assert!(
        output.status.success(),
        "actual child regression failed; safe stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "actual child test cannot pass with zero selected tests"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("capture-private"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("capture-private"));
    let metadata = fs::metadata(&evidence).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap()
}
#[cfg(unix)]
fn actual_gateway_lines(store: &Path) -> (PathBuf, Vec<u8>, Vec<Value>) {
    let paths = fs::read_dir(store)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "exact isolated native stream");
    let raw = fs::read(&paths[0]).unwrap();
    let rows = std::str::from_utf8(&raw)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect();
    (paths[0].clone(), raw, rows)
}
#[cfg(unix)]
#[test]
#[ignore = "required child gate; three actual executions through the two real native Gateway parents, no standalone fallback"]
fn native_capture_gateway_actual_child() {
    use std::error::Error;
    use std::os::unix::fs::OpenOptionsExt;
    let input = PathBuf::from(
        std::env::var_os("FACTORY_CAPTURE_GATEWAY_CHILD_INPUT")
            .expect("only the actual native parent fixture provides input"),
    );
    let evidence =
        PathBuf::from(std::env::var_os("FACTORY_CAPTURE_GATEWAY_CHILD_EVIDENCE").unwrap());
    let standing = std::env::var("FACTORY_CAPTURE_GATEWAY_CHILD_STANDING").unwrap();
    let invocation =
        serde_json::from_slice::<epilogos_factory::native_owner::NativeOwnerInvocation>(
            &fs::read(input).unwrap(),
        )
        .unwrap();
    let observed = epilogos_factory::native_owner::invoke_native_owner(&invocation);
    let actual = match standing.as_str() {
        "acknowledged-capture" => {
            let receipt = observed.unwrap();
            assert_eq!(
                receipt.phase,
                epilogos_factory::attempt_runtime::OwnerOperationPhase::Uncertain
            );
            assert_eq!(receipt.payload["native_capture"]["stdoutTruncated"], true);
            assert_eq!(receipt.payload["tool_result_receipt"]["ok"], true);
            assert_eq!(receipt.payload["return_receipt"]["ok"], true);
            assert!(!format!("{receipt:?}").contains("retainedPrefix"));
            serde_json::to_value(receipt).unwrap()
        }
        "complete-nonzero" => {
            let receipt = observed.unwrap();
            assert_eq!(
                receipt.phase,
                epilogos_factory::attempt_runtime::OwnerOperationPhase::Failed
            );
            assert_eq!(receipt.payload["exit_code"], 7);
            assert!(receipt.payload["native_capture"].is_null());
            serde_json::to_value(receipt).unwrap()
        }
        "post-capture-store-failure" => {
            let error = observed.unwrap_err();
            assert!(!format!("{error:?} {error}").contains("retainedPrefix"));
            let epilogos_factory::native_owner::NativeOwnerError::ToolObservation(failure) = &error
            else {
                panic!("real local capture must retain its later Gateway cause");
            };
            let original = failure.primary_capture_cause().unwrap();
            let capture = epilogos_factory::native_process::capture_failure(original).unwrap();
            assert!(capture.process_started());
            assert!(capture.stdout_truncated());
            assert!(capture.stdout().len() <= 4 * 1024 * 1024);
            assert!(failure
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .is_some());
            assert!(!failure.acknowledged_receipts()["tool_request_receipt"].is_null());
            assert!(failure.acknowledged_receipts()["tool_result_receipt"].is_null());
            json!({"actualCapture":failure.observation(),"actualAcknowledgedReceipts":failure.acknowledged_receipts(),
                "gatewayCauseStanding":"actual producer refusal; no local ACK or invented durable event","durableCaptureAcknowledged":false})
        }
        _ => panic!("unrecognised real native child standing"),
    };
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&evidence)
        .unwrap();
    out.write_all(serde_json::to_vec(&actual).unwrap().as_slice())
        .unwrap();
    out.sync_all().unwrap();
}
#[cfg(unix)]
#[test]
#[ignore = "actual hash-pinned Actuation server; real local overflow, native ACK and owned restart, no provider"]
fn native_gateway_capture_acknowledgement_survives_actual_store_restart_and_complete_nonzero() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let (binary, sha) = native_gateway_binary();
    let socket = root.join("gateway.sock");
    let store = root.join("stream-store");
    let policy = root.join("gateway-policy.json");
    capture_gateway_policy(&policy);
    let (mut server, initial_hello) =
        start_capture_gateway(&root, &binary, &socket, &store, &policy);
    assert_eq!(initial_hello["store"], json!(store));
    assert_eq!(initial_hello["ok"], true);
    let input = capture_gateway_invocation(
        &socket,
        Path::new("/bin/sh"),
        vec![
            "-c".into(),
            "while :; do printf '%8192s' capture-private-gateway-overflow; done".into(),
        ],
        "return:capture-native-overflow",
    );
    let receipt = capture_gateway_child(&root, &input, "acknowledged-capture", "overflow");
    let (_, before, rows) = actual_gateway_lines(&store);
    let tool_event =
        epilogos_factory::native_gateway::receipt_event(&receipt["payload"]["tool_result_receipt"]);
    let return_event =
        epilogos_factory::native_gateway::receipt_event(&receipt["payload"]["return_receipt"]);
    assert_eq!(tool_event["kind"], "tool-result");
    assert_eq!(tool_event["return_ref"], "return:capture-native-overflow");
    assert_eq!(
        tool_event["metadata"]["native_capture"],
        receipt["payload"]["native_capture"]
    );
    assert_eq!(return_event["kind"], "return");
    assert_eq!(return_event["return_ref"], "return:capture-native-overflow");
    assert!(
        rows.iter().any(|row| row == tool_event),
        "actual native JSONL retains the exact ACKed tool event and capture"
    );
    assert!(
        rows.iter().any(|row| row == return_event),
        "actual native JSONL retains the exact correlated Return event"
    );
    server.retire().unwrap();
    capture_gateway_stale_listener_is_closed(&socket);
    assert_eq!(
        before,
        actual_gateway_lines(&store).1,
        "stale-socket native refusal cannot change retained stream bytes"
    );
    let (mut restarted, successor_hello) =
        start_capture_gateway(&root, &binary, &socket, &store, &policy);
    assert_eq!(successor_hello["store"], initial_hello["store"]);
    assert_eq!(successor_hello["ok"], true);
    let (_, restored, _) = actual_gateway_lines(&store);
    assert_eq!(
        before, restored,
        "server restart cannot rewrite prior actual bytes"
    );
    let input = capture_gateway_invocation(
        &socket,
        Path::new("/bin/sh"),
        vec!["-c".into(), "printf actual-complete-nonzero; exit 7".into()],
        "return:capture-native-complete-nonzero",
    );
    let control = capture_gateway_child(&root, &input, "complete-nonzero", "complete-nonzero");
    assert_eq!(control["phase"], "failed");
    assert_eq!(receipt["phase"], "uncertain");
    let (_, after, _) = actual_gateway_lines(&store);
    assert!(after.starts_with(&before));
    restarted.retire().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(binary).unwrap())),
        sha
    );
}
#[cfg(unix)]
#[test]
#[ignore = "actual hash-pinned Actuation JSONL owner; real EACCES after tool request plus actual local overflow"]
fn native_gateway_after_capture_publication_failure_keeps_both_causes_and_no_false_ack() {
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "real EACCES fixture requires an unprivileged native owner"
    );
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let (binary, sha) = native_gateway_binary();
    let socket = root.join("gateway.sock");
    let store = root.join("stream-store");
    let policy = root.join("gateway-policy.json");
    capture_gateway_policy(&policy);
    let (mut server, initial_hello) =
        start_capture_gateway(&root, &binary, &socket, &store, &policy);
    assert_eq!(initial_hello["store"], json!(store));
    assert_eq!(initial_hello["ok"], true);
    let tail=format!("set -- {}/*.jsonl; [ \"$#\" -eq 1 ] || exit 65; /bin/chmod 000 \"$1\" || exit 66; while :; do printf '%8192s' capture-private-gateway-overflow; done",native_shell_quote(&store));
    let input = capture_gateway_invocation(
        &socket,
        Path::new("/bin/sh"),
        vec!["-c".into(), tail],
        "return:capture-native-store-denied",
    );
    let actual = capture_gateway_child(&root, &input, "post-capture-store-failure", "store-denied");
    let paths = fs::read_dir(&store)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1);
    fs::set_permissions(&paths[0], fs::Permissions::from_mode(0o600)).unwrap();
    let (_, before, rows) = actual_gateway_lines(&store);
    let request_event = epilogos_factory::native_gateway::receipt_event(
        &actual["actualAcknowledgedReceipts"]["tool_request_receipt"],
    );
    assert_eq!(request_event["kind"], "tool-request");
    assert_eq!(
        request_event["return_ref"],
        "return:capture-native-store-denied"
    );
    assert!(
        rows.iter().any(|row| row == request_event),
        "actual native tool-request ACK must be retained verbatim"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "tool-result" || row["kind"] == "return"),
        "failed native store append cannot be certified by consumer metadata"
    );
    assert_eq!(actual["durableCaptureAcknowledged"], false);
    server.retire().unwrap();
    capture_gateway_stale_listener_is_closed(&socket);
    assert_eq!(
        before,
        actual_gateway_lines(&store).1,
        "real stale-socket refusal cannot create an event"
    );
    let (mut restarted, successor_hello) =
        start_capture_gateway(&root, &binary, &socket, &store, &policy);
    assert_eq!(successor_hello["store"], initial_hello["store"]);
    assert_eq!(successor_hello["ok"], true);
    assert_eq!(
        before,
        actual_gateway_lines(&store).1,
        "native successor hello/restart cannot create the refused result/evidence/Return"
    );
    restarted.retire().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(binary).unwrap())),
        sha
    );
}

#[test]
#[ignore = "requires actual pinned ctrl and admitted FACTORY_NATIVE_EVIDENCE_DIR; no provider-double fallback"]
fn actual_late_return_before_termination_retains_current_stop_across_restart() {
    use epilogos_factory::orchestration::LegStatus;
    for move_subject in [false, true] {
        let world = World::new_retained(false);
        let attempt = world.start("inspect-source");
        let unit = world.workflow.unit("inspect-source").unwrap();
        let (returned, artifact) = world.prepare_native_contribution(
            "inspect-source",
            &attempt,
            "late-before-termination",
        );
        // Central's genuine finite subprocess has exited. These Factory facts
        // record its returned material and stop chronology; no model is launched.
        world.action(json!({"operation":"request-cancellation","attempt_ref":attempt}));
        world.action(json!({"operation":"accept-cancellation","attempt_ref":attempt}));
        if move_subject {
            world.action(
                json!({"operation":"advance-subject","subject_ref":unit.subject_ref,
                "revision":returned["revision"]}),
            );
        }
        world.action(artifact);
        let (late, _) = world.closure_readings("late-before-termination");
        assert_eq!(
            late["legs"][unit.reference.to_string()]["status"],
            "late_result"
        );
        let old_late = world.reading().legs[&unit.reference].late_artifacts.clone();
        assert_eq!(old_late.len(), 1);
        // Exact bytes from the real owner at this point, before termination.
        // A separately selected library test consumes only this controlled
        // fixture to exercise the crate-private superseded-artifact path.
        let mut source_file = fs::File::open(&world.state).unwrap();
        assert!(source_file.metadata().unwrap().is_file());
        assert!(source_file.metadata().unwrap().len() <= 16 * 1024 * 1024);
        let mut source_bytes = Vec::new();
        Read::take(&mut source_file, 16 * 1024 * 1024 + 1)
            .read_to_end(&mut source_bytes)
            .unwrap();
        assert!(source_bytes.len() <= 16 * 1024 * 1024);
        let snapshot_path = world._dir.path().join(if move_subject {
            "late-before-termination-moved.owner-state.json"
        } else {
            "late-before-termination-original.owner-state.json"
        });
        let mut snapshot_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&snapshot_path)
            .unwrap();
        snapshot_file.write_all(&source_bytes).unwrap();
        snapshot_file.sync_all().unwrap();
        assert_eq!(fs::read(&snapshot_path).unwrap(), source_bytes);
        assert_eq!(fs::read(&world.state).unwrap(), source_bytes);
        let snapshot_sha = format!("{:x}", Sha256::digest(&source_bytes));
        let snapshot_manifest = json!({
            "schema":"factory.native-cancellation-engine-basis/v1",
            "standing":"actual controlled native owner bytes; library guard input only",
            "sourcePath":world.state,
            "snapshotPath":snapshot_path,
            "sha256":snapshot_sha,
            "bytes":source_bytes.len(),
            "runRef":world.run.reference(),
            "workflowUnitRef":unit.reference,
            "workflowSource":world.workflow.source,
            "moveSubject":move_subject,
            "status":"late_result",
            "attemptRef":attempt,
            "artifactRef":old_late[0].artifact_ref,
            "originalRunCredit":false
        });
        let manifest_bytes = serde_json::to_vec_pretty(&snapshot_manifest).unwrap();
        let manifest_path = snapshot_path.with_extension("manifest.json");
        let mut manifest_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&manifest_path)
            .unwrap();
        manifest_file.write_all(&manifest_bytes).unwrap();
        manifest_file.sync_all().unwrap();
        fs::File::open(world._dir.path())
            .unwrap()
            .sync_all()
            .unwrap();
        assert_eq!(fs::read(&manifest_path).unwrap(), manifest_bytes);
        eprintln!(
            "native cancellation engine basis: {} sha256:{}",
            snapshot_path.display(),
            snapshot_sha
        );
        let request =
            world.request(json!({"operation":"record-process-termination","attempt_ref":attempt}));
        let receipt = value(world.raw_action(&request));
        let committed = fs::read(&world.state).unwrap();
        assert_eq!(value(world.raw_action(&request)), receipt);
        assert_eq!(
            fs::read(&world.state).unwrap(),
            committed,
            "exact restart replay must not record a second termination"
        );
        let current = world.reading();
        let leg = &current.legs[&unit.reference];
        assert_eq!(leg.status, LegStatus::ProcessTerminated);
        assert_eq!(leg.late_artifacts, old_late);
        assert_eq!(leg.attempts.last().unwrap().late_artifacts, old_late);
        assert_eq!(
            leg.status_history
                .iter()
                .filter(|s| **s == LegStatus::ProcessTerminated)
                .count(),
            1
        );
        assert!(!leg.status_history.contains(&LegStatus::Quiescent));
        assert!(!current.completion_verified);
        if move_subject {
            world.refuse_with_reason(
                json!({"operation":"incorporate-late-result","attempt_ref":attempt}),
                &["StaleLateResult"],
            );
        } else {
            world.action(json!({"operation":"incorporate-late-result","attempt_ref":attempt}));
            assert_eq!(
                world.reading().legs[&unit.reference].status,
                LegStatus::Returned
            );
            assert!(!world.reading().completion_verified);
        }
        world.closure_readings("late-termination-final");
    }
}

#[test]
#[ignore = "requires actual pinned ctrl and admitted FACTORY_NATIVE_EVIDENCE_DIR; no provider-double fallback"]
fn actual_termination_before_late_return_replays_original_stop_without_reissue() {
    use epilogos_factory::orchestration::LegStatus;
    let world = World::new_retained(false);
    let attempt = world.start("inspect-source");
    let unit = world.workflow.unit("inspect-source").unwrap();
    let (_, artifact) =
        world.prepare_native_contribution("inspect-source", &attempt, "termination-before-late");
    world.action(json!({"operation":"request-cancellation","attempt_ref":attempt}));
    world.action(json!({"operation":"accept-cancellation","attempt_ref":attempt}));
    let request =
        world.request(json!({"operation":"record-process-termination","attempt_ref":attempt}));
    let receipt = value(world.raw_action(&request));
    world.action(artifact);
    let committed = fs::read(&world.state).unwrap();
    assert_eq!(value(world.raw_action(&request)), receipt);
    assert_eq!(fs::read(&world.state).unwrap(), committed);
    world.refuse_with_reason(
        json!({"operation":"record-process-termination","attempt_ref":attempt}),
        &["InvalidTransition", "LateResult"],
    );
    let current = world.reading();
    let leg = &current.legs[&unit.reference];
    assert_eq!(leg.status, LegStatus::LateResult);
    assert_eq!(leg.late_artifacts.len(), 1);
    assert_eq!(
        leg.status_history
            .iter()
            .filter(|s| **s == LegStatus::ProcessTerminated)
            .count(),
        1
    );
    assert!(!current.completion_verified);
    world.closure_readings("termination-before-late-final");
}

#[test]
#[ignore = "requires actual pinned ctrl and admitted FACTORY_NATIVE_EVIDENCE_DIR; no provider-double fallback"]
fn actual_retry_late_material_cannot_borrow_predecessor_cancellation() {
    use epilogos_factory::orchestration::LegStatus;
    let world = World::new_retained(false);
    let original = world.start("inspect-source");
    let unit = world.workflow.unit("inspect-source").unwrap();
    let (first_returned, first_artifact) =
        world.prepare_native_contribution("inspect-source", &original, "retry-predecessor");
    world.action(json!({"operation":"request-cancellation","attempt_ref":original}));
    world.action(json!({"operation":"accept-cancellation","attempt_ref":original}));
    world.action(first_artifact);
    world.action(json!({"operation":"record-process-termination","attempt_ref":original}));
    world.action(json!({"operation":"mark-quiescent","attempt_ref":original}));
    let prior = world.reading();
    let record = prior
        .attempts
        .iter()
        .find(|r| r.attempt_ref == original)
        .unwrap();
    let mut selected = record.disposition.clone();
    selected.body.agent_session_ref = "session:native-stop-retry".into();
    let mut partial = record
        .dispatch
        .iter()
        .chain(&record.observations)
        .flat_map(|receipt| receipt.partial_effect_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    partial.insert(first_returned["return_ref"].as_str().unwrap().to_owned());
    let resolution = ReresolutionRecord {
        resolution_ref: "resolution:native-stop-retry".into(),
        reason: "Fresh controlled native operation after observed predecessor stop".into(),
        source_revision: selected.participant.source_revision.clone(),
        evidence_refs: partial,
        replacement_now_ref: None,
        replacement_material_ref: None,
        replacement_harness_ref: None,
    };
    let replacement = "attempt:native-stop-retry";
    world.action(json!({"operation":"retry","attempt_ref":replacement,
        "task_ref":record.task_ref,"parent_journey_ref":"journey:controlled-native-receiving",
        "workflow_unit_ref":unit.reference,"grant_ref":"grant:native-inspect-source",
        "disposition":selected,"tracking":[],"reresolution":resolution}));
    let (returned, second_artifact) =
        world.prepare_native_contribution("inspect-source", replacement, "retry-successor");
    world.action(
        json!({"operation":"advance-subject","subject_ref":unit.subject_ref,
        "revision":returned["revision"]}),
    );
    world.action(second_artifact);
    let current = world.reading();
    let leg = &current.legs[&unit.reference];
    assert_eq!(leg.status, LegStatus::LateResult);
    assert_eq!(leg.attempts.len(), 2);
    assert!(leg.attempts[0]
        .status_history
        .contains(&LegStatus::CancellationAccepted));
    assert!(!leg.attempts[1]
        .status_history
        .contains(&LegStatus::CancellationAccepted));
    world.refuse_with_reason(
        json!({"operation":"record-process-termination","attempt_ref":replacement}),
        &["InvalidTransition", "LateResult"],
    );
    world.refuse_with_reason(
        json!({"operation":"record-process-termination","attempt_ref":original}),
        &["historical attempt cannot mutate its replacement"],
    );
    world.action(json!({"operation":"request-cancellation","attempt_ref":replacement}));
    world.refuse_with_reason(
        json!({"operation":"record-process-termination","attempt_ref":replacement}),
        &["InvalidTransition", "CancelRequested"],
    );
    world.action(json!({"operation":"accept-cancellation","attempt_ref":replacement}));
    world.action(json!({"operation":"record-process-termination","attempt_ref":replacement}));
    let final_read = world.reading();
    assert_eq!(
        final_read.legs[&unit.reference].status,
        LegStatus::ProcessTerminated
    );
    assert_eq!(final_read.legs[&unit.reference].late_artifacts.len(), 1);
    assert!(!final_read.completion_verified);
    world.refuse_with_reason(
        json!({"operation":"incorporate-late-result","attempt_ref":replacement}),
        &["StaleLateResult"],
    );
    world.closure_readings("retry-cancellation-final");
}
