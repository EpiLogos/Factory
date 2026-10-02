//! Uses actual compiled Workcell CLI/control-service binaries. The service it
//! hosts is a disposable TCP workload, not an Agent or a commercial model.
use super::*;
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::Child;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc,
};

const TOKEN: &str = "factory-material-native-test-token-not-a-user-secret";
const NATIVE_ATTEMPT: &str = "attempt:native-material";

fn port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn reachable(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(100),
    )
    .is_ok()
}

fn await_port(port: u16, expected: bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while reachable(port) != expected && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(reachable(port), expected, "native service port {port}");
}

struct Host(Option<Child>);
impl Host {
    fn launch(binary: &Path, root: &Path, endpoint: &str) -> Self {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("host.log"))
            .unwrap();
        let child = Command::new(binary)
            .args([
                "--state-root",
                root.to_str().unwrap(),
                "--workcell-ref",
                WORKCELL,
            ])
            .args(["--listen", endpoint])
            .env("WORKCELL_CONTROL_TOKEN", TOKEN)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut host = Self(Some(child));
        let port = endpoint.rsplit(':').next().unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !reachable(port) && Instant::now() < deadline {
            assert!(
                host.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "native Workcell exited: {}",
                fs::read_to_string(root.join("host.log")).unwrap()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(reachable(port), "native Workcell did not listen");
        host
    }
    fn try_stop(&mut self) -> io::Result<()> {
        if let Some(child) = self.0.as_mut() {
            if child.try_wait()?.is_none() {
                // This is the exact native owner Child spawned above, not a
                // process rediscovered by PID or a shared installed service.
                child.kill()?;
            }
            child.wait()?;
        }
        self.0.take();
        Ok(())
    }
    fn stop(&mut self) {
        self.try_stop()
            .expect("owned native host shutdown and wait");
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Err(error) = self.try_stop() {
            // Retain the original test failure and let evidence/TempDir drops
            // run. Explicit successful finalization still requires stop().
            eprintln!("secondary native host cleanup failure: {error}");
        }
    }
}

fn native_call(world: &World, request: &FactoryAttemptOwnerRequest) -> Output {
    let home = world.dir.path().join("isolated-client-home");
    fs::create_dir_all(&home).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(["attempt", "material", &world.state(), "-", "--json"])
        .env("WORKCELL_CONTROL_TOKEN", TOKEN)
        .env("HOME", &home)
        .env("XDG_STATE_HOME", home.join("state"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires actual Workcell binaries and retained evidence directory; required Factory material CI executes it"]
fn native_workcell_caller_retains_process_recovery_and_release() {
    let bin = PathBuf::from(
        std::env::var("FACTORY_TEST_WORKCELL_BIN_DIR")
            .expect("native Workcell source-build directory is mandatory for this case"),
    );
    let workcell = bin.join("workcell");
    let service = bin.join("workcell-control-service");
    assert!(workcell.is_file() && service.is_file());
    let world = World::new();
    let mut evidence = RetainNativeMaterial::new(world.dir.path(), "process-recovery");
    let root = world.dir.path();
    let state = root.join("native-owner-state");
    let now = root.join("native-NOW");
    let client = root.join("native-client-state");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&now).unwrap();
    fs::create_dir_all(&client).unwrap();
    let source = root.join("human-source.txt");
    fs::write(&source, b"human source stays byte-identical\0").unwrap();
    fs::write(now.join("pending-return.txt"), b"retained pending Return\0").unwrap();
    let workload_port = port();
    let stop_marker = root.join("stop-native-workload");
    let control_port = port();
    let endpoint = format!("127.0.0.1:{control_port}");
    let python = Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap().trim().to_owned();
    let declarations = json!({
        "schema":"workcell.service-declaration/v1",
        "services":[{
            "logical_ref":"service:factory-material-test",
            "endpoint":format!("tcp://127.0.0.1:{workload_port}"),
            "lifetime":"provider-process-scoped","program":python,
            "args":["-S","-c","import json,os,pathlib,socket,sys,time; p=pathlib.Path(sys.argv[2]); p.write_bytes(b'native workload partial bytes\\0') if not p.exists() else None; stop=pathlib.Path(sys.argv[3]); lifetime=pathlib.Path(sys.argv[4]); lifetime.open('a').write(json.dumps({'event':'started','pid':os.getpid(),'port':int(sys.argv[1])})+'\\n'); s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',int(sys.argv[1]))); s.listen(); s.settimeout(.025); deadline=time.monotonic()+300\nwhile not stop.exists() and time.monotonic()<deadline:\n try: connection,address=s.accept(); connection.close()\n except TimeoutError: pass\ns.close(); lifetime.open('a').write(json.dumps({'event':'self-stopped','pid':os.getpid(),'marker':stop.exists()})+'\\n')",workload_port.to_string(),now.join("partial-work.txt"),stop_marker,now.join("workload-lifetime.jsonl")],
            "cwd":now,"readiness":{"host":"127.0.0.1","port":workload_port,"timeout_ms":5000}
        }]
    });
    fs::write(state.join("services.json"), declarations.to_string()).unwrap();
    fs::write(
        state.join("storage.json"),
        json!({"schema":"workcell.directory-storage/v1","directories":[{"logical_ref":"now:factory-material-test","path":now}]}).to_string(),
    )
    .unwrap();
    let tiers = json!({"required":[],"preferred":[],"optional":[]});
    let demand = json!({
        "demand_ref":"demand:factory-material-native",
        "subjects":{"attempt":NATIVE_ATTEMPT,"run":RUN,"source":world.workflow.source.reference.to_string()},
        "affordances":tiers,"connectivity":{"required":["service:factory-material-test"],"preferred":[],"optional":[]},
        "exposure":tiers,"outputs":tiers,
        "storage":{"required":[{"logical_ref":"now:factory-material-test","access":"writable","sharing":"shared","minimum_capacity":null,"unit":null,"persistence":"external","retention":"preserve"}],"preferred":[],"optional":[]},
        "workspace":null,"project_runtime":null,"resources":[],"persistence":null,
        "isolation_trust":null,"retention":"release","extensions":{}
    });
    let demand_path = root.join("native-demand.json");
    fs::write(&demand_path, demand.to_string()).unwrap();
    let mut host = Host::launch(&service, &state, &endpoint);
    let _workload_cleanup = StopNativeWorkload {
        marker: stop_marker.clone(),
        port: workload_port,
    };
    let receipt_path = root.join("native-original-world.json");
    let prepared = success(
        Command::new(&workcell)
            .args([
                "--endpoint",
                &endpoint,
                "--state-root",
                client.to_str().unwrap(),
            ])
            .args([
                "--receipt",
                receipt_path.to_str().unwrap(),
                "--json",
                "prepare",
            ])
            .args(["--demand-json", demand_path.to_str().unwrap()])
            .env("WORKCELL_CONTROL_TOKEN", TOKEN)
            .output()
            .unwrap(),
    );
    let original = prepared["world"].clone();
    let original_ref = original["world_ref"].as_str().unwrap();
    let original_bytes = fs::read(&receipt_path).unwrap();
    await_port(workload_port, true);
    let mut disposition = world.disposition("peer-read");
    disposition.body.material_world_ref = Some(original_ref.into());
    success(world.action(FactoryAttemptOperation::StartSerial {
        attempt_ref: NATIVE_ATTEMPT.into(),
        task_ref: "task:native-material".into(),
        parent_journey_ref: "journey:test".into(),
        workflow_unit_ref: world.workflow.unit("peer-read").unwrap().reference.clone(),
        disposition,
        retry_grant: None,
        tracking: vec![],
        place_grant: None,
    }));
    let request = |key: &str, operation: WorkcellWorldOperation, receipt: &Path| {
        let mut request = world.request(key, operation);
        request.attempt_ref = NATIVE_ATTEMPT.into();
        request.execution_ref = format!("factory-attempt:{NATIVE_ATTEMPT}");
        request.invocation = NativeOwnerInvocation::WorkcellWorld {
            binary: workcell.clone(),
            receipt: receipt.into(),
            world_operation: operation,
            endpoint: Some(endpoint.clone()),
            authorization: None,
            contract_revision: WORKCELL_CAW_CONTRACT_REVISION.into(),
        };
        request
    };
    for (key, operation) in [
        ("native:inspect", WorkcellWorldOperation::Inspect),
        ("native:observe", WorkcellWorldOperation::Observe),
        ("native:expose", WorkcellWorldOperation::Expose),
        ("native:collect", WorkcellWorldOperation::Collect),
    ] {
        let response = success(native_call(&world, &request(key, operation, &receipt_path)));
        assert_eq!(response["needsReconciliation"], false, "{response}");
    }
    let partial = fs::read(now.join("partial-work.txt")).unwrap();
    let original_pid = managed_workload_pid(&original);
    #[cfg(target_os = "linux")]
    {
        // Exercise the real Linux parent-death contract without an exit marker.
        // A closed socket alone is not proof that the old process terminated.
        assert!(!stop_marker.exists());
        host.stop();
        await_port(workload_port, false);
        let old_process_state = await_linux_workload_stopped(original_pid);
        fs::write(
            root.join("native-lifetime-recovery.json"),
            json!({"mechanism":"linux-parent-death", "managed_pid":original_pid,
                "old_process_state":old_process_state, "marker_requested":false,
                "native_model_executed":false})
            .to_string(),
        )
        .unwrap();
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Mac has no PDEATHSIG contract: request this workload's own shutdown,
        // then make its still-owning native service observe and reap it.
        fs::write(&stop_marker, b"stop original owned workload").unwrap();
        await_port(workload_port, false);
        let stopped = await_native_workload_stopped(
            &workcell,
            &endpoint,
            &client,
            &receipt_path,
            original_pid,
        );
        fs::write(
            root.join("native-lifetime-recovery.json"),
            json!({"mechanism":"owned-marker-and-native-reap", "managed_pid":original_pid,
                "native_observation":stopped, "marker_requested":true,
                "native_model_executed":false})
            .to_string(),
        )
        .unwrap();
        host.stop();
        fs::remove_file(&stop_marker).unwrap();
    }
    host = Host::launch(&service, &state, &endpoint);
    let recover_request = request(
        "native:recover",
        WorkcellWorldOperation::Recover,
        &receipt_path,
    );
    let recovered = success(native_call(&world, &recover_request));
    assert_eq!(recovered["needsReconciliation"], false, "{recovered}");
    let successor = recovered["ownerReceipt"]["payload"]["world"].clone();
    assert_ne!(successor["world_ref"], original["world_ref"]);
    assert_eq!(successor["subjects"], original["subjects"]);
    assert_eq!(
        recovered["ownerReceipt"]["payload"]["previous_world_ref"],
        original_ref
    );
    await_port(workload_port, true);
    assert_eq!(fs::read(now.join("partial-work.txt")).unwrap(), partial);
    let pids = |value: &Value| {
        value["binding_graph"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|binding| binding["properties"]["pid"].as_str().map(str::to_owned))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(pids(&original).len(), 1);
    assert_eq!(pids(&successor).len(), 1);
    assert!(pids(&original).is_disjoint(&pids(&successor)));
    assert_eq!(
        success(native_call(&world, &recover_request))["replayed"],
        true
    );
    refused(
        native_call(
            &world,
            &request("native:old", WorkcellWorldOperation::Recover, &receipt_path),
        ),
        "superseded material",
    );
    let successor_path = root.join("native-successor-world.json");
    fs::write(&successor_path, successor.to_string()).unwrap();
    for operation in [
        FactoryAttemptOperation::RequestCancellation {
            attempt_ref: NATIVE_ATTEMPT.into(),
        },
        FactoryAttemptOperation::AcceptCancellation {
            attempt_ref: NATIVE_ATTEMPT.into(),
        },
        FactoryAttemptOperation::RecordProcessTermination {
            attempt_ref: NATIVE_ATTEMPT.into(),
        },
        FactoryAttemptOperation::MarkQuiescent {
            attempt_ref: NATIVE_ATTEMPT.into(),
        },
    ] {
        success(world.action(operation));
    }
    let released = success(native_call(
        &world,
        &request(
            "native:release",
            WorkcellWorldOperation::Release,
            &successor_path,
        ),
    ));
    assert_eq!(released["needsReconciliation"], false, "{released}");
    assert_eq!(
        released["ownerReceipt"]["payload"]["disposition"],
        "released"
    );
    await_port(workload_port, false);
    assert_eq!(fs::read(&receipt_path).unwrap(), original_bytes);
    assert_eq!(
        fs::read(&source).unwrap(),
        b"human source stays byte-identical\0"
    );
    assert_eq!(
        fs::read(now.join("pending-return.txt")).unwrap(),
        b"retained pending Return\0"
    );
    assert_eq!(
        world.calls(),
        0,
        "the protocol double must never run in this native case"
    );
    let reading = world.read();
    let attempt = reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == NATIVE_ATTEMPT)
        .unwrap();
    assert_eq!(
        attempt.disposition.body.material_world_ref.as_deref(),
        Some(original_ref)
    );
    assert!(attempt.readable_return.is_none());
    assert!(!reading.completion_verified);
    assert_eq!(fs::read(now.join("partial-work.txt")).unwrap(), partial);
    assert!(!fs::read_to_string(world.state()).unwrap().contains(TOKEN));
    println!("native Workcell source {WORKCELL_CAW_CONTRACT_REVISION}; original={original_ref}; recovered={}; real managed child replaced and released; source/Return bytes preserved", successor["world_ref"]);
    host.stop();
    evidence.finish(&[
        "state.json",
        "native-original-world.json",
        "native-successor-world.json",
        "native-lifetime-recovery.json",
        "native-NOW/partial-work.txt",
        "native-NOW/pending-return.txt",
        "native-NOW/workload-lifetime.jsonl",
    ]);
}

// These are the actual Workcell control carrier's four-byte big-endian frames.
// The proxy never creates or rewrites an owner response.
fn native_control_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut prefix = [0u8; 4];
    stream.read_exact(&mut prefix)?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized actual control frame",
        ));
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}

struct DropActualRecoveryReply {
    endpoint: String,
    received: Arc<AtomicUsize>,
    forwarded: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<Value, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl DropActualRecoveryReply {
    fn launch(owner: &str, original: &Value, evidence: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let received = Arc::new(AtomicUsize::new(0));
        let forwarded = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, result) = mpsc::channel();
        let owner: SocketAddr = owner.parse().unwrap();
        let original = original.clone();
        let evidence = evidence.to_path_buf();
        let count = received.clone();
        let sent = forwarded.clone();
        let done = stop.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !done.load(Ordering::SeqCst) && Instant::now() < deadline {
                let mut client = match listener.accept() {
                    Ok((client, _)) => client,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        return;
                    }
                };
                let outcome = (|| -> Result<Value, String> {
                    client
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .map_err(|e| e.to_string())?;
                    let request_bytes =
                        native_control_frame(&mut client).map_err(|e| e.to_string())?;
                    let request: Value =
                        serde_json::from_slice(&request_bytes).map_err(|e| e.to_string())?;
                    if count.fetch_add(1, Ordering::SeqCst) != 0 {
                        return Err(
                            "unexpected second framed material request; no second effect forwarded"
                                .into(),
                        );
                    }
                    if request["version"] != "workcell.control/v1"
                        || request["operation"] != "recover"
                        || request["payload"]["world_ref"] != original["world_ref"]
                    {
                        return Err("actual native client sent another control basis".into());
                    }
                    let mut upstream = TcpStream::connect_timeout(&owner, Duration::from_secs(2))
                        .map_err(|e| e.to_string())?;
                    upstream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .map_err(|e| e.to_string())?;
                    upstream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .map_err(|e| e.to_string())?;
                    let length = u32::try_from(request_bytes.len()).map_err(|e| e.to_string())?;
                    upstream
                        .write_all(&length.to_be_bytes())
                        .and_then(|()| upstream.write_all(&request_bytes))
                        .and_then(|()| upstream.flush())
                        .map_err(|e| e.to_string())?;
                    sent.fetch_add(1, Ordering::SeqCst);
                    let response_bytes =
                        native_control_frame(&mut upstream).map_err(|e| e.to_string())?;
                    let response: Value =
                        serde_json::from_slice(&response_bytes).map_err(|e| e.to_string())?;
                    if response["version"] != request["version"]
                        || response["request_id"] != request["request_id"]
                        || response["ok"] != true
                        || response["payload"]["version"] != "workcell.material-world/v1"
                        || response["payload"]["workcell_ref"] != original["workcell_ref"]
                        || response["payload"]["subjects"] != original["subjects"]
                        || !response["payload"]["world_ref"].is_string()
                        || response["payload"]["world_ref"] == original["world_ref"]
                    {
                        return Err(
                            "actual owner did not produce the required successor reply".into()
                        );
                    }
                    // Keep genuine native bytes, not a reconstructed success object.
                    fs::write(&evidence, &response_bytes).map_err(|e| e.to_string())?;
                    Ok(response)
                })();
                // No byte of the actual owner reply is returned to its client.
                let _ = client.shutdown(Shutdown::Both);
                let _ = sender.send(outcome);
            }
            if !done.load(Ordering::SeqCst) {
                let _ = sender.send(Err(
                    "native proxy observation bound elapsed before explicit case completion".into(),
                ));
            }
        });
        Self {
            endpoint,
            received,
            forwarded,
            stop,
            result,
            thread: Some(thread),
        }
    }
    fn actual_reply(&self) -> Value {
        self.result
            .recv_timeout(Duration::from_secs(12))
            .expect("bounded actual owner reply")
            .expect("validated genuine native recover reply")
    }
    fn finish(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect("native framed proxy must end without panic");
        assert_eq!(self.count(), 1);
        assert_eq!(self.forwarded(), 1);
        for outcome in self.result.try_iter() {
            outcome.expect("no unobserved native proxy failure");
        }
    }
    fn count(&self) -> usize {
        self.received.load(Ordering::SeqCst)
    }
    fn forwarded(&self) -> usize {
        self.forwarded.load(Ordering::SeqCst)
    }
}
impl Drop for DropActualRecoveryReply {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn managed_workload_pid(world: &Value) -> u32 {
    let bindings = world["binding_graph"]["bindings"].as_array().unwrap();
    let pids = bindings
        .iter()
        .filter(|binding| binding["properties"]["logical_ref"] == "service:factory-material-test")
        .map(|binding| {
            binding["properties"]["pid"]
                .as_str()
                .unwrap()
                .parse::<u32>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 1, "one actual native managed workload");
    pids[0]
}

fn await_native_workload_stopped(
    workcell: &Path,
    endpoint: &str,
    client: &Path,
    receipt: &Path,
    pid: u32,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let observed = success(
            Command::new(workcell)
                .args([
                    "--endpoint",
                    endpoint,
                    "--state-root",
                    client.to_str().unwrap(),
                    "--receipt",
                    receipt.to_str().unwrap(),
                    "--json",
                    "observe",
                ])
                .env("WORKCELL_CONTROL_TOKEN", TOKEN)
                .output()
                .unwrap(),
        );
        let services = observed["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|observation| {
                observation["detail"]["logical_ref"] == "service:factory-material-test"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            services.len(),
            1,
            "one native lifetime observation: {observed}"
        );
        let detail = &services[0]["detail"];
        assert_eq!(
            detail["pid"],
            pid.to_string(),
            "the exact original managed child"
        );
        if detail["running"] == "false" {
            assert_eq!(detail["reachable"], "false");
            assert!(
                detail["exit_status"].is_string(),
                "native child reaping: {observed}"
            );
            return observed;
        }
        assert!(
            Instant::now() < deadline,
            "native owner has not reaped its child: {observed}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(target_os = "linux")]
fn await_linux_workload_stopped(pid: u32) -> &'static str {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return "absent",
            Err(error) => panic!("cannot observe the actual Linux child: {error}"),
            Ok(stat) => {
                let tail = stat.rsplit_once(") ").expect("actual Linux proc stat").1;
                if tail.starts_with("Z ") {
                    return "zombie-not-running";
                }
                if tail.starts_with("X ") {
                    return "exited-not-running";
                }
                assert!(
                    Instant::now() < deadline,
                    "actual Linux child is still running: {stat}"
                );
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

// The disposable workload honours this marker itself on every platform. This
// does not claim Linux parent-death supervision on Mac or kill an unrelated PID.
struct StopNativeWorkload {
    marker: PathBuf,
    port: u16,
}
impl Drop for StopNativeWorkload {
    fn drop(&mut self) {
        if let Err(error) = fs::write(&self.marker, b"stop owned test workload") {
            eprintln!("native workload cleanup marker could not be retained: {error}");
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while reachable(self.port) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        if reachable(self.port) {
            eprintln!("owned native test workload remains reachable");
        }
    }
}

fn native_material_receipts(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root.join("control-worlds"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .filter(|(name, _)| name.ends_with(".json"))
        .collect()
}

// A failing opt-in native case leaves real owner journal/partial bytes in the
// explicitly supplied retained evidence directory. Required success finalizes
// evidence before returning; unwind cleanup is secondary and never certifies it.
struct RetainNativeMaterial {
    root: PathBuf,
    destination: PathBuf,
    case: &'static str,
    complete: bool,
}
impl RetainNativeMaterial {
    fn new(root: &Path, case: &'static str) -> Self {
        let parent = PathBuf::from(
            std::env::var_os("FACTORY_TEST_NATIVE_EVIDENCE_DIR")
                .expect("a retained native evidence directory is mandatory"),
        );
        assert!(parent.is_absolute() && parent.is_dir());
        let destination = parent.join(format!("{case}-{}", ulid::Ulid::new()));
        fs::create_dir(&destination).unwrap();
        Self {
            root: root.into(),
            destination,
            case,
            complete: false,
        }
    }
    fn snapshot(&self) -> io::Result<()> {
        fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
            if !source.exists() {
                return Ok(());
            }
            if source.is_dir() {
                fs::create_dir_all(destination)?;
                for entry in fs::read_dir(source)? {
                    let entry = entry?;
                    if entry.file_type()?.is_symlink() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "native test evidence contains a symlink",
                        ));
                    }
                    copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
                }
            } else {
                fs::copy(source, destination)?;
            }
            Ok(())
        }
        for name in [
            "state.json",
            "native-owner-state",
            "native-NOW",
            "native-original-world.json",
            "native-successor-world.json",
            "native-demand.json",
            "human-source.txt",
            "native-dropped-control-response.json",
            "native-lifetime-recovery.json",
            "host.log",
        ] {
            copy_tree(&self.root.join(name), &self.destination.join(name))?;
        }
        Ok(())
    }
    fn finish(&mut self, required: &[&str]) {
        for required in required {
            assert!(
                self.root.join(required).is_file(),
                "required native evidence is absent: {required}"
            );
        }
        assert!(self.root.join("native-owner-state/control-worlds").is_dir());
        self.snapshot()
            .expect("required native evidence must be retained");
        fs::write(
            self.destination.join("case-standing.json"),
            json!({"case":self.case,"native_material_case_passed":true,
                "model_executed":false,"worker_quiescence_established":false,
                "factory_whole_completed":false})
            .to_string(),
        )
        .expect("required evidence standing");
        self.complete = true;
        println!(
            "actual native {} evidence: {}",
            self.case,
            self.destination.display()
        );
    }
}
impl Drop for RetainNativeMaterial {
    fn drop(&mut self) {
        if !self.complete {
            if let Err(error) = self.snapshot() {
                eprintln!("secondary native evidence retention failure: {error}");
            }
        }
    }
}

#[test]
#[ignore = "requires actual Workcell binaries and retained evidence directory; explicit native material CI case"]
fn native_workcell_lost_recovery_reply_survives_restart_without_implicit_effect() {
    let bin = PathBuf::from(
        std::env::var("FACTORY_TEST_WORKCELL_BIN_DIR")
            .expect("native Workcell source-build directory is mandatory for this case"),
    );
    let workcell = bin.join("workcell");
    let service = bin.join("workcell-control-service");
    assert!(workcell.is_file() && service.is_file());
    let world = World::new();
    let mut evidence = RetainNativeMaterial::new(world.dir.path(), "lost-recovery");
    let root = world.dir.path();
    let state = root.join("native-owner-state");
    let now = root.join("native-NOW");
    let client = root.join("native-client-state");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&now).unwrap();
    fs::create_dir_all(&client).unwrap();
    let source = root.join("human-source.txt");
    fs::write(&source, b"human source stays byte-identical\0").unwrap();
    fs::write(now.join("pending-return.txt"), b"retained pending Return\0").unwrap();
    let workload_port = port();
    let stop_marker = root.join("stop-native-workload");
    let control_port = port();
    let endpoint = format!("127.0.0.1:{control_port}");
    let python = Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap().trim().to_owned();
    let declarations = json!({
        "schema":"workcell.service-declaration/v1",
        "services":[{
            "logical_ref":"service:factory-material-test",
            "endpoint":format!("tcp://127.0.0.1:{workload_port}"),
            "lifetime":"provider-process-scoped","program":python,
            "args":["-S","-c","import pathlib,socket,sys,time; p=pathlib.Path(sys.argv[2]); p.write_bytes(b'native workload partial bytes\\0') if not p.exists() else None; stop=pathlib.Path(sys.argv[3]); s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',int(sys.argv[1]))); s.listen(); deadline=time.monotonic()+300\nwhile not stop.exists() and time.monotonic()<deadline: time.sleep(.025)\ns.close()",workload_port.to_string(),now.join("partial-work.txt"),stop_marker],
            "cwd":now,"readiness":{"host":"127.0.0.1","port":workload_port,"timeout_ms":5000}
        }]
    });
    fs::write(state.join("services.json"), declarations.to_string()).unwrap();
    fs::write(
        state.join("storage.json"),
        json!({"schema":"workcell.directory-storage/v1","directories":[{"logical_ref":"now:factory-material-test","path":now}]}).to_string(),
    )
    .unwrap();
    let tiers = json!({"required":[],"preferred":[],"optional":[]});
    let demand = json!({
        "demand_ref":"demand:factory-material-native",
        "subjects":{"attempt":NATIVE_ATTEMPT,"run":RUN,"source":world.workflow.source.reference.to_string()},
        "affordances":tiers,"connectivity":{"required":["service:factory-material-test"],"preferred":[],"optional":[]},
        "exposure":tiers,"outputs":tiers,
        "storage":{"required":[{"logical_ref":"now:factory-material-test","access":"writable","sharing":"shared","minimum_capacity":null,"unit":null,"persistence":"external","retention":"preserve"}],"preferred":[],"optional":[]},
        "workspace":null,"project_runtime":null,"resources":[],"persistence":null,
        "isolation_trust":null,"retention":"release","extensions":{}
    });
    let demand_path = root.join("native-demand.json");
    fs::write(&demand_path, demand.to_string()).unwrap();
    let mut host = Host::launch(&service, &state, &endpoint);
    let _workload_cleanup = StopNativeWorkload {
        marker: stop_marker.clone(),
        port: workload_port,
    };
    let receipt_path = root.join("native-original-world.json");
    let prepared = success(
        Command::new(&workcell)
            .args([
                "--endpoint",
                &endpoint,
                "--state-root",
                client.to_str().unwrap(),
            ])
            .args([
                "--receipt",
                receipt_path.to_str().unwrap(),
                "--json",
                "prepare",
            ])
            .args(["--demand-json", demand_path.to_str().unwrap()])
            .env("WORKCELL_CONTROL_TOKEN", TOKEN)
            .output()
            .unwrap(),
    );
    let original = prepared["world"].clone();
    let original_ref = original["world_ref"].as_str().unwrap();
    let original_bytes = fs::read(&receipt_path).unwrap();
    await_port(workload_port, true);
    let mut disposition = world.disposition("peer-read");
    disposition.body.material_world_ref = Some(original_ref.into());
    success(world.action(FactoryAttemptOperation::StartSerial {
        attempt_ref: NATIVE_ATTEMPT.into(),
        task_ref: "task:native-material".into(),
        parent_journey_ref: "journey:test".into(),
        workflow_unit_ref: world.workflow.unit("peer-read").unwrap().reference.clone(),
        disposition,
        retry_grant: None,
        tracking: vec![],
        place_grant: None,
    }));
    let request = |key: &str, operation: WorkcellWorldOperation, receipt: &Path| {
        let mut request = world.request(key, operation);
        request.attempt_ref = NATIVE_ATTEMPT.into();
        request.execution_ref = format!("factory-attempt:{NATIVE_ATTEMPT}");
        request.invocation = NativeOwnerInvocation::WorkcellWorld {
            binary: workcell.clone(),
            receipt: receipt.into(),
            world_operation: operation,
            endpoint: Some(endpoint.clone()),
            authorization: None,
            contract_revision: WORKCELL_CAW_CONTRACT_REVISION.into(),
        };
        request
    };

    let partial = fs::read(now.join("partial-work.txt")).unwrap();
    fs::write(&stop_marker, b"stop original owned workload").unwrap();
    await_port(workload_port, false);
    // Require actual observe/reaping, rather than only a closed socket.
    await_native_workload_stopped(
        &workcell,
        &endpoint,
        &client,
        &receipt_path,
        managed_workload_pid(&original),
    );
    fs::remove_file(&stop_marker).unwrap();
    let mut proxy = DropActualRecoveryReply::launch(
        &endpoint,
        &original,
        &root.join("native-dropped-control-response.json"),
    );
    let mut recover = request(
        "native:recover-lost",
        WorkcellWorldOperation::Recover,
        &receipt_path,
    );
    if let NativeOwnerInvocation::WorkcellWorld { endpoint, .. } = &mut recover.invocation {
        *endpoint = Some(proxy.endpoint.clone());
    }
    let lost = success(native_call(&world, &recover));
    assert_eq!(lost["needsReconciliation"], true, "{lost}");
    assert_eq!(lost["transportObservation"]["phase"], "uncertain", "{lost}");
    assert!(
        lost["ownerReceipt"].is_null(),
        "Factory cannot adopt the proxy's retained owner reply"
    );
    let reply = proxy.actual_reply();
    let successor = reply["payload"].clone();
    let successor_ref = successor["world_ref"].as_str().unwrap();
    await_port(workload_port, true);
    assert_eq!(fs::read(now.join("partial-work.txt")).unwrap(), partial);
    assert_eq!(fs::read(&receipt_path).unwrap(), original_bytes);
    let journal = native_material_receipts(&state);
    let retained_worlds = journal
        .values()
        .map(|bytes| serde_json::from_slice::<Value>(bytes).unwrap())
        .collect::<Vec<_>>();
    assert!(retained_worlds
        .iter()
        .any(|world| world["world_ref"] == successor["world_ref"]
            && world["subjects"] == original["subjects"]));
    assert!(retained_worlds
        .iter()
        .any(|world| world["world_ref"] == original["world_ref"]
            && world["provenance"]["superseded_by"] == successor["world_ref"]));
    assert_eq!(proxy.count(), 1);
    assert_eq!(proxy.forwarded(), 1);
    let uncertain_bytes = fs::read(world.state()).unwrap();
    let exact = success(native_call(&world, &recover));
    assert_eq!(exact["replayed"], true);
    assert_eq!(exact["needsReconciliation"], true);
    assert_eq!(fs::read(world.state()).unwrap(), uncertain_bytes);
    let mut new_key = request(
        "native:recover-lost-new-key",
        WorkcellWorldOperation::Recover,
        &receipt_path,
    );
    if let NativeOwnerInvocation::WorkcellWorld { endpoint, .. } = &mut new_key.invocation {
        *endpoint = Some(proxy.endpoint.clone());
    }
    refused(native_call(&world, &new_key), "unresolved material");
    assert_eq!(proxy.count(), 1);
    assert_eq!(proxy.forwarded(), 1);
    // Explicit isolated native housekeeping uses the actual owner reply. It is
    // not a Factory receipt or reconciliation, Agent Return or task success.
    let successor_path = root.join("native-successor-world.json");
    fs::write(&successor_path, serde_json::to_vec(&successor).unwrap()).unwrap();
    let released = success(
        Command::new(&workcell)
            .args([
                "--endpoint",
                &endpoint,
                "--state-root",
                client.to_str().unwrap(),
                "--receipt",
                successor_path.to_str().unwrap(),
                "--json",
                "release",
            ])
            .env("WORKCELL_CONTROL_TOKEN", TOKEN)
            .output()
            .unwrap(),
    );
    assert_eq!(released["world_ref"], successor_ref);
    assert_eq!(released["disposition"], "released");
    await_port(workload_port, false);
    assert_eq!(
        fs::read(world.state()).unwrap(),
        uncertain_bytes,
        "late native cleanup cannot complete the Factory undertaking"
    );
    let released_journal = native_material_receipts(&state);
    host.stop();
    host = Host::launch(&service, &state, &endpoint);
    // Every native_call starts a fresh Factory process. After actual material
    // owner replacement, the same immutable request still replays, never acts.
    let restarted = success(native_call(&world, &recover));
    assert_eq!(restarted["replayed"], true);
    assert_eq!(restarted["needsReconciliation"], true);
    assert_eq!(fs::read(world.state()).unwrap(), uncertain_bytes);
    refused(native_call(&world, &new_key), "unresolved material");
    assert_eq!(proxy.count(), 1);
    assert_eq!(proxy.forwarded(), 1);
    assert_eq!(native_material_receipts(&state), released_journal);
    assert_eq!(fs::read(&receipt_path).unwrap(), original_bytes);
    assert_eq!(fs::read(now.join("partial-work.txt")).unwrap(), partial);
    assert_eq!(
        fs::read(now.join("pending-return.txt")).unwrap(),
        b"retained pending Return\0"
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        b"human source stays byte-identical\0"
    );
    assert_eq!(
        world.calls(),
        0,
        "no protocol double may run in this native case"
    );
    let reading = world.read();
    let attempt = reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == NATIVE_ATTEMPT)
        .unwrap();
    assert!(attempt.readable_return.is_none());
    assert!(!reading.completion_verified);
    assert!(!fs::read_to_string(world.state()).unwrap().contains(TOKEN));
    host.stop();
    proxy.finish();
    assert!(!reachable(workload_port));
    evidence.finish(&[
        "state.json",
        "native-original-world.json",
        "native-successor-world.json",
        "native-dropped-control-response.json",
        "native-NOW/partial-work.txt",
        "native-NOW/pending-return.txt",
    ]);
}
