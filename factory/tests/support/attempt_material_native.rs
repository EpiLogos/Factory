//! Uses actual compiled Workcell CLI/control-service binaries. The service it
//! hosts is a disposable TCP workload, not an Agent or a commercial model.
use super::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::Child;

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
            .args(["--state-root", root.to_str().unwrap(), "--workcell-ref", WORKCELL])
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
    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            if child.try_wait().unwrap().is_none() {
                child.kill().unwrap();
            }
            child.wait().unwrap();
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
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
#[ignore = "requires source-built native Workcell binaries; required Factory material CI executes it"]
fn native_workcell_caller_retains_process_recovery_and_release() {
    let bin = PathBuf::from(
        std::env::var("FACTORY_TEST_WORKCELL_BIN_DIR")
            .expect("native Workcell source-build directory is mandatory for this case"),
    );
    let workcell = bin.join("workcell");
    let service = bin.join("workcell-control-service");
    assert!(workcell.is_file() && service.is_file());
    let world = World::new();
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
            "args":["-S","-c","import socket,sys,time; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(('127.0.0.1',int(sys.argv[1]))); s.listen(); time.sleep(300)",workload_port.to_string()],
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
    let receipt_path = root.join("native-original-world.json");
    let prepared = success(
        Command::new(&workcell)
            .args(["--endpoint", &endpoint, "--state-root", client.to_str().unwrap()])
            .args(["--receipt", receipt_path.to_str().unwrap(), "--json", "prepare"])
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
    host.stop();
    await_port(workload_port, false);
    host = Host::launch(&service, &state, &endpoint);
    let recover_request = request("native:recover", WorkcellWorldOperation::Recover, &receipt_path);
    let recovered = success(native_call(&world, &recover_request));
    assert_eq!(recovered["needsReconciliation"], false, "{recovered}");
    let successor = recovered["ownerReceipt"]["payload"]["world"].clone();
    assert_ne!(successor["world_ref"], original["world_ref"]);
    assert_eq!(successor["subjects"], original["subjects"]);
    assert_eq!(recovered["ownerReceipt"]["payload"]["previous_world_ref"], original_ref);
    await_port(workload_port, true);
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
    assert_eq!(success(native_call(&world, &recover_request))["replayed"], true);
    refused(
        native_call(&world, &request("native:old", WorkcellWorldOperation::Recover, &receipt_path)),
        "superseded material",
    );
    let successor_path = root.join("native-successor-world.json");
    fs::write(&successor_path, successor.to_string()).unwrap();
    for operation in [
        FactoryAttemptOperation::RequestCancellation { attempt_ref: NATIVE_ATTEMPT.into() },
        FactoryAttemptOperation::AcceptCancellation { attempt_ref: NATIVE_ATTEMPT.into() },
        FactoryAttemptOperation::RecordProcessTermination { attempt_ref: NATIVE_ATTEMPT.into() },
        FactoryAttemptOperation::MarkQuiescent { attempt_ref: NATIVE_ATTEMPT.into() },
    ] {
        success(world.action(operation));
    }
    let released = success(native_call(
        &world,
        &request("native:release", WorkcellWorldOperation::Release, &successor_path),
    ));
    assert_eq!(released["needsReconciliation"], false, "{released}");
    assert_eq!(released["ownerReceipt"]["payload"]["disposition"], "released");
    await_port(workload_port, false);
    assert_eq!(fs::read(&receipt_path).unwrap(), original_bytes);
    assert_eq!(fs::read(&source).unwrap(), b"human source stays byte-identical\0");
    assert_eq!(fs::read(now.join("pending-return.txt")).unwrap(), b"retained pending Return\0");
    assert_eq!(world.calls(), 0, "the protocol double must never run in this native case");
    let reading = world.read();
    let attempt = reading.attempts.iter().find(|attempt| attempt.attempt_ref == NATIVE_ATTEMPT).unwrap();
    assert_eq!(attempt.disposition.body.material_world_ref.as_deref(), Some(original_ref));
    assert!(attempt.readable_return.is_none());
    assert!(!fs::read_to_string(world.state()).unwrap().contains(TOKEN));
    println!("native Workcell source {WORKCELL_CAW_CONTRACT_REVISION}; original={original_ref}; recovered={}; real managed child replaced and released; source/Return bytes preserved", successor["world_ref"]);
    host.stop();
}
