use crate::core::run::RunRef;
use crate::project_development::{ProjectDevelopmentLedger, PROJECT_DEVELOPMENT_VERSION};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

/// Owner-native persistence boundary for Factory's run-scoped developmental truth.
///
/// Stores persist the existing `ProjectDevelopmentLedger`; they do not mint Project,
/// Run, Intent, ContextResolution, Candidate, or Evidence identity. Provider paths are
/// material state only and must never be treated as semantic refs.
pub trait ProjectDevelopmentStore {
    fn save(&self, ledger: &ProjectDevelopmentLedger) -> Result<(), ProjectDevelopmentStoreError>;

    fn load(
        &self,
        run_ref: &RunRef,
    ) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError>;
}

/// Small local reference provider used by deterministic acceptance and local Factory
/// operation. One JSON document is retained per canonical RunRef. The filename uses
/// only the Run ULID as provider addressing; the canonical RunRef remains inside and
/// is revalidated on every load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileProjectDevelopmentStore {
    root: PathBuf,
}

impl FileProjectDevelopmentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, run_ref: &RunRef) -> PathBuf {
        self.root.join(format!("{}.json", run_ref.as_ref().id()))
    }
}

#[derive(Debug)]
pub enum ProjectDevelopmentStoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
    VersionMismatch { expected: String, actual: String },
    RunMismatch { expected: RunRef, actual: RunRef },
    Native(String),
}

impl Display for ProjectDevelopmentStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "project-development store I/O failed: {error}"),
            Self::Json(error) => {
                write!(formatter, "project-development store JSON failed: {error}")
            }
            Self::VersionMismatch { expected, actual } => write!(
                formatter,
                "project-development store version {actual} does not match {expected}"
            ),
            Self::RunMismatch { expected, actual } => write!(
                formatter,
                "project-development store returned {actual}, expected {expected}"
            ),
            Self::Native(error) => write!(formatter, "native developmental transaction: {error}"),
        }
    }
}

impl Error for ProjectDevelopmentStoreError {}

impl From<std::io::Error> for ProjectDevelopmentStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ProjectDevelopmentStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl ProjectDevelopmentStore for FileProjectDevelopmentStore {
    fn save(&self, ledger: &ProjectDevelopmentLedger) -> Result<(), ProjectDevelopmentStoreError> {
        fs::create_dir_all(&self.root)?;
        let encoded = serde_json::to_vec_pretty(ledger)?;
        fs::write(self.path_for(&ledger.run_ref), encoded)?;
        Ok(())
    }

    fn load(
        &self,
        run_ref: &RunRef,
    ) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError> {
        let path = self.path_for(run_ref);
        let encoded = match fs::read(path) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let ledger: ProjectDevelopmentLedger = serde_json::from_slice(&encoded)?;
        if ledger.version != PROJECT_DEVELOPMENT_VERSION {
            return Err(ProjectDevelopmentStoreError::VersionMismatch {
                expected: PROJECT_DEVELOPMENT_VERSION.to_owned(),
                actual: ledger.version,
            });
        }
        if &ledger.run_ref != run_ref {
            return Err(ProjectDevelopmentStoreError::RunMismatch {
                expected: run_ref.clone(),
                actual: ledger.run_ref,
            });
        }
        Ok(Some(ledger))
    }
}

/// Reopen the existing first-party developmental provider encoding. This is not
/// a second Run store: attempt operations and ordinary developmental Actions
/// read and replace the very same provider document.
pub fn read_developmental_state(
    path: &Path,
) -> Result<crate::developmental_read::FactoryDevelopmentalState, ProjectDevelopmentStoreError> {
    #[derive(serde::Deserialize)]
    struct Stored {
        schema: String,
        state: crate::developmental_read::FactoryDevelopmentalState,
    }
    let stored: Stored = serde_json::from_slice(&fs::read(path)?)?;
    if stored.schema != crate::developmental_read::FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER {
        return Err(ProjectDevelopmentStoreError::Native(
            "unsupported native developmental provider schema".into(),
        ));
    }
    stored
        .state
        .validate()
        .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))?;
    Ok(stored.state)
}

/// A native transaction shares the developmental provider's advisory lock and
/// atomic, synced publication path. The caller must validate its Action, exact
/// revisions and current authority *inside* this closure, after reopening.
/// Failure publishes nothing; process death releases the OS lock. There is no
/// stale PID lock to delete, write-ahead copy of a Run, or transfer artifact.
/// Owner transport must run outside this closure, after durable reservation.
pub fn transact_developmental_state<T>(
    path: &Path,
    operation: impl FnOnce(
        &mut crate::developmental_read::FactoryDevelopmentalState,
    ) -> Result<T, ProjectDevelopmentStoreError>,
) -> Result<T, ProjectDevelopmentStoreError> {
    use fs2::FileExt;
    let lock_path = path.with_file_name(format!(
        ".{}.lock",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("factory-developmental-state.json")
    ));
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    let mut candidate = read_developmental_state(path)?;
    let before = candidate.clone();
    let result = operation(&mut candidate)?;
    candidate
        .validate()
        .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))?;
    if candidate != before {
        crate::developmental_read::FactoryDevelopmentalFileProvider::create(path, candidate)
            .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))?;
    }
    FileExt::unlock(&lock)?;
    Ok(result)
}

#[cfg(test)]
mod native_transaction_tests {
    use super::*;
    use crate::build::{ClaimRecord, FactoryBuildState};
    use crate::core::run::{Project, ProjectRef, Run};
    use crate::developmental_read::{FactoryDevelopmentalFileProvider, FactoryDevelopmentalState};

    fn initial(path: &Path) -> RunRef {
        let project_ref: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();
        let run_ref: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
        let run = Run::new(run_ref.clone(), project_ref.clone(), "native attempts", "factory")
            .unwrap();
        let build = FactoryBuildState::new(Project::new(project_ref), run).unwrap();
        FactoryDevelopmentalFileProvider::create(
            path,
            FactoryDevelopmentalState::new(build, vec![]).unwrap(),
        )
        .unwrap();
        run_ref
    }

    fn claim(run_ref: &RunRef, reference: &str) -> ClaimRecord {
        ClaimRecord {
            run_ref: run_ref.clone(),
            claim_ref: reference.into(),
            statement: "controlled transaction test".into(),
            status: "proposed".into(),
            evidence_refs: vec![],
        }
    }

    #[test]
    fn failed_transaction_keeps_exact_provider_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let run_ref = initial(&path);
        let before = fs::read(&path).unwrap();
        let result: Result<(), _> = transact_developmental_state(&path, |state| {
            state.build.insert_claim(claim(&run_ref, "claim:rollback")).unwrap();
            Err(ProjectDevelopmentStoreError::Native("interrupted before commit".into()))
        });
        assert!(result.is_err());
        assert_eq!(before, fs::read(&path).unwrap());
        // The error path released the actual OS lock.
        transact_developmental_state(&path, |_| Ok(())).unwrap();
    }

    #[test]
    fn reopened_transaction_detects_stale_revision_and_public_provider_reads_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let run_ref = initial(&path);
        let expected = read_developmental_state(&path).unwrap().build.revision();
        transact_developmental_state(&path, |state| {
            assert_eq!(state.build.revision(), expected);
            state.build.insert_claim(claim(&run_ref, "claim:committed"))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap();
        let after = fs::read(&path).unwrap();
        let stale = transact_developmental_state(&path, |state| {
            if state.build.revision() != expected {
                return Err(ProjectDevelopmentStoreError::Native("stale revision".into()));
            }
            Ok(())
        });
        assert!(stale.is_err());
        assert_eq!(after, fs::read(&path).unwrap());
        FactoryDevelopmentalFileProvider::open(&path)
            .unwrap()
            .run_reading(&run_ref)
            .unwrap();
    }
}
