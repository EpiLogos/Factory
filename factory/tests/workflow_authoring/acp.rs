//! Controlled process evidence through Factory -> AIKit -> native Actuation ->
//! ACP, never a live model. Configuration is made through public owner verbs.
//! This read-only floor does not claim Workcell write confinement or human craft.
use super::*;
use epilogos_factory::native_owner::AIKIT_CAW_CONTRACT_REVISION;
use epilogos_factory::workflow_inputs::SelectedWorkflowInput;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};

fn native_binary(key: &str) -> PathBuf {
    let p=PathBuf::from(std::env::var_os(key).unwrap_or_else(|| panic!("BLOCKED: {key} must bind a source-verified native executable; no fallback or skipped success")));
    assert!(
        p.is_absolute() && p.is_file(),
        "BLOCKED: {key} is not an absolute native executable"
    );
    p
}
struct Native {
    work: World,
    aikit: PathBuf,
    actuation: PathBuf,
    home: PathBuf,
    source: PathBuf,
    child: Option<Child>,
    bindings: std::collections::BTreeMap<String, Value>,
}
impl Native {
    fn new(plural: bool) -> Self {
        let mut d = if plural {
            chain_definition()
        } else {
            definition()
        };
        for u in d["units"].as_array_mut().unwrap() {
            let key = u["key"].as_str().unwrap().to_owned();
            u["agentRequirements"]["agentRefs"] = json!([format!("agent:{key}")]);
            u["subjectRef"] = json!("source:controlled-acp-input");
            u["basisRevision"] = json!("fixture-source-v1");
            u["praxisRefs"] = json!([format!("skill/{key}")]);
            u["capabilityRefs"] = json!(["capability/source-read"]);
            u["permittedEffects"] = json!(["read exact controlled source"]);
            u["verificationObligations"] =
                json!(["exact source bytes and attributable native tool result"]);
        }
        let work = World::from_definition(d);
        let home = work.dir.path().join("isolated-home");
        fs::create_dir_all(&home).unwrap();
        let source = work.dir.path().join("input.txt");
        fs::write(
            &source,
            "CONTROLLED_SOURCE_CONTENT\nNot a provider or creative-quality proof.\n",
        )
        .unwrap();
        Self {
            work,
            aikit: native_binary("FACTORY_NATIVE_AIKIT_BIN"),
            actuation: native_binary("FACTORY_NATIVE_ACTUATION_BIN"),
            home,
            source,
            child: None,
            bindings: Default::default(),
        }
    }
    fn process(&self, binary: &Path) -> Command {
        let mut p = Command::new(binary);
        p.env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("AIKIT_HOME", self.home.join("aikit"))
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join(".local/share"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("LANG", "C.UTF-8")
            .env("TZ", "UTC")
            .current_dir(self.work.dir.path());
        p
    }
    fn cli(&self, args: &[String]) -> Value {
        success(
            self.process(&self.aikit)
                .arg("-C")
                .arg(self.work.dir.path())
                .arg("session-space")
                .args(args)
                .output()
                .unwrap(),
        )
    }
    fn request(&self, value: Value) -> Value {
        let v = self.cli(&[
            "encounter".into(),
            "--request-json".into(),
            value.to_string(),
        ]);
        assert_eq!(v["ok"], true, "native request failed: {v}");
        v["data"].clone()
    }
    fn attach(&mut self, key: &str, mode: &str) {
        let space = format!("session-space/{key}");
        let session = format!("agent-session/{key}");
        let preview = self.cli(&["create".into(), space.clone()]);
        self.cli(&["apply".into(), "--preview-json".into(), preview.to_string()]);
        let preview=self.cli(&["stage".into(),"--space".into(),space,"--intent-json".into(),json!({"operation":"attach-agent-session","attachment":{"agent_session":session,"purpose":"Controlled Factory native ACP test","provenance":["controlled-protocol-evidence-only"]}}).to_string()]);
        self.cli(&["apply".into(), "--preview-json".into(), preview.to_string()]);
        let mut src: Value = serde_json::from_str(include_str!("agency-request.json")).unwrap();
        src["differentiated_binding"]["agent_ref"] = json!(format!("agent:{key}"));
        src["differentiated_binding"]["agency_ref"] = json!(format!("agency:{key}"));
        src["differentiated_binding"]["binding_ref"] = json!(format!("binding:{key}"));
        src["determination"]["differentiated_agency_ref"] = json!(format!("agency:{key}"));
        src["determination"]["world_binding_ref"] = json!(format!("binding:{key}"));
        let bytes = serde_json::to_vec(&src).unwrap();
        let path = self.work.dir.path().join(format!("{key}-agency.json"));
        fs::write(&path, &bytes).unwrap();
        let context_path = self.work.dir.path().join(format!("{key}-operative.md"));
        let text=format!("OPERATIVE_ROLE_{key}\nRead only the selected source; return exact bytes/hash and your own native identity. No private parent context, source mutation or publication.\n");
        fs::write(&context_path, &text).unwrap();
        let binding = json!({"revision":"rev/1","active":true,"agent_ref":format!("agent:{key}"),"agency_ref":format!("agency:{key}"),"world_ref":"central:project:Example","world_binding_ref":format!("binding:{key}"),
            "agency_source":{"source_ref":format!("source/{key}"),"revision":"rev/native-1","path":path,"content_digest":format!("blake3:{}",blake3::hash(&bytes))},
            "actuation_bin":self.actuation,"allowed_senders":["caller:controlled-test"],"allowed_packet_sources":["source/controlled-task"],
            "context":{"sources":[{"source":format!("skill/{key}"),"revision":"rev/context-1","path":context_path,"content_digest":format!("blake3:{}",blake3::hash(text.as_bytes()))}],"source_activations":[]}});
        self.cli(&[
            "encounter-agency-configure".into(),
            "--agent-session".into(),
            session,
            "--binding-json".into(),
            binding.to_string(),
        ]);
        self.bindings.insert(key.into(), binding);
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/workflow_authoring/controlled_acp.py");
        let python = native_binary("FACTORY_CONTROLLED_PYTHON_BIN");
        self.cli(&["encounter-configure".into(),"--provider-json".into(),json!({"protocol":"acp","id":key,"label":"Controlled ACP process, not a live model","argv":[python,"-u",fixture,key,mode,self.source,self.work.dir.path().join(format!("{key}-protocol.jsonl"))]}).to_string()]);
    }
    fn serve(&mut self) {
        self.child = Some(
            self.process(&self.aikit)
                .arg("-C")
                .arg(self.work.dir.path())
                .arg("session-space")
                .arg("encounter-serve")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(
                    fs::File::create(self.work.dir.path().join("owner.stderr")).unwrap(),
                ))
                .spawn()
                .unwrap(),
        );
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            let o = self
                .process(&self.aikit)
                .arg("-C")
                .arg(self.work.dir.path())
                .arg("session-space")
                .args(["encounter", "--request-json", r#"{"action":"health"}"#])
                .output()
                .unwrap();
            if o.status.success()
                && serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"] == true
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "native owner did not start: {}",
                fs::read_to_string(self.work.dir.path().join("owner.stderr")).unwrap()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn open(&self, key: &str) {
        self.request(json!({"action":"open","space":format!("session-space/{key}"),"agent_session":format!("agent-session/{key}"),"provider":key,"cwd":self.work.dir.path()}));
    }
    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let mut d = self.work.disposition(key);
        let b = &self.bindings[key];
        d.participant.source_ref = b["agency_source"]["source_ref"].as_str().unwrap().into();
        d.participant.source_revision = b["agency_source"]["revision"].as_str().unwrap().into();
        d.participant.source_digest = b["agency_source"]["content_digest"]
            .as_str()
            .unwrap()
            .into();
        d.body.agent_session_ref = format!("agent-session/{key}");
        d.body.session_space_ref = format!("session-space/{key}");
        d.context_refs.insert(format!("skill/{key}"));
        d.budget.wall_clock_timeout_ms = Some(10000);
        d
    }
    fn start(&self, key: &str) {
        let mut op = self.work.start_op(&format!("attempt:{key}"), key);
        if let FactoryAttemptOperation::StartSerial { disposition, .. } = &mut op {
            *disposition = self.disposition(key);
        }
        success(self.work.action(op));
    }
    fn owner(&self, key: &str, action: &str, serial: usize) -> Value {
        let r = self.work.reading();
        let packet = if action == "send" {
            json!({"action":"send","agent_session":format!("agent-session/{key}"),"turn":{"delivery_ref":format!("delivery/{key}"),"sender":"caller:controlled-test","expected_binding_revision":"rev/1","packet":{"text":"Read the bounded source and report attributable tool evidence. Do not fabricate success or mutate source.","source_refs":["source/controlled-task"],"audience":[format!("agent:{key}")]}}})
        } else {
            json!({"action":"delivery","agent_session":format!("agent-session/{key}"),"delivery_ref":format!("delivery/{key}")})
        };
        let input = json!({"contract":"factory.attempt-owner-action/v1","requestRef":format!("native:{key}:{action}:{serial}"),"projectionRef":"projection:controlled-native","caller":{"callerRef":"caller:controlled-test","projectionKind":"headless","lineage":["caller:controlled-test"]},"runRef":self.work.run,"expectedRevision":r.revision,"authority":{"authorityRef":"authority:controlled-test","nativeOwner":"factory","capabilityRef":FACTORY_ATTEMPT_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true},"attemptRef":format!("attempt:{key}"),"executionRef":format!("execution:native-{key}"),"invocation":{"operation":"aikit-encounter","binary":self.aikit,"cwd":self.work.dir.path(),"contract_revision":AIKIT_CAW_CONTRACT_REVISION,"request":packet}});
        let mut p = self
            .process(Path::new(env!("CARGO_BIN_EXE_factory")))
            .args(["attempt", "owner-action"])
            .arg(self.work.path())
            .args(["-", "--json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        p.stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        success(p.wait_with_output().unwrap())
    }
    fn wait(&self, key: &str) -> Value {
        let until = Instant::now() + Duration::from_secs(10);
        let mut n = 1;
        loop {
            let v = self.owner(key, "delivery", n);
            n += 1;
            if matches!(
                v["ownerReceipt"]["phase"].as_str(),
                Some("returned" | "failed" | "cancelled")
            ) {
                return v;
            }
            assert!(
                Instant::now() < until,
                "native delivery did not settle: {v}"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    fn events(&self, key: &str) -> Vec<Value> {
        let mut after = 0;
        let mut events = vec![];
        for _ in 0..128 {
            let p=self.request(json!({"action":"read","agent_session":format!("agent-session/{key}"),"after":after,"limit":3}));
            let next = p["next_cursor"].as_u64().unwrap();
            events.extend(p["events"].as_array().unwrap().iter().cloned());
            if p["more"] == false {
                return events;
            }
            assert!(next > after, "stalled native cursor");
            after = next;
        }
        panic!("native evidence overflow")
    }
    fn evidence(&self, key: &str) -> Result<Value, String> {
        let events = self.events(key);
        let mut reports = vec![];
        let mut tool_calls = 0;
        for event in &events {
            let signal = &event["event"]["event"]["Signal"];
            if signal["kind"]["kind"] == "tool-call" {
                tool_calls += 1;
            }
            if signal["kind"]["kind"] == "agent-message-chunk" {
                if let Some(text) = signal["kind"]["text"].as_str() {
                    if let Ok(v) = serde_json::from_str::<Value>(text) {
                        reports.push(v);
                    }
                }
            }
        }
        if reports.len() != 1 {
            return Err(format!(
                "exact independent report absent: {}",
                json!(events)
            ));
        }
        let r = &reports[0];
        let work = self.work.reading();
        let expected_hash = format!("{:x}", Sha256::digest(fs::read(&self.source).unwrap()));
        if tool_calls == 0
            || r["standing"] != "controlled-acp-not-model"
            || r["agentRef"] != format!("agent:{key}")
            || r["attemptRef"] != format!("attempt:{key}")
            || r["executionRef"] != format!("execution:native-{key}")
            || r["runRef"] != self.work.run.to_string()
            || r["unitRef"] != self.work.workflow.unit(key).unwrap().reference.to_string()
            || r["workflowBasis"]["ref"] != work.workflow_source_ref
            || r["workflowBasis"]["revision"] != work.workflow_source_revision
            || r["workflowBasis"]["digest"] != work.workflow_source_digest
            || r["childRoleLoaded"] != true
            || r["sourceSha256"] != expected_hash
            || r["pid"].as_u64().is_none()
            || r["toolCallId"] != format!("tool:attempt:{key}")
        {
            return Err(format!(
                "native evidence failed exact source/attempt/context/tool predicate: {r}"
            ));
        }
        let current = work
            .attempts
            .iter()
            .find(|a| a.attempt_ref == format!("attempt:{key}"))
            .unwrap();
        if r["selectedInputs"] != json!(current.disposition.selected_inputs) {
            return Err("child did not receive exact selected inputs".into());
        }
        Ok(r.clone())
    }
    fn retain_return(&self, key: &str) -> Value {
        let report = self.evidence(key).expect(
            "independent native readback must pass before recording verification or Return",
        );
        let id = format!("attempt:{key}");
        let u = self.work.workflow.unit(key).unwrap();
        let ref_id = format!(
            "evidence:sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&report).unwrap())
        );
        fs::write(
            self.work.dir.path().join(format!("{key}-verified.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        success(
            self.work
                .action(FactoryAttemptOperation::RecordVerification {
                    attempt_ref: id.clone(),
                    verification: VerificationReceipt {
                        verification_ref: format!("verification:{key}"),
                        owner_ref: "verifier:deterministic-native-readback".into(),
                        source_revision: format!(
                            "sha256:{:x}",
                            Sha256::digest(include_bytes!("acp.rs"))
                        ),
                        outcome: VerificationOutcome::Passed,
                        obligations: u.verification_obligations.clone(),
                        evidence_refs: [ref_id.clone()].into(),
                    },
                }),
        );
        success(self.work.action(FactoryAttemptOperation::ReturnArtifact{attempt_ref:id,artifact:ReturnedArtifact{artifact_ref:format!("artifact:{key}"),subject_ref:u.subject_ref.to_string(),subject_revision:u.basis_revision.clone(),producing_execution_ref:format!("execution:native-{key}"),evidence_refs:[ref_id.clone()].into(),semantic_difference:format!("Native controlled source read verified at {}",report["sourceSha256"])},readable_return:ReadableReturn{return_ref:format!("return:{key}"),summary:"Controlled ACP source read; model efficacy and human acceptance not inferred".into(),artifact_refs:[format!("artifact:{key}")].into(),evidence_refs:[ref_id].into(),receiving_ref:None,receiving_source_revision:None,archive_refs:Default::default(),regression_observation_refs:Default::default()}}));
        report
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = self
                .process(&self.aikit)
                .arg("-C")
                .arg(self.work.dir.path())
                .args(["encounter", "--request-json"])
                .arg(json!({"action":"shutdown","expected_pid":child.id()}).to_string())
                .output();
            let until = Instant::now() + Duration::from_secs(4);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() > until {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
#[ignore = "requires verified native AIKit/Actuation binaries; explicitly run in joined CI"]
fn native_acp_public_source_dispatch_tool_return_is_not_a_direct_chat_or_fake_receipt() {
    let mut n = Native::new(false);
    let key = "inspect-source";
    n.attach(key, "normal");
    n.serve();
    n.open(key);
    n.start(key);
    assert!(n.work.reading().attempts[0].dispatch.is_none());
    n.owner(key, "send", 0);
    let v = n.wait(key);
    assert_eq!(
        v["ownerReceipt"]["phase"],
        "returned",
        "{}",
        json!({"receipt":v,"events":n.events(key)})
    );
    let r = n.retain_return(key);
    assert!(r["pid"].as_u64().unwrap() > 0);
    let selected = success(n.work.inspect(&["--attempt", "attempt:inspect-source"]));
    assert_eq!(selected["attempts"].as_array().unwrap().len(), 1);
    assert!(n.work.reading().attempts[0].readable_return.is_some());
    println!("FACTORY_NATIVE_ACP_TOOL_RETURN_EXECUTED");
}

#[test]
#[ignore = "requires verified native AIKit/Actuation binaries; explicitly run in joined CI"]
fn native_acp_verifier_rejects_wrong_source_attempt_unloaded_context_and_noop_completion() {
    for mode in ["wrong-source", "wrong-attempt", "unloaded", "noop"] {
        let mut n = Native::new(false);
        let key = "inspect-source";
        n.attach(key, mode);
        n.serve();
        n.open(key);
        n.start(key);
        n.owner(key, "send", 0);
        assert_eq!(n.wait(key)["ownerReceipt"]["phase"], "returned");
        assert!(n.evidence(key).is_err(), "{mode} falsely verified");
        assert!(n.work.reading().attempts[0].readable_return.is_none());
    }
    println!("FACTORY_NATIVE_ACP_FALSE_COMPLETION_REFUSED");
}

#[test]
#[ignore = "requires verified native AIKit/Actuation binaries; explicitly run in joined CI"]
fn native_acp_plural_agents_receive_selected_returns_and_independently_synthesize() {
    let mut n = Native::new(true);
    for key in ["read-left", "read-right", "verify"] {
        n.attach(key, "normal");
    }
    n.serve();
    for key in ["read-left", "read-right", "verify"] {
        n.open(key);
    }
    let attempts = ["read-left", "read-right"].map(|key| {
        let mut a = n.work.fork_start(&format!("attempt:{key}"), key);
        a.disposition = n.disposition(key);
        a
    });
    success(n.work.action(FactoryAttemptOperation::StartFork {
        parent_journey_ref: n.work.journey.clone(),
        attempts: attempts.into(),
    }));
    for key in ["read-left", "read-right"] {
        n.owner(key, "send", 0);
    }
    let mut pids = BTreeSet::new();
    for key in ["read-right", "read-left"] {
        n.wait(key);
        pids.insert(n.retain_return(key)["pid"].as_u64().unwrap());
    }
    let reading = n.work.reading();
    let mut op = n.work.start_op("attempt:verify", "verify");
    if let FactoryAttemptOperation::StartSerial { disposition, .. } = &mut op {
        *disposition = n.disposition("verify");
        disposition.selection.demand.independence_from =
            set(["execution:native-read-left", "execution:native-read-right"]);
        for input in &n.work.workflow.unit("verify").unwrap().inputs {
            let leg = &reading.legs[&input.predecessor];
            disposition
                .context_refs
                .insert(input.receiving_context_ref.clone());
            disposition.selected_inputs.push(SelectedWorkflowInput {
                predecessor: input.predecessor.clone(),
                receiving_context_ref: input.receiving_context_ref.clone(),
                execution_ref: leg.execution_ref.clone(),
                artifacts: leg.artifacts.clone(),
            });
        }
    }
    success(n.work.action(op));
    n.owner("verify", "send", 0);
    n.wait("verify");
    pids.insert(n.retain_return("verify")["pid"].as_u64().unwrap());
    assert_eq!(
        pids.len(),
        3,
        "one process impersonating a cast is not plural proof"
    );
    success(
        n.work
            .action(FactoryAttemptOperation::RegisterIndependentReview {
                attempt_ref: "attempt:verify".into(),
                review_of: ["read-left", "read-right"]
                    .iter()
                    .map(|k| n.work.workflow.unit(k).unwrap().reference.clone())
                    .collect(),
            }),
    );
    success(n.work.action(FactoryAttemptOperation::Synthesize {
        attempt_ref: "attempt:verify".into(),
        reviewer_attempt_ref: "attempt:verify".into(),
        synthesis_ref: "synthesis:native-acp".into(),
        barrier_key: "both-readings".into(),
        result_artifact_ref: "artifact:verify".into(),
        artifact_refs: set(["artifact:read-left", "artifact:read-right"]),
    }));
    assert_eq!(n.work.reading().syntheses.len(), 1);
    println!("FACTORY_NATIVE_ACP_PLURAL_REVIEW_EXECUTED");
}
