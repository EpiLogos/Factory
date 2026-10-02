//! Opt-in acceptance against actual installed AIKit and Workcell.
//! Live Task/material queries are read-only. The separate source-health case
//! may replace and restore only its reviewed disposable DirectoryStorage path.
//! Query expectations are not reserved attempts, worker execution or Return.
//! The exact production modules are compiled here because their admission
//! helpers are private; this test introduces no owner, scheduler or dispatcher.
use epilogos_factory::attempt_native_store::FileAttemptStore;
use epilogos_factory::core::run::{RunRef, WorkflowUnitRef};
use epilogos_factory::{attempt_runtime, core, native_process};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[path = "../src/attempt_task_material.rs"]
mod material;
#[path = "../src/native_aikit_route.rs"]
mod native_aikit_route;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeQuery {
    workflow_unit_ref: WorkflowUnitRef,
    task_ref: String,
    agent_ref: String,
    agency_ref: String,
    world_binding_ref: String,
    agent_session_ref: String,
    session_space_ref: String,
}

impl NativeQuery {
    fn basis(&self) -> material::MaterialAttemptBasis<'_> {
        material::MaterialAttemptBasis {
            workflow_unit_ref: &self.workflow_unit_ref,
            task_ref: &self.task_ref,
            agent_ref: &self.agent_ref,
            agency_ref: &self.agency_ref,
            world_binding_ref: &self.world_binding_ref,
            agent_session_ref: &self.agent_session_ref,
            session_space_ref: &self.session_space_ref,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeMaterialCase {
    /// The actual owner state is only read. No isolated or live attempt starts.
    factory_state: PathBuf,
    run_ref: RunRef,
    binary: PathBuf,
    cwd: PathBuf,
    transport: native_aikit_route::AikitOwnerTransport,
    task_revision: String,
    stale_task_revision: String,
    /// Exact native executable provenance, captured before choosing this case.
    aikit_version: String,
    workcell_version: String,
    aikit_sha256: String,
    workcell_sha256: String,
    sha256_binary: PathBuf,
    #[serde(default)]
    sha256_arguments: Vec<String>,
    /// Read-only assertions recovered from admitted source and native Task /
    /// Agency. Only identity fields are supplied: no model, execution record or
    /// canonical reservation is manufactured to query the actual material.
    query_expectation: NativeQuery,
    agency_source_revision: String,
    agency_source_digest: String,
}

struct Case {
    driver: NativeMaterialCase,
    current: attempt_runtime::FactoryAttemptReading,
    task: Value,
    state_bytes: Vec<u8>,
}

fn structured(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "native owner refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("actual native structured reading")
}

impl Case {
    fn load() -> Self {
        let path = std::env::var_os("FACTORY_NATIVE_AIKIT_MATERIAL_CASE").expect(
            "supply the reviewed current installed material case; no provider-double fallback",
        );
        let driver: NativeMaterialCase = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let state_bytes = fs::read(&driver.factory_state).unwrap();
        let current = FileAttemptStore::open_run(&driver.factory_state, driver.run_ref.clone())
            .unwrap()
            .reading()
            .unwrap();
        assert!(
            current.source_current,
            "case must name the current native source"
        );
        assert_eq!(current.run_ref, driver.run_ref);
        let expectation = &driver.query_expectation;
        assert!(current
            .required_units
            .contains(&expectation.workflow_unit_ref));
        let task = Self::task_read(&driver);
        assert_eq!(task["schema"], "aikit.encounter-task/v1");
        assert_eq!(task["ready"], true);
        assert_eq!(task["revision"], driver.task_revision);
        assert_ne!(driver.stale_task_revision, driver.task_revision);
        assert_eq!(task["request"]["central"]["task_ref"], expectation.task_ref);
        assert_eq!(task["request"]["cwd"], json!(driver.cwd));
        assert_eq!(
            task["prepared_run"]["scope"]["schema"],
            "workcell.prepared-run-scope/v1"
        );
        let actual_binding =
            &task["prepared_run"]["scope"]["agency"]["admission"]["differentiated_binding"];
        assert_eq!(actual_binding["agent_ref"], expectation.agent_ref);
        assert_eq!(actual_binding["agency_ref"], expectation.agency_ref);
        assert_eq!(actual_binding["binding_ref"], expectation.world_binding_ref);
        assert_eq!(
            task["prepared_run"]["scope"]["agency"]["agency_rev"],
            driver.agency_source_revision
        );
        assert_eq!(
            task["prepared_run"]["scope"]["agency"]["source_digest"],
            driver.agency_source_digest
        );
        let case = Self {
            driver,
            current,
            task,
            state_bytes,
        };
        case.check_native_executable(
            &case.driver.binary,
            &case.driver.aikit_version,
            &case.driver.aikit_sha256,
        );
        case.check_native_executable(
            Path::new(case.task["prepared_run"]["executable"].as_str().unwrap()),
            &case.driver.workcell_version,
            &case.driver.workcell_sha256,
        );
        case
    }

    fn task_read(driver: &NativeMaterialCase) -> Value {
        structured(
            native_aikit_route::output(
                &driver.binary,
                &driver.cwd,
                Some(&driver.transport),
                &[
                    "session-space".into(),
                    "encounter-task-read".into(),
                    "--agent-session".into(),
                    driver.query_expectation.agent_session_ref.clone(),
                ],
                Duration::from_secs(30),
            )
            .unwrap(),
        )
    }

    fn check_native_executable(&self, binary: &Path, version: &str, sha256: &str) {
        assert_eq!(sha256.len(), 64);
        assert!(sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let format = native_aikit_route::material_output(
            Path::new("/usr/bin/file"),
            Some(&self.driver.transport),
            &["-b".into(), binary.display().to_string()],
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(format.status.success());
        let format = String::from_utf8(format.stdout).unwrap();
        assert!(
            format.contains("ELF") || format.contains("Mach-O"),
            "owner must be a native executable, not an owner protocol script: {format}"
        );
        let native_version = native_aikit_route::material_output(
            binary,
            Some(&self.driver.transport),
            &["--version".into()],
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(native_version.status.success());
        assert_eq!(
            String::from_utf8(native_version.stdout).unwrap().trim(),
            version
        );
        assert!(matches!(
            self.driver
                .sha256_binary
                .file_name()
                .and_then(|name| name.to_str()),
            Some("sha256sum" | "shasum")
        ));
        let mut arguments = self.driver.sha256_arguments.clone();
        arguments.push(binary.display().to_string());
        let digest = native_aikit_route::material_output(
            &self.driver.sha256_binary,
            Some(&self.driver.transport),
            &arguments,
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(digest.status.success());
        assert_eq!(
            String::from_utf8(digest.stdout)
                .unwrap()
                .split_whitespace()
                .next(),
            Some(sha256)
        );
    }

    fn inspect(
        &self,
        current: &attempt_runtime::FactoryAttemptReading,
        expectation: &NativeQuery,
        task: &Value,
        transport: &native_aikit_route::AikitOwnerTransport,
    ) -> Result<Option<Value>, String> {
        material::inspect(
            current,
            &expectation.basis(),
            task,
            Some(transport),
            Instant::now() + Duration::from_secs(30),
        )
    }

    fn admitted(&self) -> Value {
        self.inspect(
            &self.current,
            &self.driver.query_expectation,
            &self.task,
            &self.driver.transport,
        )
        .expect("actual same-source prepared material must admit")
        .expect("actual case requires prepared material")
    }

    fn unchanged(&self, material_before: &Value) {
        assert_eq!(
            fs::read(&self.driver.factory_state).unwrap(),
            self.state_bytes,
            "native acceptance must not write Factory state"
        );
        assert_eq!(
            FileAttemptStore::open_run(&self.driver.factory_state, self.driver.run_ref.clone())
                .unwrap()
                .reading()
                .unwrap(),
            self.current
        );
        assert_eq!(
            Self::task_read(&self.driver),
            self.task,
            "native acceptance must retain the original Task"
        );
        let after = self.admitted();
        assert_eq!(after["preparedRun"], material_before["preparedRun"]);
        assert_eq!(
            after["runReading"], material_before["runReading"],
            "native acceptance must retain the actual original material Run"
        );
        assert_eq!(
            after["materialReading"]["receipt_world"],
            material_before["materialReading"]["receipt_world"]
        );
        // A fresh observation has its own actual completion time. Compare its
        // native material facts, without confusing a new read with a write.
        assert_eq!(
            after["materialReading"]["observation"]["reading"],
            material_before["materialReading"]["observation"]["reading"]
        );
        assert_eq!(
            after["materialReading"]["observation"]["status"],
            "supplied"
        );
    }
}

#[test]
#[ignore = "requires a reviewed installed AIKit/Workcell material case on its declared native route"]
fn actual_material_retains_same_current_source_task_agency_and_workcell_without_effects() {
    let case = Case::load();
    let material = case.admitted();
    let world = &material["materialReading"]["receipt_world"];
    assert_eq!(
        world["subjects"]["factory_run"],
        case.current.run_ref.to_string()
    );
    assert_eq!(
        world["subjects"]["workflow_source"],
        case.current.workflow_source_ref
    );
    assert_eq!(
        world["subjects"]["workflow_source_revision"],
        case.current.workflow_source_revision
    );
    assert_eq!(
        world["subjects"]["workflow_source_digest"],
        case.current.workflow_source_digest
    );
    assert_eq!(
        world["subjects"]["workflow_unit"],
        json!(case.driver.query_expectation.workflow_unit_ref)
    );
    assert_eq!(
        world["subjects"]["central_task"],
        case.driver.query_expectation.task_ref
    );
    assert_eq!(
        world["workcell_ref"],
        case.driver.transport.declared_workcell()
    );
    assert_eq!(
        material["materialReading"]["observation"]["status"],
        "supplied"
    );
    case.unchanged(&material);
}

#[test]
#[ignore = "requires a reviewed installed AIKit/Workcell material case on its declared native route"]
fn changed_query_bases_cannot_admit_unchanged_native_material() {
    let case = Case::load();
    let material = case.admitted();
    // These are adversarial query assertions against unchanged owner records,
    // not actual source changes, reservations, effects or successful workers.
    for change in 0..5 {
        let mut current = case.current.clone();
        match change {
            0 => current.run_ref = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse().unwrap(),
            1 => current.workflow_source_ref.push_str(":different-source"),
            2 => current
                .workflow_source_revision
                .push_str(":newer-candidate"),
            3 => current.workflow_source_digest = "f".repeat(64),
            4 => current.source_current = false,
            _ => unreachable!(),
        }
        assert!(
            case.inspect(
                &current,
                &case.driver.query_expectation,
                &case.task,
                &case.driver.transport
            )
            .is_err(),
            "material incorrectly admitted changed query {change}"
        );
    }
    let mut unit = case.driver.query_expectation.clone();
    unit.workflow_unit_ref = "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FBD"
        .parse::<WorkflowUnitRef>()
        .unwrap();
    assert_ne!(
        unit.workflow_unit_ref,
        case.driver.query_expectation.workflow_unit_ref
    );
    assert!(case
        .inspect(&case.current, &unit, &case.task, &case.driver.transport)
        .is_err());
    let mut workcell = case.driver.transport.clone();
    match &mut workcell {
        native_aikit_route::AikitOwnerTransport::Local { workcell_ref, .. }
        | native_aikit_route::AikitOwnerTransport::Ssh { workcell_ref, .. } => {
            *workcell_ref = "workcell:wrong-native-material-query".into()
        }
    }
    assert!(case
        .inspect(
            &case.current,
            &case.driver.query_expectation,
            &case.task,
            &workcell
        )
        .is_err());
    let mut stale_scope = case.task.clone();
    stale_scope["prepared_run"]["scope"]["run_revision"] = json!("sha256:stale-native-run-query");
    assert!(case
        .inspect(
            &case.current,
            &case.driver.query_expectation,
            &stale_scope,
            &case.driver.transport
        )
        .is_err());
    let mut changed_boundary = case.task.clone();
    changed_boundary["inspection"]["requirements_digest"] =
        json!("sha256:different-source-boundary-query");
    assert!(case
        .inspect(
            &case.current,
            &case.driver.query_expectation,
            &changed_boundary,
            &case.driver.transport
        )
        .is_err());
    let mut expired = case.task.clone();
    expired["requirements"]["expires_at_unix_ms"] = json!(1);
    let refusal = case
        .inspect(
            &case.current,
            &case.driver.query_expectation,
            &expired,
            &case.driver.transport,
        )
        .unwrap_err();
    assert!(refusal.contains("lease is absent or expired"));
    case.unchanged(&material);
}

#[test]
#[ignore = "requires a reviewed installed AIKit/Workcell material case with a genuinely retained older Task revision"]
fn installed_aikit_refuses_the_actual_stale_task_revision_before_body_launch() {
    let case = Case::load();
    let material = case.admitted();
    // The obsolete revision is captured from native predecessor history. The
    // owner checks revision before executing the provider; only this refusing
    // operation is permitted, never a current-revision positive exec.
    let refusal = native_aikit_route::output(
        &case.driver.binary,
        &case.driver.cwd,
        Some(&case.driver.transport),
        &[
            "session-space".into(),
            "encounter-task-exec".into(),
            "--agent-session".into(),
            case.driver.query_expectation.agent_session_ref.clone(),
            "--expected-revision".into(),
            case.driver.stale_task_revision.clone(),
        ],
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        !refusal.status.success(),
        "stale native Task must never start its body"
    );
    assert!(
        String::from_utf8_lossy(&refusal.stderr).contains(
            "Task revision or actual process cwd differs from the prepared execution basis"
        ),
        "native stale admission refused for an unrelated reason: {}",
        String::from_utf8_lossy(&refusal.stderr)
    );
    assert!(refusal.stdout.is_empty());
    case.unchanged(&material);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeSourceHealthCase {
    schema: String,
    binary: PathBuf,
    binary_sha256: String,
    binary_version: String,
    state_root: PathBuf,
    receipt: PathBuf,
    run_slug: String,
    world_ref: String,
    selected_material: String,
    logical_ref: String,
    source_path: PathBuf,
    retained_original_path: PathBuf,
    replacement_path: PathBuf,
    source_identity: String,
    replacement_identity: String,
    file_sha256: BTreeMap<String, String>,
    standing: String,
}

fn sha256_file(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> String {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(
        metadata.is_dir(),
        "source fixture must be an actual directory"
    );
    format!("{}:{}", metadata.dev(), metadata.ino())
}

fn owner_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(
                !kind.is_symlink(),
                "owner fixture must not escape through symlinks"
            );
            if kind.is_dir() {
                visit(root, &entry.path(), files);
            } else {
                assert!(kind.is_file());
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

/// Restore the original object even if a negative admission assertion panics.
/// Neither directory is deleted, emptied, or recreated by this gate.
struct DisposableSourceSwap<'a> {
    case: &'a NativeSourceHealthCase,
    original_moved: bool,
    replacement_moved: bool,
}

impl<'a> DisposableSourceSwap<'a> {
    fn begin(case: &'a NativeSourceHealthCase) -> io::Result<Self> {
        let mut swap = Self {
            case,
            original_moved: false,
            replacement_moved: false,
        };
        fs::rename(&case.source_path, &case.retained_original_path)?;
        swap.original_moved = true;
        fs::rename(&case.replacement_path, &case.source_path)?;
        swap.replacement_moved = true;
        Ok(swap)
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.replacement_moved {
            fs::rename(&self.case.source_path, &self.case.replacement_path)?;
            self.replacement_moved = false;
        }
        if self.original_moved {
            fs::rename(&self.case.retained_original_path, &self.case.source_path)?;
            self.original_moved = false;
        }
        Ok(())
    }
}

impl Drop for DisposableSourceSwap<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!(
                "Disposable source restoration failed; retain the original at {}: {error}",
                self.case.retained_original_path.display()
            );
        }
    }
}

impl NativeSourceHealthCase {
    fn load() -> Self {
        let path = std::env::var_os("FACTORY_NATIVE_WORKCELL_SOURCE_HEALTH_CASE")
            .expect("supply the reviewed real disposable source-health driver");
        let case: Self = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(case.schema, "factory.native-source-health-case/v1");
        assert!(!case.standing.is_empty());
        let fixture = case.source_path.parent().unwrap();
        assert_eq!(
            fixture.file_name().and_then(|name| name.to_str()),
            Some("native-material-unavailable-owner-20261001"),
            "this gate is authorized only for the retained disposable fixture"
        );
        for (path, name) in [
            (&case.source_path, "bound-source"),
            (&case.retained_original_path, "retained-original-source"),
            (&case.replacement_path, "retained-replacement-source"),
            (&case.state_root, "owner-state"),
        ] {
            assert!(path.is_absolute());
            assert_eq!(path.parent(), Some(fixture));
            assert_eq!(path.file_name().and_then(|name| name.to_str()), Some(name));
        }
        assert!(case.receipt.starts_with(&case.state_root));
        assert!(!case.retained_original_path.exists());
        assert!(fs::read_dir(&case.replacement_path)
            .unwrap()
            .next()
            .is_none());
        assert_eq!(case.file_sha256.len(), 2);
        assert!(case.file_sha256.contains_key("partial.txt"));
        assert!(case.file_sha256.contains_key("Return.md"));
        let binary = fs::read(&case.binary).unwrap();
        assert!(
            binary.starts_with(b"\x7fELF")
                || binary.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
                || binary.starts_with(&[0xfe, 0xed, 0xfa, 0xcf]),
            "Workcell must be the pinned native executable"
        );
        assert_eq!(format!("{:x}", Sha256::digest(&binary)), case.binary_sha256);
        let version = native_process::output(
            Command::new(&case.binary).arg("--version"),
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8(version.stdout).unwrap().trim(),
            case.binary_version
        );
        case
    }

    fn inspect(&self) -> Value {
        let mut command = Command::new(&self.binary);
        command
            .arg("--state-root")
            .arg(&self.state_root)
            .arg("--receipt")
            .arg(&self.receipt)
            .args(["--json", "inspect"]);
        let reading =
            structured(native_process::output(&mut command, Duration::from_secs(30)).unwrap());
        assert_eq!(reading["ok"], true);
        assert_eq!(reading["receipt_world"]["world_ref"], self.world_ref);
        assert_eq!(reading["observation"]["status"], "supplied");
        assert_eq!(reading["observation"]["reading"]["ok"], true);
        reading
    }

    fn run_read(&self) -> Value {
        let mut command = Command::new(&self.binary);
        command.arg("--state-root").arg(&self.state_root).args([
            "--json",
            "run",
            "show",
            "--run",
            &self.run_slug,
        ]);
        structured(native_process::output(&mut command, Duration::from_secs(30)).unwrap())
    }

    fn check_retained_bytes(&self, path: &Path) {
        for (name, hash) in &self.file_sha256 {
            assert_eq!(
                sha256_file(&path.join(name)),
                *hash,
                "retained {name} changed"
            );
        }
    }

    fn select<'a>(&self, reading: &'a Value) -> Result<(&'a Value, &'a Value), String> {
        material::selected_source(
            &self.selected_material,
            &json!(self.source_path),
            &reading["receipt_world"],
            reading,
        )
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires the reviewed disposable DirectoryStorage fixture and pinned installed Workcell"]
fn supplied_native_observation_cannot_admit_a_replaced_source_object() {
    let case = NativeSourceHealthCase::load();
    assert_eq!(directory_identity(&case.source_path), case.source_identity);
    assert_eq!(
        directory_identity(&case.replacement_path),
        case.replacement_identity
    );
    case.check_retained_bytes(&case.source_path);
    let owner_before = owner_files(&case.state_root);
    let run_before = case.run_read();
    let healthy = case.inspect();
    let (binding, observation) = case.select(&healthy).expect("actual healthy source admits");
    assert_eq!(binding["logical_ref"], case.logical_ref);
    assert_eq!(observation["state"], "healthy");
    assert_eq!(
        observation["detail"]["object_identity"],
        case.source_identity
    );

    let mut swap = DisposableSourceSwap::begin(&case).unwrap();
    assert_eq!(
        directory_identity(&case.source_path),
        case.replacement_identity
    );
    assert_eq!(
        directory_identity(&case.retained_original_path),
        case.source_identity
    );
    case.check_retained_bytes(&case.retained_original_path);
    let unavailable = case.inspect();
    let actual = unavailable["observation"]["reading"]["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|observation| observation["logical_ref"] == case.logical_ref)
        .expect("actual selected source observation");
    assert_eq!(actual["state"], "unavailable");
    assert!(actual["detail"]["observation_error"]
        .as_str()
        .unwrap()
        .contains("old receipt cannot silently bind the new path"));
    assert_eq!(
        case.select(&unavailable).unwrap_err(),
        "Native selected source material is unavailable or changed",
        "supplied/ok native observation must not manufacture source health"
    );
    assert_eq!(owner_files(&case.state_root), owner_before);
    swap.restore()
        .expect("restore actual original source object");

    assert_eq!(directory_identity(&case.source_path), case.source_identity);
    assert_eq!(
        directory_identity(&case.replacement_path),
        case.replacement_identity
    );
    assert!(!case.retained_original_path.exists());
    case.check_retained_bytes(&case.source_path);
    let restored = case.inspect();
    assert_eq!(case.select(&restored).unwrap().1["state"], "healthy");
    assert_eq!(restored["receipt_world"], healthy["receipt_world"]);
    assert_eq!(
        restored["observation"]["reading"],
        healthy["observation"]["reading"]
    );
    assert_eq!(case.run_read(), run_before);
    assert_eq!(owner_files(&case.state_root), owner_before);
}
