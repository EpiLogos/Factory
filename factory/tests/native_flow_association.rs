//! Real installed Central ordinary Flow ownership and the Factory public CLI.
//! These cases commission isolated work and admit navigation relationships;
//! they never manufacture a worker, model, human reply or completed Run.

use epilogos_factory::commission::FactoryDevelopmentalMutationRequest;
use epilogos_factory::project_development_store::read_developmental_state;
use epilogos_factory::workflow::{workflow_source_digest, WorkflowSource};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const ISLAND_OPEN: &str = "<script type=\"application/json\" id=\"ql-doc\">";
const COMMISSION: &str = "commission:native-flow-association-regression";
const ACTOR: &str = "agent:controlled-native-flow-association";
const SESSION: &str = "agent-session:controlled-native-flow-association";
const SUCCESSOR: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBE";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds.try_into().unwrap(), 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "native process failed\nstdout:{}\nstderr:{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("native structured output")
}

fn ctrl_output(binary: &Path, root: &Path, action: &str, input: &Value) -> Output {
    Command::new(binary)
        .args(["--json", "--root"])
        .arg(root)
        .args(["action", "run", action])
        .arg(input.to_string())
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .output()
        .unwrap()
}

fn ctrl(binary: &Path, root: &Path, action: &str, input: &Value) -> Value {
    let result = successful(ctrl_output(binary, root, action, input));
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["status"], "success", "{result}");
    assert_eq!(result["action"], action, "{result}");
    result["data"].clone()
}

fn native_prerequisites() -> (PathBuf, String) {
    let binary = PathBuf::from(std::env::var_os("FACTORY_TEST_CTRL").expect(
        "supply an actual pinned installed ctrl; there is no fake native process fallback",
    ))
    .canonicalize()
    .unwrap();
    let bytes = fs::read(&binary).unwrap();
    assert!(
        bytes.starts_with(b"\x7fELF")
            || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
            || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe]),
        "ctrl must be a real native executable"
    );
    let binary_sha = sha(&bytes);
    if let Ok(expected) = std::env::var("FACTORY_TEST_CTRL_SHA256") {
        assert_eq!(
            binary_sha, expected,
            "installed native Central bytes changed"
        );
    }
    let version = Command::new(&binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(version.starts_with("ctrl "), "not the Central executable");
    let template = PathBuf::from(std::env::var_os("FACTORY_TEST_FLOW_TEMPLATE").expect(
        "supply the actual shipped ql-flow.html; no minimal JSON asset substitutes for the Flow form",
    ))
    .canonicalize()
    .unwrap();
    let form_bytes = fs::read(&template).unwrap();
    let form_sha = sha(&form_bytes);
    if let Ok(expected) = std::env::var("FACTORY_TEST_FLOW_TEMPLATE_SHA256") {
        assert_eq!(form_sha, expected, "actual shipped Flow form changed");
    }
    let form = String::from_utf8(form_bytes).unwrap();
    let doc = parse_document(&form);
    assert_eq!(doc["meta"]["format"]["version"], 4);
    assert_eq!(doc["meta"]["documentId"], Value::Null);
    for collection in ["entries", "notes", "packet", "media", "journal"] {
        assert!(doc[collection].as_array().unwrap().is_empty());
    }
    assert!(
        form.len() > 4096,
        "the shipped self-contained form is required"
    );
    eprintln!(
        "native association prerequisites: {} sha256:{}; form:{} sha256:{}; factory:{} sha256:{}",
        version.trim(),
        binary_sha,
        template.display(),
        form_sha,
        env!("CARGO_BIN_EXE_factory"),
        sha(&fs::read(env!("CARGO_BIN_EXE_factory")).unwrap())
    );
    (binary, form)
}

fn parse_document(html: &str) -> Value {
    let start = html.find(ISLAND_OPEN).expect("actual Flow island") + ISLAND_OPEN.len();
    let end = start + html[start..].find("</script>").unwrap();
    serde_json::from_str(&html[start..end].replace("<\\/script", "</script")).unwrap()
}

fn embed_document(html: &str, doc: &Value) -> String {
    let start = html.find(ISLAND_OPEN).unwrap() + ISLAND_OPEN.len();
    let end = start + html[start..].find("</script>").unwrap();
    let island = doc.to_string().replace("</script", "<\\/script");
    format!("{}{}{}", &html[..start], island, &html[end..])
}

fn document_id() -> String {
    // Same UUID-v4 shape as the shipped blank form's crypto.randomUUID;
    // real OS entropy, no seeded document identity or person contribution.
    let mut bytes = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut bytes)
        .unwrap();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[derive(Clone)]
struct Flow {
    location: Value,
    reading: Value,
}

struct World {
    _directory: TempDir,
    root: PathBuf,
    state: PathBuf,
    ctrl: PathBuf,
    form: String,
    run: String,
    journey: String,
    source: WorkflowSource,
    external: Value,
    capture_dir: OnceLock<PathBuf>,
}

fn persist_capture(directory: &Path, name: &str, bytes: &[u8]) -> Value {
    let path = directory.join(name);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    output.write_all(bytes).unwrap();
    output.sync_all().unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "required native capture readback failed"
    );
    json!({"file":name,"sha256":sha(bytes)})
}

fn capture_operation(
    directory: &Path,
    name: &str,
    binary: &Path,
    args: &[String],
    output: Output,
) -> (Value, Value, Value) {
    let stdout = persist_capture(directory, &format!("{name}.json"), &output.stdout);
    let stderr = persist_capture(directory, &format!("{name}.stderr.txt"), &output.stderr);
    let operation = json!({"argv":std::iter::once(binary.to_str().unwrap().to_owned()).chain(args.iter().cloned()).collect::<Vec<_>>(),
        "exitCode":output.status.code(),"stdout":stdout,"stderr":stderr});
    (successful(output), stdout, operation)
}

fn factory_command(ctrl: &Path, root: &Path, args: &[&str], input: Option<&Value>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(args)
        .env("FACTORY_NATIVE_CENTRAL_BINARY", ctrl)
        .env("FACTORY_NATIVE_CENTRAL_ROOT", root)
        .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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

fn create_document(binary: &Path, root: &Path, form: &str, parent_path: &str, name: &str) -> Value {
    fs::create_dir_all(root.join(parent_path)).unwrap();
    let parent = ctrl(
        binary,
        root,
        "central.files.list",
        &json!({"path":parent_path}),
    );
    let mut doc = parse_document(form);
    doc["meta"]["documentId"] = json!(document_id());
    doc["meta"]["created"] = json!(now());
    let content = embed_document(form, &doc);
    let created = ctrl(
        binary,
        root,
        "central.files.create",
        &json!({
            "parent":parent["location"],"name":name,"content":content,"expected_absent":true,
            "operation_ref":format!("native-flow-first-save:{}",doc["meta"]["documentId"].as_str().unwrap()),
            "actor":ACTOR,"actor_kind":"agent","agent_session_ref":SESSION
        }),
    );
    assert_eq!(created["outcome"], "created");
    let readback = ctrl(
        binary,
        root,
        "central.files.read",
        &json!({"location":created["location"]}),
    );
    assert_eq!(readback["content"], content);
    assert_eq!(readback["revision"], created["revision"]);
    eprintln!(
        "actual ordinary first-save: {} @ {}",
        created["location"]["ref"], created["revision"]
    );
    created
}

impl World {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let (binary, form) = native_prerequisites();
        successful(
            Command::new(&binary)
                .args(["--json", "--root"])
                .arg(&root)
                .arg("init")
                .env_remove("CENTRAL_NATIVE_TOKEN")
                .output()
                .unwrap(),
        );
        let recognized = ctrl(&binary, &root, "central.recognize", &json!({"path":root}));
        assert_eq!(recognized["outcome"], "recognized");
        assert_eq!(recognized["mutated"], false);
        let external = create_document(
            &binary,
            &root,
            &form,
            "Work/association-origin",
            "original-flow.html",
        );
        let state = root.join("factory-development.json");
        let commissioned = successful(factory_command(
            &binary,
            &root,
            &[
                "development",
                "commission",
                state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&json!({
                "contract":"factory.commission-request/v1","requestRef":COMMISSION,
                "projectKey":format!("native-flow-association-{}",parse_document(external["current"]["content"].as_str().unwrap())["meta"]["documentId"].as_str().unwrap()),
                "purpose":"Verify actual native same-Run Flow association",
                "frontier":"No worker is executed or accepted","runDestination":"bounded native relation regression",
                "writeOwner":"factory","commissionedAt":now(),"participantRequirements":[{
                    "ref":ACTOR,"description":"Declared controlled relation test principal; no execution claim",
                    "sourceOwner":"test-contract","sourceRef":"source:native-flow-association-test",
                    "sourceRevision":"native-flow-association-v1"}],
                "rootAct":{"actRef":"act:native-flow-association","agentRef":ACTOR,
                    "purpose":"Test the native relation owner only","scopeRefs":["issue:O-I-203"],
                    "standing":"commissioned-not-executed"}
            })),
        ));
        let run = commissioned["commission"]["runRef"]
            .as_str()
            .unwrap()
            .to_owned();
        let journey = commissioned["commission"]["journeyRef"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
        source.source.flow_ref = Some(external["location"]["ref"].as_str().unwrap().to_owned());
        source.source.digest = workflow_source_digest(&source).unwrap();
        let world = Self {
            _directory: directory,
            root,
            state,
            ctrl: binary,
            form,
            run,
            journey,
            source,
            external,
            capture_dir: OnceLock::new(),
        };
        let attached = world.owner_request("attach-source", "factory", &world.source.source.reference.to_string(),
            &world.source.source.revision, json!({"kind":"attach-workflow-source","runRef":world.run,"workflowSource":world.source}));
        world.mutate(&attached);
        world.attach(&world.run, &world.source);
        let external_activity = world.owner_request("retain-external", "central",
            world.external["location"]["ref"].as_str().unwrap(), world.external["revision"].as_str().unwrap(),
            json!({"kind":"correlate-owner-activity","journeyRef":world.journey,"activityRef":world.external["location"]["ref"]}));
        world.mutate(&external_activity);
        world
    }

    fn factory(&self, args: &[&str], input: Option<&Value>) -> Output {
        factory_command(&self.ctrl, &self.root, args, input)
    }

    fn bytes(&self) -> Vec<u8> {
        fs::read(&self.state).unwrap()
    }

    fn owner_request(
        &self,
        id: &str,
        owner: &str,
        reference: &str,
        revision: &str,
        mutation: Value,
    ) -> Value {
        json!({"contract":"factory.developmental-mutation-request/v1","mutationRef":format!("mutation:{id}"),
            "occurrenceRef":format!("occurrence:{id}"),"source":{"owner":owner,"reference":reference,
                "revision":revision,"standing":"owner-native-observation"},"observedAt":now(),"mutation":mutation})
    }

    fn mutate_output(&self, request: &Value) -> Output {
        self.factory(
            &[
                "development",
                "mutate",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(request),
        )
    }

    fn mutate(&self, request: &Value) -> Value {
        successful(self.mutate_output(request))
    }

    fn refused(&self, request: &Value, expected_error: &str) {
        let before = self.bytes();
        let output = self.mutate_output(request);
        assert!(
            !output.status.success(),
            "unexpected relation admission: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error
                .to_ascii_lowercase()
                .contains(&expected_error.to_ascii_lowercase()),
            "expected {expected_error:?}, actual:{error}"
        );
        assert_eq!(
            self.bytes(),
            before,
            "refusal changed canonical Factory bytes"
        );
        eprintln!(
            "actual association refusal:{}; unchanged state sha256:{}",
            error.trim(),
            sha(&before)
        );
    }

    fn reading(&self, kind: &str, reference: &str) -> Value {
        successful(self.factory(
            &[
                "development",
                kind,
                self.state.to_str().unwrap(),
                reference,
                "--json",
            ],
            None,
        ))
    }

    fn attempts(&self, run: &str) -> Value {
        successful(self.factory(
            &[
                "attempt",
                "read",
                self.state.to_str().unwrap(),
                run,
                "--json",
            ],
            None,
        ))
    }

    fn attach(&self, run: &str, source: &WorkflowSource) {
        successful(self.factory(
            &[
                "attempt",
                "attach",
                self.state.to_str().unwrap(),
                run,
                &source.source.reference.to_string(),
                "--json",
            ],
            None,
        ));
    }

    fn flow(&self, name: &str) -> Flow {
        let created = create_document(
            &self.ctrl,
            &self.root,
            &self.form,
            "Control/user/flows",
            name,
        );
        let reading = ctrl(
            &self.ctrl,
            &self.root,
            "central.flow.read",
            &json!({"location":created["location"],"max_entries":1}),
        );
        assert_eq!(reading["format_version"], 4);
        assert_eq!(reading["private_collections_included"], false);
        Flow {
            location: created["location"].clone(),
            reading,
        }
    }

    fn association(&self, id: &str, run: &str, source: &WorkflowSource, flow: &Flow) -> Value {
        let reading = self.reading("run", run);
        let journey = self.reading("journey", &self.journey);
        self.owner_request(id,"central",flow.location["ref"].as_str().unwrap(), flow.reading["revision"].as_str().unwrap(),json!({
            "kind":"associate-run-flow","expectedProviderRevision":reading["provenance"]["factoryStateRevision"],
            "expectedJourneyRevision":journey["revision"],"expectedRunRevision":reading["revision"],
            "association":{"journeyRef":self.journey,"runRef":run,"workflowSourceRef":source.source.reference,
                "workflowSourceRevision":source.source.revision,"workflowSourceDigest":source.source.digest,
                "flow":{"location":flow.location,"documentId":flow.reading["document_id"],
                    "documentRevision":flow.reading["document_revision"],"sourceRevision":flow.reading["revision"]},
                "basisRefs":[COMMISSION,self.external["location"]["ref"].as_str().unwrap(),source.source.reference.to_string().as_str()]}
        }))
    }

    fn advance_flow(&self, flow: &Flow) -> Flow {
        let read = ctrl(
            &self.ctrl,
            &self.root,
            "central.files.read",
            &json!({"location":flow.location}),
        );
        let html = read["content"].as_str().unwrap();
        let mut doc = parse_document(html);
        doc["meta"]["revision"] = json!(doc["meta"]["revision"].as_u64().unwrap() + 1);
        doc["meta"]["title"] = json!("Actual native source changed after the association read");
        ctrl(
            &self.ctrl,
            &self.root,
            "central.files.write",
            &json!({"location":flow.location,"expected_revision":read["revision"],
            "content":embed_document(html,&doc),"actor":ACTOR,"actor_kind":"agent","agent_session_ref":SESSION}),
        );
        Flow {
            location: flow.location.clone(),
            reading: ctrl(
                &self.ctrl,
                &self.root,
                "central.flow.read",
                &json!({"location":flow.location,"max_entries":1}),
            ),
        }
    }

    fn replace_document_at_same_location(&self, flow: &Flow) -> Flow {
        let read = ctrl(
            &self.ctrl,
            &self.root,
            "central.files.read",
            &json!({"location":flow.location}),
        );
        let mut doc = parse_document(&self.form);
        doc["meta"]["documentId"] = json!(document_id());
        doc["meta"]["created"] = json!(now());
        assert_ne!(doc["meta"]["documentId"], flow.reading["document_id"]);
        ctrl(
            &self.ctrl,
            &self.root,
            "central.files.write",
            &json!({"location":flow.location,"expected_revision":read["revision"],
            "content":embed_document(&self.form,&doc),"actor":ACTOR,"actor_kind":"agent","agent_session_ref":SESSION}),
        );
        Flow {
            location: flow.location.clone(),
            reading: ctrl(
                &self.ctrl,
                &self.root,
                "central.flow.read",
                &json!({"location":flow.location,"max_entries":1}),
            ),
        }
    }

    fn capture_phase(&self, phase: &str, run: &str, sibling: &str, expected_flow: &Flow) {
        let Some(configured) = std::env::var_os("FACTORY_NATIVE_FLOW_EVIDENCE_DIR") else {
            return;
        };
        let configured = PathBuf::from(configured);
        assert!(
            configured.is_absolute(),
            "native capture requires the existing allocated absolute evidence root"
        );
        assert!(
            configured.is_dir(),
            "configured required native evidence root is missing"
        );
        assert_eq!(
            configured.canonicalize().unwrap(),
            configured,
            "native evidence root must name its actual canonical directory"
        );
        let case = self.capture_dir.get_or_init(|| {
            let case = configured.join(format!("native-flow-association-{}", document_id()));
            fs::create_dir(&case).unwrap();
            case
        });
        assert_eq!(
            case.parent(),
            Some(configured.as_path()),
            "configured evidence root changed during a native case"
        );
        let directory = case.join(phase);
        fs::create_dir(&directory).unwrap();
        let factory = PathBuf::from(env!("CARGO_BIN_EXE_factory"))
            .canonicalize()
            .unwrap();
        let factory_sha_before = sha(&fs::read(&factory).unwrap());
        let ctrl_sha_before = sha(&fs::read(&self.ctrl).unwrap());
        let state_before = self.bytes();
        let mut files = serde_json::Map::new();
        let mut operations = Vec::new();
        let mut readings = Vec::new();
        let mut versions = serde_json::Map::new();
        for (name, binary) in [
            ("factory-version", &factory),
            ("central-version", &self.ctrl),
        ] {
            let output = Command::new(binary)
                .arg("--version")
                .env_remove("CENTRAL_NATIVE_TOKEN")
                .output()
                .unwrap();
            let stdout = persist_capture(&directory, &format!("{name}.txt"), &output.stdout);
            let stderr = persist_capture(&directory, &format!("{name}.stderr.txt"), &output.stderr);
            assert!(
                output.status.success(),
                "actual native version read refused"
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.starts_with(if name == "factory-version" {
                "factory "
            } else {
                "ctrl "
            }));
            versions.insert(name.into(), json!({"reading":text.trim(),"file":stdout}));
            operations.push(json!({"argv":[binary,"--version"],"exitCode":output.status.code(),"stdout":stdout,"stderr":stderr}));
        }
        for (name, reference) in [("run", run), ("sibling", sibling)] {
            let args = vec![
                "development".to_owned(),
                "run".into(),
                self.state.to_str().unwrap().into(),
                reference.into(),
                "--json".into(),
            ];
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            let (reading, file, operation) =
                capture_operation(&directory, name, &factory, &args, self.factory(&refs, None));
            assert_eq!(reading["runRef"], reference);
            assert_eq!(reading["contract"], "factory.run-reading/v1");
            files.insert(name.into(), file);
            operations.push(operation);
            readings.push(reading);
        }
        let native_input = json!({"location":expected_flow.location,"max_entries":1});
        let flow_args = vec![
            "--json".to_owned(),
            "--root".into(),
            self.root.to_str().unwrap().into(),
            "action".into(),
            "run".into(),
            "central.flow.read".into(),
            native_input.to_string(),
        ];
        let (flow, file, operation) = capture_operation(
            &directory,
            "flow",
            &self.ctrl,
            &flow_args,
            ctrl_output(&self.ctrl, &self.root, "central.flow.read", &native_input),
        );
        assert_eq!(flow["ok"], true);
        assert_eq!(flow["status"], "success");
        assert_eq!(flow["action"], "central.flow.read");
        assert_eq!(
            flow["data"]["document_id"],
            expected_flow.reading["document_id"]
        );
        assert_eq!(flow["data"]["revision"], expected_flow.reading["revision"]);
        assert_eq!(flow["data"]["location"], expected_flow.location);
        assert_eq!(flow["data"]["private_collections_included"], false);
        files.insert("flow".into(), file);
        operations.push(operation);
        let native_input = json!({"location":expected_flow.location});
        let file_args = vec![
            "--json".to_owned(),
            "--root".into(),
            self.root.to_str().unwrap().into(),
            "action".into(),
            "run".into(),
            "central.files.read".into(),
            native_input.to_string(),
        ];
        let (source, file, operation) = capture_operation(
            &directory,
            "file",
            &self.ctrl,
            &file_args,
            ctrl_output(&self.ctrl, &self.root, "central.files.read", &native_input),
        );
        assert_eq!(source["ok"], true);
        assert_eq!(source["status"], "success");
        assert_eq!(source["action"], "central.files.read");
        assert_eq!(source["data"]["revision"], flow["data"]["revision"]);
        assert_eq!(source["data"]["location"], flow["data"]["location"]);
        let doc = parse_document(source["data"]["content"].as_str().unwrap());
        assert_eq!(doc["meta"]["documentId"], flow["data"]["document_id"]);
        assert_eq!(doc["meta"]["revision"], flow["data"]["document_revision"]);
        for collection in ["entries", "notes", "packet", "media", "journal"] {
            assert!(
                doc[collection].as_array().unwrap().is_empty(),
                "only controlled blank-source captures are allowed"
            );
        }
        files.insert("file".into(), file);
        operations.push(operation);
        let operation_bytes = serde_json::to_vec_pretty(&operations).unwrap();
        let operation_file = persist_capture(&directory, "operations.json", &operation_bytes);
        let template = PathBuf::from(std::env::var_os("FACTORY_TEST_FLOW_TEMPLATE").unwrap())
            .canonicalize()
            .unwrap();
        let template_bytes = fs::read(&template).unwrap();
        assert_eq!(
            template_bytes,
            self.form.as_bytes(),
            "actual form changed during native capture"
        );
        assert_eq!(
            readings[0]["provenance"]["factoryStateRevision"],
            readings[1]["provenance"]["factoryStateRevision"],
            "isolated Factory basis changed between native reads"
        );
        let state_after = self.bytes();
        assert_eq!(
            state_after, state_before,
            "capture changed actual Factory owner bytes"
        );
        let factory_sha_after = sha(&fs::read(&factory).unwrap());
        let ctrl_sha_after = sha(&fs::read(&self.ctrl).unwrap());
        assert_eq!(
            factory_sha_before, factory_sha_after,
            "Factory executable changed during native capture"
        );
        assert_eq!(
            ctrl_sha_before, ctrl_sha_after,
            "Central executable changed during native capture"
        );
        let state = persist_capture(&directory, "factory-state.json", &state_after);
        let manifest = json!({"schema":"oi.factory-run-flow-native-phase/v1","phase":phase,"files":files,"operations":operation_file,
            "factory":{"binary":factory,"sha256":factory_sha_before,"sha256Before":factory_sha_before,"sha256After":factory_sha_after,"statePath":self.state,
                "stateSha256Before":sha(&state_before),"stateSha256After":sha(&state_after),
                "state":state,"runRef":run,"siblingRunRef":sibling,"journeyRef":self.journey,
                "providerRevision":readings[0]["provenance"]["factoryStateRevision"],"runRevision":readings[0]["revision"]},
            "central":{"binary":self.ctrl,"sha256":ctrl_sha_before,"sha256Before":ctrl_sha_before,"sha256After":ctrl_sha_after,"root":self.root},
            "versions":versions,
            "form":{"path":template,"sha256":sha(&template_bytes)},
            "standing":"actual isolated native read captures; no repair Run or worker/whole completion claim",
            "controlledIsolatedOwner":true,"modelOpened":false,"humanAnswer":false,"credentialsCopied":false,"repairRunTouched":false});
        persist_capture(
            &directory,
            "manifest.json",
            &serde_json::to_vec_pretty(&manifest).unwrap(),
        );
        eprintln!("actual native Flow phase capture: {}", directory.display());
    }

    fn continuation(&self, relation: &str) -> (String, WorkflowSource) {
        let run = self.reading("run", &self.run);
        let journey = self.reading("journey", &self.journey);
        let mut source = self.source.clone();
        source.source.reference = "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAX"
            .parse()
            .unwrap();
        source.source.revision = "explicit-native-relation-successor-v2".into();
        source.source.digest = workflow_source_digest(&source).unwrap();
        self.mutate(&self.owner_request("source-continuation","factory",&source.source.reference.to_string(),&source.source.revision,json!({
            "kind":"continue-commission","continuationRelation":relation,"commissionRef":COMMISSION,"journeyRef":self.journey,
            "predecessorRunRef":self.run,"expectedJourneyRevision":journey["revision"],"expectedPredecessorRunRevision":run["revision"],
            "predecessorSourceRef":self.source.source.reference,"predecessorSourceRevision":self.source.source.revision,
            "predecessorSourceDigest":self.source.source.digest,"successorRunRef":SUCCESSOR,"workflowSource":source,
            "reason":"Explicit isolated continuation retains original source and unexecuted work",
            "basisRefs":[COMMISSION,self.external["location"]["ref"].as_str().unwrap()]
        })));
        self.attach(SUCCESSOR, &source);
        (SUCCESSOR.into(), source)
    }
}

fn associations(reading: &Value) -> Vec<Value> {
    reading["flowAssociations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn assert_unfinished(world: &World, run: &str) {
    let reading = world.attempts(run);
    assert_eq!(reading["wholeRunState"], "incomplete");
    assert_eq!(reading["completionVerified"], false);
    assert!(reading["attempts"].as_array().unwrap().is_empty());
    assert!(matches!(
        reading["lifecycle"].as_str(),
        Some("seeded" | "active")
    ));
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn actual_flow_association_is_per_run_retains_external_ancestry_and_does_not_complete_work() {
    let world = World::new();
    let (bounded, source) = world.continuation("bounded-contribution");
    let flow = world.flow("bounded-factory-flow.html");
    let request = world.association("associate-bounded", &bounded, &source, &flow);
    let before: Value = serde_json::from_slice(&world.bytes()).unwrap();
    let run_before = world.reading("run", &bounded);
    let journey_before = world.reading("journey", &world.journey);
    world.capture_phase("before", &bounded, &world.run, &flow);
    let reply = world.mutate(&request);
    assert_eq!(reply["status"], "applied");
    let after: Value = serde_json::from_slice(&world.bytes()).unwrap();
    for pointer in [
        "/state/workflowSources",
        "/state/attemptStates",
        "/state/build/runs",
    ] {
        assert!(
            before.pointer(pointer).is_some(),
            "missing canonical owner field {pointer}"
        );
        assert_eq!(
            before.pointer(pointer),
            after.pointer(pointer),
            "association changed {pointer}"
        );
    }
    let fresh = world.reading("run", &bounded);
    assert_eq!(fresh["revision"], run_before["revision"]);
    assert_eq!(
        fresh["provenance"]["factoryStateRevision"]
            .as_u64()
            .unwrap(),
        run_before["provenance"]["factoryStateRevision"]
            .as_u64()
            .unwrap()
            + 1
    );
    assert_eq!(
        associations(&fresh),
        vec![request["mutation"]["association"].clone()]
    );
    assert!(associations(&world.reading("run", &world.run)).is_empty());
    let journey_after = world.reading("journey", &world.journey);
    assert_eq!(
        journey_after["revision"].as_u64().unwrap(),
        journey_before["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(journey_after["flowRefs"], json!([flow.location["ref"]]));
    assert_eq!(
        journey_after["activityRefs"],
        journey_before["activityRefs"]
    );
    assert!(journey_after["activityRefs"]
        .as_array()
        .unwrap()
        .contains(&world.external["location"]["ref"]));
    let commissioned = world.reading("commission-read", COMMISSION);
    assert_eq!(commissioned["commission"]["runRef"], world.run);
    let owner = read_developmental_state(&world.state).unwrap();
    assert_eq!(
        owner
            .workflow_sources
            .iter()
            .find(|s| s.source.reference == world.source.source.reference)
            .unwrap(),
        &world.source
    );
    assert_unfinished(&world, &world.run);
    assert_unfinished(&world, &bounded);
    assert_eq!(world.attempts(&world.run)["sourceCurrent"], true);
    assert_eq!(world.attempts(&bounded)["sourceCurrent"], true);
    world.capture_phase("retained", &bounded, &world.run, &flow);
    let canonical = world.bytes();
    let advanced = world.advance_flow(&flow);
    assert_eq!(advanced.reading["document_id"], flow.reading["document_id"]);
    assert_ne!(advanced.reading["revision"], flow.reading["revision"]);
    assert_eq!(world.bytes(), canonical);
    world.capture_phase("changed", &bounded, &world.run, &advanced);
    let different = world.replace_document_at_same_location(&advanced);
    assert_eq!(different.location, flow.location);
    assert_ne!(
        different.reading["document_id"],
        flow.reading["document_id"]
    );
    assert_eq!(world.bytes(), canonical);
    world.capture_phase("replaced", &bounded, &world.run, &different);
    assert_eq!(
        associations(&world.reading("run", &bounded)),
        vec![request["mutation"]["association"].clone()]
    );
    assert_unfinished(&world, &bounded);
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn exact_replay_after_actual_flow_change_and_removal_preserves_captured_basis_and_bytes() {
    let world = World::new();
    let flow = world.flow("replayed-flow.html");
    let request = world.association("associate-original", &world.run, &world.source, &flow);
    world.mutate(&request);
    let admitted = world.bytes();
    let changed = world.advance_flow(&flow);
    assert_ne!(changed.reading["revision"], flow.reading["revision"]);
    assert_eq!(world.mutate(&request)["status"], "already-applied");
    assert_eq!(world.bytes(), admitted);
    let original_path = world.root.join(flow.location["path"].as_str().unwrap());
    let removed = world.root.join("retained-flow-after-removal.html");
    fs::rename(&original_path, &removed).unwrap();
    let unavailable = ctrl_output(
        &world.ctrl,
        &world.root,
        "central.flow.read",
        &json!({"location":flow.location}),
    );
    let unavailable: Value = serde_json::from_slice(&unavailable.stdout).unwrap();
    assert_eq!(unavailable["ok"], false);
    assert_eq!(world.mutate(&request)["status"], "already-applied");
    assert_eq!(world.bytes(), admitted);
    assert_eq!(
        associations(&world.reading("run", &world.run)),
        vec![request["mutation"]["association"].clone()]
    );
    let mut conflict = request.clone();
    conflict["mutation"]["association"]["basisRefs"] =
        json!(["source:changed-under-same-occurrence"]);
    world.refused(&conflict, "replay");
    assert_unfinished(&world, &world.run);
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn actual_native_document_and_factory_basis_fences_refuse_without_canonical_writes() {
    let world = World::new();
    let flow = world.flow("exact-flow.html");
    let other = world.flow("other-real-flow.html");
    let request = world.association("exact-basis", &world.run, &world.source, &flow);
    for (field, value) in [
        ("documentId", other.reading["document_id"].clone()),
        (
            "documentRevision",
            json!(flow.reading["document_revision"].as_u64().unwrap() + 1),
        ),
        ("location", other.location.clone()),
    ] {
        let mut wrong = request.clone();
        wrong["mutation"]["association"]["flow"][field] = value;
        if field == "location" {
            wrong["source"]["reference"] = other.location["ref"].clone();
        }
        world.refused(&wrong, "Central Flow read");
    }
    let mut wrong = request.clone();
    wrong["source"]["revision"] = json!("wrong-observed-revision");
    wrong["mutation"]["association"]["flow"]["sourceRevision"] =
        wrong["source"]["revision"].clone();
    world.refused(&wrong, "Central Flow read");
    for field in [
        "expectedProviderRevision",
        "expectedJourneyRevision",
        "expectedRunRevision",
    ] {
        let mut wrong = request.clone();
        wrong["mutation"][field] = json!(request["mutation"][field].as_u64().unwrap() + 1);
        world.refused(&wrong, "stale");
    }
    for (field, value) in [
        (
            "workflowSourceRef",
            json!("workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAX"),
        ),
        ("workflowSourceRevision", json!("foreign-source-revision")),
        ("workflowSourceDigest", json!("0".repeat(64))),
    ] {
        let mut wrong = request.clone();
        wrong["mutation"]["association"][field] = value;
        world.refused(&wrong, "unrelated");
    }
    let foreign = World::new();
    let foreign_flow = foreign.flow("foreign-root-flow.html");
    let mut wrong = request.clone();
    wrong["mutation"]["association"]["flow"] = json!({"location":foreign_flow.location,"documentId":foreign_flow.reading["document_id"],
        "documentRevision":foreign_flow.reading["document_revision"],"sourceRevision":foreign_flow.reading["revision"]});
    wrong["source"]["reference"] = foreign_flow.location["ref"].clone();
    wrong["source"]["revision"] = foreign_flow.reading["revision"].clone();
    world.refused(&wrong, "another configured Central root");
    wrong = request.clone();
    wrong["mutation"]["association"]["runRef"] = json!(foreign.run);
    world.refused(&wrong, "Run");
    wrong = request.clone();
    wrong["mutation"]["association"]["journeyRef"] = json!(foreign.journey);
    world.refused(&wrong, "stored");
    let changed = world.advance_flow(&flow);
    assert_ne!(changed.reading["revision"], flow.reading["revision"]);
    world.refused(&request, "Central Flow read");
    assert_unfinished(&world, &world.run);
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn deserialized_pure_state_has_no_native_admission_even_with_correct_owner_labels() {
    let world = World::new();
    let flow = world.flow("pure-state-refusal.html");
    let request = world.association("unadmitted-pure-request", &world.run, &world.source, &flow);
    let parsed: FactoryDevelopmentalMutationRequest =
        serde_json::from_value(request.clone()).unwrap();
    let mut owner = read_developmental_state(&world.state).unwrap();
    let before = serde_json::to_value(&owner).unwrap();
    let error = owner
        .apply_developmental_mutation(parsed)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("actual configured native Central reading"),
        "{error}"
    );
    assert_eq!(serde_json::to_value(&owner).unwrap(), before);
    for field in ["nativeAdmission", "verified", "humanAdopted"] {
        let mut invented = request.clone();
        invented["mutation"][field] = json!(true);
        assert!(
            serde_json::from_value::<FactoryDevelopmentalMutationRequest>(invented.clone())
                .is_err()
        );
        world.refused(&invented, "unknown field");
    }
    // The same actual request is admitted only through the real public file
    // provider's native read. No test double injects the private witness.
    world.mutate(&request);
    assert_unfinished(&world, &world.run);
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn actual_concurrent_clients_commit_one_relation_and_restart_without_overwrite() {
    let world = World::new();
    let first = world.flow("concurrent-first.html");
    let second = world.flow("concurrent-second.html");
    let requests = [
        world.association("concurrent-first", &world.run, &world.source, &first),
        world.association("concurrent-second", &world.run, &world.source, &second),
    ];
    let initial_revision = world.reading("run", &world.run)["provenance"]["factoryStateRevision"]
        .as_u64()
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let threads = requests
        .iter()
        .map(|request| {
            let request = request.clone();
            let binary = world.ctrl.clone();
            let root = world.root.clone();
            let state = world.state.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                factory_command(
                    &binary,
                    &root,
                    &[
                        "development",
                        "mutate",
                        state.to_str().unwrap(),
                        "-",
                        "--json",
                    ],
                    Some(&request),
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let outputs = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outputs.iter().filter(|o| o.status.success()).count(), 1);
    let winner = outputs.iter().position(|o| o.status.success()).unwrap();
    let loser = 1 - winner;
    let error = String::from_utf8_lossy(&outputs[loser].stderr);
    assert!(
        error.contains("stale") || error.contains("already has an explicit Flow association"),
        "{error}"
    );
    let reading = world.reading("run", &world.run);
    assert_eq!(
        associations(&reading),
        vec![requests[winner]["mutation"]["association"].clone()]
    );
    assert_eq!(
        reading["provenance"]["factoryStateRevision"]
            .as_u64()
            .unwrap(),
        initial_revision + 1
    );
    let settled = world.bytes();
    assert_eq!(world.mutate(&requests[winner])["status"], "already-applied");
    assert_eq!(world.bytes(), settled);
    let selected = if loser == 0 { &first } else { &second };
    let fresh_loser = world.association(
        "explicit-replacement-refused",
        &world.run,
        &world.source,
        selected,
    );
    world.refused(&fresh_loser, "already has an explicit Flow association");
    assert_unfinished(&world, &world.run);
}

#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn superseded_source_refuses_new_relation_while_retained_native_relation_remains_readable() {
    let world = World::new();
    let flow = world.flow("historical-relation.html");
    let request = world.association("historical-association", &world.run, &world.source, &flow);
    world.mutate(&request);
    let (successor, _) = world.continuation("superseding-source");
    let fresh_attempt =
        world.association("old-source-new-relation", &world.run, &world.source, &flow);
    world.refused(&fresh_attempt, "stale");
    assert_eq!(
        associations(&world.reading("run", &world.run)),
        vec![request["mutation"]["association"].clone()]
    );
    assert!(associations(&world.reading("run", &successor)).is_empty());
    let before = world.bytes();
    assert_eq!(world.mutate(&request)["status"], "already-applied");
    assert_eq!(world.bytes(), before);
    assert_eq!(world.attempts(&world.run)["sourceCurrent"], false);
    assert_unfinished(&world, &world.run);
    assert_unfinished(&world, &successor);
}

#[cfg(unix)]
#[test]
#[ignore = "requires actual installed ctrl and shipped v0.4 Flow form; no native fallback"]
fn missing_and_symlinked_native_flow_objects_cannot_be_newly_associated() {
    use std::os::unix::fs::symlink;
    let world = World::new();
    let flow = world.flow("source-object-fences.html");
    let request = world.association("source-object-fences", &world.run, &world.source, &flow);
    let source = world.root.join(flow.location["path"].as_str().unwrap());
    let retained = world.root.join("retained-original-flow.html");
    fs::rename(&source, &retained).unwrap();
    world.refused(&request, "Central did not confirm central.flow.read");
    symlink(&retained, &source).unwrap();
    world.refused(&request, "Central did not confirm central.flow.read");
    fs::remove_file(&source).unwrap();
    fs::rename(&retained, &source).unwrap();
    // Restoring the real source admits the actual original query basis; no
    // caller-supplied success or replacement identity can bypass the owner.
    world.mutate(&request);
    assert_eq!(
        associations(&world.reading("run", &world.run)),
        vec![request["mutation"]["association"].clone()]
    );
    assert_unfinished(&world, &world.run);
}
