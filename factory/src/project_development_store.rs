use crate::core::run::RunRef;
use crate::project_development::{ProjectDevelopmentLedger, PROJECT_DEVELOPMENT_VERSION};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Owner-native persistence of the existing Run developmental ledger. Paths are
/// provider addresses, never substitute Project/Run/Evidence identities.
pub trait ProjectDevelopmentStore {
    fn save(&self, ledger: &ProjectDevelopmentLedger) -> Result<(), ProjectDevelopmentStoreError>;
    fn load(&self, run_ref: &RunRef) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileProjectDevelopmentStore { root: PathBuf }
impl FileProjectDevelopmentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } }
    pub fn root(&self) -> &Path { &self.root }
    fn path_for(&self, run_ref: &RunRef) -> PathBuf { self.root.join(format!("{}.json",run_ref.as_ref().id())) }
}

#[derive(Debug)]
pub enum ProjectDevelopmentStoreError {
    Io(std::io::Error), Json(serde_json::Error),
    VersionMismatch { expected:String, actual:String },
    RunMismatch { expected:RunRef, actual:RunRef }, Native(String),
}
impl Display for ProjectDevelopmentStoreError {
    fn fmt(&self,formatter:&mut Formatter<'_>)->std::fmt::Result {
        match self {
            Self::Io(error)=>write!(formatter,"project-development store I/O failed: {error}"),
            Self::Json(error)=>write!(formatter,"project-development store JSON failed: {error}"),
            Self::VersionMismatch{expected,actual}=>write!(formatter,"project-development store version {actual} does not match {expected}"),
            Self::RunMismatch{expected,actual}=>write!(formatter,"project-development store returned {actual}, expected {expected}"),
            Self::Native(error)=>write!(formatter,"native developmental transaction: {error}"),
        }
    }
}
impl Error for ProjectDevelopmentStoreError {}
impl From<std::io::Error> for ProjectDevelopmentStoreError {fn from(error:std::io::Error)->Self{Self::Io(error)}}
impl From<serde_json::Error> for ProjectDevelopmentStoreError {fn from(error:serde_json::Error)->Self{Self::Json(error)}}

fn validate_ledger(ledger:&ProjectDevelopmentLedger,run_ref:&RunRef)->Result<(),ProjectDevelopmentStoreError> {
    if ledger.version!=PROJECT_DEVELOPMENT_VERSION {
        return Err(ProjectDevelopmentStoreError::VersionMismatch{expected:PROJECT_DEVELOPMENT_VERSION.into(),actual:ledger.version.clone()});
    }
    if &ledger.run_ref!=run_ref {
        return Err(ProjectDevelopmentStoreError::RunMismatch{expected:run_ref.clone(),actual:ledger.run_ref.clone()});
    }
    let mut observations=std::collections::BTreeSet::new();
    for observation in &ledger.observations {
        if &observation.run_ref!=run_ref||observation.observation_ref.trim().is_empty()
            ||!observations.insert(&observation.observation_ref) {
            return Err(ProjectDevelopmentStoreError::Native("development observation has duplicate or foreign Run identity".into()));
        }
    }
    Ok(())
}

impl ProjectDevelopmentStore for FileProjectDevelopmentStore {
    fn save(&self,ledger:&ProjectDevelopmentLedger)->Result<(),ProjectDevelopmentStoreError> {
        use fs2::FileExt;
        validate_ledger(ledger,&ledger.run_ref)?;
        fs::create_dir_all(&self.root)?;
        let path=self.path_for(&ledger.run_ref);
        let lock_path=self.root.join(format!(".{}.lock",ledger.run_ref.as_ref().id()));
        let lock=fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(lock_path)?;
        lock.lock_exclusive()?;
        if fs::symlink_metadata(&path).is_ok_and(|metadata|metadata.file_type().is_symlink()) {
            return Err(ProjectDevelopmentStoreError::Native("development ledger must not replace a symlink target".into()));
        }
        let existing=self.load(&ledger.run_ref)?;
        let mut merged=ledger.clone();
        if let Some(existing)=existing {
            // Observations are append-only native records. A caller that read an
            // earlier snapshot cannot erase another process's later observation.
            merged.observations=existing.observations;
            for incoming in &ledger.observations {
                if let Some(previous)=merged.observations.iter().find(|previous|previous.observation_ref==incoming.observation_ref) {
                    if previous!=incoming {return Err(ProjectDevelopmentStoreError::Native(format!("conflicting observation identity {}",incoming.observation_ref)));}
                }else{merged.observations.push(incoming.clone());}
            }
        }
        validate_ledger(&merged,&ledger.run_ref)?;
        let encoded=serde_json::to_vec_pretty(&merged)?;
        if fs::read(&path).is_ok_and(|current|current==encoded) {return Ok(());}
        let temporary=self.root.join(format!(".{}.{}.tmp",ledger.run_ref.as_ref().id(),ulid::Ulid::new()));
        let publication=(||->std::io::Result<()>{
            let mut file=fs::OpenOptions::new().create_new(true).write(true).open(&temporary)?;
            file.write_all(&encoded)?;file.sync_all()?;drop(file);
            fs::rename(&temporary,&path)?;
            #[cfg(unix)]
            fs::File::open(&self.root)?.sync_all()?;
            Ok(())
        })();
        if publication.is_err(){let _=fs::remove_file(&temporary);}
        publication?;
        FileExt::unlock(&lock)?;
        Ok(())
    }
    fn load(&self,run_ref:&RunRef)->Result<Option<ProjectDevelopmentLedger>,ProjectDevelopmentStoreError> {
        let encoded=match fs::read(self.path_for(run_ref)) {
            Ok(encoded)=>encoded,Err(error)if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(error)=>return Err(error.into()),
        };
        let ledger:ProjectDevelopmentLedger=serde_json::from_slice(&encoded)?;
        validate_ledger(&ledger,run_ref)?;
        Ok(Some(ledger))
    }
}

/// Reopen the existing first-party developmental provider encoding. Attempts and
/// ordinary developmental Actions read and replace this same provider document.
pub fn read_developmental_state(path:&Path)->Result<crate::developmental_read::FactoryDevelopmentalState,ProjectDevelopmentStoreError> {
    #[derive(serde::Deserialize)]
    struct Stored{schema:String,state:crate::developmental_read::FactoryDevelopmentalState}
    let stored:Stored=serde_json::from_slice(&fs::read(path)?)?;
    if stored.schema!=crate::developmental_read::FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER {
        return Err(ProjectDevelopmentStoreError::Native("unsupported native developmental provider schema".into()));
    }
    stored.state.validate().map_err(|error|ProjectDevelopmentStoreError::Native(error.to_string()))?;
    Ok(stored.state)
}

/// Share the developmental provider's advisory lock and atomic synced publisher.
/// Failure publishes nothing; process death releases the OS lock. Owner calls
/// happen outside this closure, after a durable reservation.
pub fn transact_developmental_state<T>(path:&Path,operation:impl FnOnce(&mut crate::developmental_read::FactoryDevelopmentalState)->Result<T,ProjectDevelopmentStoreError>)->Result<T,ProjectDevelopmentStoreError> {
    use fs2::FileExt;
    let lock_path=path.with_file_name(format!(".{}.lock",path.file_name().and_then(|value|value.to_str()).unwrap_or("factory-developmental-state.json")));
    let lock=fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(lock_path)?;
    lock.lock_exclusive()?;
    let mut candidate=read_developmental_state(path)?;let before=candidate.clone();
    let result=operation(&mut candidate)?;
    candidate.validate().map_err(|error|ProjectDevelopmentStoreError::Native(error.to_string()))?;
    if candidate!=before {crate::developmental_read::FactoryDevelopmentalFileProvider::create(path,candidate).map_err(|error|ProjectDevelopmentStoreError::Native(error.to_string()))?;}
    FileExt::unlock(&lock)?;Ok(result)
}

#[cfg(test)]
mod native_transaction_tests {
    use super::*;
    use crate::build::{ClaimRecord,FactoryBuildState};
    use crate::core::run::{Project,ProjectRef,Run};
    use crate::developmental_read::{FactoryDevelopmentalFileProvider,FactoryDevelopmentalState};
    fn initial(path:&Path)->RunRef {
        let project_ref:ProjectRef="project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();let run_ref:RunRef="run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
        let run=Run::new(run_ref.clone(),project_ref.clone(),"native attempts","factory").unwrap();let build=FactoryBuildState::new(Project::new(project_ref),run).unwrap();
        FactoryDevelopmentalFileProvider::create(path,FactoryDevelopmentalState::new(build,vec![]).unwrap()).unwrap();run_ref
    }
    fn claim(run_ref:&RunRef,reference:&str)->ClaimRecord {
        ClaimRecord{run_ref:run_ref.clone(),claim_ref:reference.into(),statement:"controlled transaction test".into(),status:"proposed".into(),evidence_refs:vec![]}
    }
    #[test]
    fn failed_transaction_keeps_exact_provider_bytes() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let run_ref=initial(&path);let before=fs::read(&path).unwrap();
        let result:Result<(),_>=transact_developmental_state(&path,|state|{state.build.insert_claim(claim(&run_ref,"claim:rollback")).unwrap();Err(ProjectDevelopmentStoreError::Native("interrupted before commit".into()))});
        assert!(result.is_err());assert_eq!(before,fs::read(&path).unwrap());transact_developmental_state(&path,|_|Ok(())).unwrap();
    }
    #[test]
    fn reopened_transaction_detects_stale_revision_and_public_provider_reads_commit() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let run_ref=initial(&path);let expected=read_developmental_state(&path).unwrap().build.revision();
        transact_developmental_state(&path,|state|{assert_eq!(state.build.revision(),expected);state.build.insert_claim(claim(&run_ref,"claim:committed")).map_err(|error|ProjectDevelopmentStoreError::Native(error.to_string()))}).unwrap();
        let after=fs::read(&path).unwrap();let stale=transact_developmental_state(&path,|state|{if state.build.revision()!=expected{return Err(ProjectDevelopmentStoreError::Native("stale revision".into()));}Ok(())});
        assert!(stale.is_err());assert_eq!(after,fs::read(&path).unwrap());FactoryDevelopmentalFileProvider::open(&path).unwrap().run_reading(&run_ref).unwrap();
    }
}
