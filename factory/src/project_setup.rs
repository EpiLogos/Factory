//! Factory-owned per-project placement. A Project is available before its
//! first Commission; setup never creates execution or participant evidence.
use crate::commission::project_ref_for_key;
use crate::core::run::Project;
use crate::developmental_read::{
    FactoryCentralProjectLinkRequest, FactoryDevelopmentalFileProvider,
    FACTORY_CENTRAL_PROJECT_LINK_REQUEST,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const SCHEMA: &str = "factory.project-placement/v1";
const STATE: &str = "development-state.json";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Placement {
    schema: String,
    project_key: String,
    factory_project_ref: String,
    state_file: String,
}

fn error(e: impl std::fmt::Display) -> String { e.to_string() }

fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!("Factory placement refuses symlink {}", path.display())),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}

fn reading(root: &Path, placement: &Placement, status: &str) -> Result<Value, String> {
    if placement.schema != SCHEMA || placement.state_file != STATE
        || project_ref_for_key(&placement.project_key).map_err(error)?.to_string() != placement.factory_project_ref {
        return Err("Invalid Factory project placement".into());
    }
    let state_path = root.join(".factory").join(&placement.state_file);
    reject_symlink(&state_path)?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path).map_err(error)?;
    if provider.state().build.project().reference().to_string() != placement.factory_project_ref {
        return Err("Factory placement and native Project identity disagree; existing state preserved".into());
    }
    Ok(json!({"contract":"factory.project-location/v1", "status":status,
        "projectRoot":root, "projectKey":placement.project_key,
        "projectRef":placement.factory_project_ref, "statePath":state_path,
        "centralProjectRef":provider.central_project_link().map(|link| &link.central_project_ref),
        "runCount":provider.state().build.run_count(),
        "execution":"not-requested"}))
}

/// Read the owner's placement, never infer a source from UI preferences.
pub fn locate(root: &Path) -> Result<Value, String> {
    let root = root.canonicalize().map_err(error)?;
    let dir = root.join(".factory");
    reject_symlink(&dir)?;
    reject_symlink(&dir.join("project.json"))?;
    let placement: Placement = serde_json::from_slice(&fs::read(dir.join("project.json")).map_err(error)?).map_err(error)?;
    reading(&root, &placement, "ready")
}

/// The stable key comes from the native scope owner, not a display label.
/// Existing state is validated/reused and never replaced. An interrupted
/// setup with a complete state but no placement is safely completed on retry.
pub fn setup(root: &Path, project_key: &str, central_source: Option<&Path>) -> Result<Value, String> {
    setup_bound(root, project_key, central_source, project_key)
}

/// Central identities may contain spaces. Encode the whole native identity
/// into an injective Factory key without changing Central's source identity.
pub fn setup_central(root: &Path, central_ref: &str, source: &Path) -> Result<Value, String> {
    let mut key = String::from("central-project:");
    for byte in central_ref.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.".contains(&byte) { key.push(byte as char); }
        else { key.push_str(&format!("%{byte:02X}")); }
    }
    let root = root.canonicalize().map_err(error)?;
    reject_symlink(&root.join(".factory"))?;
    if root.join(".factory/project.json").exists() {
        let existing = locate(&root)?;
        key = existing["projectKey"].as_str().ok_or("Factory placement omitted its key")?.into();
    } else if root.join(".factory").join(STATE).exists() {
        let path = root.join(".factory").join(STATE);
        reject_symlink(&path)?;
        let provider = FactoryDevelopmentalFileProvider::open(path).map_err(error)?;
        if let Some(commission) = provider.state().commissions.first() {
            key = commission.request.project_key.clone();
        }
    }
    setup_bound(&root, &key, Some(source), central_ref)
}

fn setup_bound(root: &Path, project_key: &str, central_source: Option<&Path>, central_ref: &str) -> Result<Value, String> {
    let root = root.canonicalize().map_err(error)?;
    if !root.is_dir() { return Err("Factory Project root must be a directory".into()); }
    let project_ref = project_ref_for_key(project_key).map_err(error)?;
    let placement = Placement {schema:SCHEMA.into(), project_key:project_key.into(),
        factory_project_ref:project_ref.to_string(), state_file:STATE.into()};
    // Validate the source before making any filesystem changes.
    let link = central_source.map(|source| -> Result<_, String> {
        let source = source.canonicalize().map_err(error)?;
        if source != root.join("ProjectCentral/project.json").canonicalize().map_err(error)? {
            return Err("Factory placement requires this Project's own Central manifest".into());
        }
        let request = FactoryCentralProjectLinkRequest {contract:FACTORY_CENTRAL_PROJECT_LINK_REQUEST.into(),
            factory_project_ref:project_ref.clone(), central_project_ref:central_ref.into(),
            source_path:source.to_string_lossy().into_owned()};
        crate::developmental_read::verify_central_project_link(&request).map_err(error)?;
        Ok(request)
    }).transpose()?;
    let dir = root.join(".factory");
    reject_symlink(&dir)?;
    fs::create_dir_all(&dir).map_err(error)?;
    let lock_path = dir.join("setup.lock");
    reject_symlink(&lock_path)?;
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(error)?;
    lock.lock_exclusive().map_err(error)?;
    let placement_path = dir.join("project.json");
    reject_symlink(&placement_path)?;
    if placement_path.exists() {
        let existing: Placement = serde_json::from_slice(&fs::read(&placement_path).map_err(error)?).map_err(error)?;
        if existing != placement { return Err("Factory Project is already bound to a different native scope; no state changed".into()); }
    }
    let state_path = dir.join(STATE);
    reject_symlink(&state_path)?;
    let (_, existed) = FactoryDevelopmentalFileProvider::initialize_project(
        &state_path, Project::new(project_ref), link).map_err(error)?;
    if !placement_path.exists() {
        let bytes = serde_json::to_vec_pretty(&placement).map_err(error)?;
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(error)?.as_nanos();
        let temporary = dir.join(format!(".project-{}-{nonce}.tmp", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(error)?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(error)?;
        // Hard-link publication cannot clobber a file created by another actor.
        fs::hard_link(&temporary, &placement_path).map_err(|e| format!("Native Project state is present at {}; placement publication failed: {e}; retry setup with the same key", state_path.display()))?;
        fs::remove_file(temporary).map_err(error)?;
        fs::File::open(&dir).and_then(|f| f.sync_all()).map_err(error)?;
    }
    reading(&root, &placement, if existed {"already-present"} else {"created"})
}

pub fn execute(args: &[String]) -> Result<String, String> {
    let value = match args {
        [action, root, reference, source] if action == "setup-central" => setup_central(Path::new(root), reference, Path::new(source))?,
        [action, root] if action == "locate" => locate(Path::new(root))?,
        [action, root, key] if action == "setup" => setup(Path::new(root), key, None)?,
        [action, root, key, flag, source] if action == "setup" && flag == "--central-source" => setup(Path::new(root), key, Some(Path::new(source)))?,
        _ => return Err("usage: factory project setup <project-root> <native-project-key> [--central-source <project.json>] [--json] | factory project setup-central <project-root> <central-project-ref> <project.json> [--json] | factory project locate <project-root> [--json]".into()),
    };
    serde_json::to_string_pretty(&value).map_err(error)
}
