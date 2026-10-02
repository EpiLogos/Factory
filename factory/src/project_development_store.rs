use crate::core::run::RunRef;
use crate::native_file_transaction::{
    lock_name, read_native, retain_empty_fields, unsupported_field, NativeFileTransaction,
    PublicationError,
};
use crate::project_development::{ProjectDevelopmentLedger, PROJECT_DEVELOPMENT_VERSION};
use std::error::Error;
use std::fmt::{Display, Formatter};
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};

/// Owner-native persistence of the existing Run developmental ledger. Paths are
/// provider addresses, never substitute Project/Run/Evidence identities.
pub trait ProjectDevelopmentStore {
    fn save(&self, ledger: &ProjectDevelopmentLedger) -> Result<(), ProjectDevelopmentStoreError>;
    fn load(
        &self,
        run_ref: &RunRef,
    ) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError>;
}

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

    /// Serialize one owner-native semantic operation on the current Run ledger.
    /// Parse input and call external owners before this closure. The returned
    /// ledger is the committed state, including concurrent retained relations.
    pub fn transact(
        &self,
        run_ref: &RunRef,
        create_if_missing: bool,
        operation: impl FnOnce(
            &mut ProjectDevelopmentLedger,
        ) -> Result<(), ProjectDevelopmentStoreError>,
    ) -> Result<ProjectDevelopmentLedger, ProjectDevelopmentStoreError> {
        let transaction = self.lock(run_ref)?;
        let mut candidate = match self.load_for_mutation(run_ref, &transaction)? {
            Some(ledger) => ledger,
            None if create_if_missing => ProjectDevelopmentLedger::new(run_ref.clone()),
            None => {
                return Err(ProjectDevelopmentStoreError::Native(format!(
                    "development ledger not found for {run_ref}"
                )))
            }
        };
        operation(&mut candidate)?;
        validate_ledger(&candidate, run_ref)?;
        self.publish(&transaction, &candidate)?;
        Ok(candidate)
    }

    fn lock(
        &self,
        run_ref: &RunRef,
    ) -> Result<NativeFileTransaction, ProjectDevelopmentStoreError> {
        let name = std::ffi::OsString::from(format!(".{}.lock", run_ref.as_ref().id()));
        Ok(NativeFileTransaction::acquire(
            &self.path_for(run_ref),
            &name,
            true,
        )?)
    }

    fn load_for_mutation(
        &self,
        run_ref: &RunRef,
        transaction: &NativeFileTransaction,
    ) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError> {
        let Some(input) = transaction.current() else {
            return Ok(None);
        };
        let ledger: ProjectDevelopmentLedger = serde_json::from_slice(input)?;
        validate_ledger(&ledger, run_ref)?;
        let raw: serde_json::Value = serde_json::from_slice(input)?;
        if let Some(field) = unsupported_field(&raw, &serde_json::to_value(&ledger)?, "$") {
            return Err(ProjectDevelopmentStoreError::Native(format!(
                "mutation refused: unsupported stored field {field}; retained bytes unchanged"
            )));
        }
        Ok(Some(ledger))
    }

    fn publish(
        &self,
        transaction: &NativeFileTransaction,
        ledger: &ProjectDevelopmentLedger,
    ) -> Result<(), ProjectDevelopmentStoreError> {
        let mut encoded = serde_json::to_value(ledger)?;
        if let Some(input) = transaction.current() {
            let raw: serde_json::Value = serde_json::from_slice(input)?;
            let previous: ProjectDevelopmentLedger = serde_json::from_slice(input)?;
            retain_empty_fields(
                &raw,
                &serde_json::to_value(previous)?,
                &mut encoded,
                "$",
                ledger_record_key,
                ledger_array_key,
            )
            .map_err(ProjectDevelopmentStoreError::Native)?;
        }
        publish(
            transaction,
            &serde_json::to_vec_pretty(&encoded)?,
            transaction.current().is_none(),
        )
    }
}

fn publish(
    transaction: &NativeFileTransaction,
    encoded: &[u8],
    create_new: bool,
) -> Result<(), ProjectDevelopmentStoreError> {
    match transaction.publish(encoded, create_new) {
        Ok(()) => Ok(()),
        Err(PublicationError::Before(error)) => Err(error.into()),
        Err(PublicationError::Uncertain(error)) => {
            Err(ProjectDevelopmentStoreError::PublicationUncertain(error))
        }
    }
}

// These are explicit fields of this native ledger, not generic ref heuristics.
fn ledger_record_key(path: &str) -> Option<&'static str> {
    match path {
        "$" => Some("run_ref"),
        "$.orientation" | "$.intent" | "$.praxis" => Some("condition_ref"),
        "$.intent_return" => Some("return_ref"),
        "$.development_field" => Some("fieldRef"),
        "$.development_field.aikitOperative" => Some("resolutionRef"),
        "$.development_field.gitBasis" => Some("gitDevelopmentRef"),
        "$.development_field.targets" => Some("planRef"),
        _ => None,
    }
}
fn ledger_array_key(path: &str) -> Option<&'static str> {
    match path {
        "$.intent_return.criterion_evaluations" => Some("criterion_ref"),
        "$.reflection_anchors" => Some("anchor_ref"),
        "$.capability_rows" => Some("row_ref"),
        "$.observations" => Some("observation_ref"),
        "$.development_field.materialBindings" => Some("bindingRef"),
        "$.development_field.returns" => Some("returnRef"),
        _ => None,
    }
}

#[derive(Debug)]
pub enum ProjectDevelopmentStoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
    VersionMismatch { expected: String, actual: String },
    RunMismatch { expected: RunRef, actual: RunRef },
    Native(String),
    PublicationUncertain(crate::NativePublicationUncertainty),
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
            Self::PublicationUncertain(error) => write!(formatter, "native developmental transaction: publication is uncertain after replacement; read back owner state before retry: {error}"),
        }
    }
}
impl Error for ProjectDevelopmentStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PublicationUncertain(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}
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

fn validate_ledger(
    ledger: &ProjectDevelopmentLedger,
    run_ref: &RunRef,
) -> Result<(), ProjectDevelopmentStoreError> {
    if ledger.version != PROJECT_DEVELOPMENT_VERSION {
        return Err(ProjectDevelopmentStoreError::VersionMismatch {
            expected: PROJECT_DEVELOPMENT_VERSION.into(),
            actual: ledger.version.clone(),
        });
    }
    if &ledger.run_ref != run_ref {
        return Err(ProjectDevelopmentStoreError::RunMismatch {
            expected: run_ref.clone(),
            actual: ledger.run_ref.clone(),
        });
    }
    // Use the native ledger operations to validate every retained relation,
    // including foreign Runs and cross-record identities, without changing it.
    let mut validated = ProjectDevelopmentLedger::new(run_ref.clone());
    let native = |error: crate::project_development::ProjectDevelopmentError| {
        ProjectDevelopmentStoreError::Native(error.to_string())
    };
    if let Some(value) = &ledger.orientation {
        validated.set_orientation(value.clone()).map_err(native)?;
    }
    if let Some(value) = &ledger.intent {
        validated.set_intent(value.clone()).map_err(native)?;
    }
    if let Some(value) = &ledger.intent_return {
        validated.set_intent_return(value.clone()).map_err(native)?;
    }
    if let Some(value) = &ledger.development_field {
        validated
            .set_development_field(value.clone())
            .map_err(native)?;
    }
    for value in &ledger.reflection_anchors {
        validated
            .add_reflection_anchor(value.clone())
            .map_err(native)?;
    }
    if let Some(value) = &ledger.praxis {
        validated.set_praxis(value.clone()).map_err(native)?;
    }
    for value in &ledger.capability_rows {
        validated
            .add_capability_row(value.clone())
            .map_err(native)?;
    }
    for value in &ledger.observations {
        if value.observation_ref.trim().is_empty() {
            return Err(ProjectDevelopmentStoreError::Native(
                "development observation has empty identity".into(),
            ));
        }
        validated.add_observation(value.clone()).map_err(native)?;
    }
    Ok(())
}

impl ProjectDevelopmentStore for FileProjectDevelopmentStore {
    /// Compatibility for bootstrap and append-only observations. Metadata edits
    /// must use transact; a blind snapshot has no basis to replace current truth.
    fn save(&self, ledger: &ProjectDevelopmentLedger) -> Result<(), ProjectDevelopmentStoreError> {
        validate_ledger(ledger, &ledger.run_ref)?;
        let transaction = self.lock(&ledger.run_ref)?;
        let mut merged = ledger.clone();
        if let Some(existing) = self.load_for_mutation(&ledger.run_ref, &transaction)? {
            let mut current_metadata = existing.clone();
            current_metadata.observations.clear();
            let mut incoming_metadata = ledger.clone();
            incoming_metadata.observations.clear();
            if current_metadata != incoming_metadata {
                return Err(ProjectDevelopmentStoreError::Native("blind ledger snapshot cannot replace non-observation state; use current-state transact".into()));
            }
            merged.observations = existing.observations;
            for incoming in &ledger.observations {
                if let Some(previous) = merged
                    .observations
                    .iter()
                    .find(|previous| previous.observation_ref == incoming.observation_ref)
                {
                    if previous != incoming {
                        return Err(ProjectDevelopmentStoreError::Native(format!(
                            "conflicting observation identity {}",
                            incoming.observation_ref
                        )));
                    }
                } else {
                    merged.observations.push(incoming.clone());
                }
            }
        }
        validate_ledger(&merged, &ledger.run_ref)?;
        self.publish(&transaction, &merged)
    }
    fn load(
        &self,
        run_ref: &RunRef,
    ) -> Result<Option<ProjectDevelopmentLedger>, ProjectDevelopmentStoreError> {
        let encoded = match read_native(&self.path_for(run_ref)) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let ledger: ProjectDevelopmentLedger = serde_json::from_slice(&encoded)?;
        validate_ledger(&ledger, run_ref)?;
        Ok(Some(ledger))
    }
}

/// Reopen the existing first-party developmental provider encoding. Attempts and
/// ordinary developmental Actions read and replace this same provider document.
pub fn read_developmental_state(
    path: &Path,
) -> Result<crate::developmental_read::FactoryDevelopmentalState, ProjectDevelopmentStoreError> {
    decode_developmental_state(&read_native(path)?)
}
fn decode_developmental_state(
    input: &[u8],
) -> Result<crate::developmental_read::FactoryDevelopmentalState, ProjectDevelopmentStoreError> {
    #[derive(serde::Deserialize)]
    struct Stored {
        schema: String,
        state: crate::developmental_read::FactoryDevelopmentalState,
    }
    let stored: Stored = serde_json::from_slice(input)?;
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

/// Share the developmental provider's advisory lock and atomic synced publisher.
/// Before-publication failure retains the original; late failures are uncertain.
/// Process death releases the OS lock. Owner calls
/// happen outside this closure, after a durable reservation.
pub fn transact_developmental_state<T>(
    path: &Path,
    operation: impl FnOnce(
        &mut crate::developmental_read::FactoryDevelopmentalState,
    ) -> Result<T, ProjectDevelopmentStoreError>,
) -> Result<T, ProjectDevelopmentStoreError> {
    let transaction = NativeFileTransaction::acquire(path, &lock_name(path)?, false)?;
    let input = transaction.current().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "native developmental state not found",
        )
    })?;
    let raw: serde_json::Value = serde_json::from_slice(input)?;
    let mut candidate = decode_developmental_state(input)?;
    let before = candidate.clone();
    let known = serde_json::json!({"schema": crate::developmental_read::FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER, "state": &before});
    if let Some(field) = unsupported_field(&raw, &known, "$") {
        return Err(ProjectDevelopmentStoreError::Native(format!(
            "mutation refused: unsupported stored field {field}; retained bytes unchanged"
        )));
    }
    let result = operation(&mut candidate)?;
    candidate
        .validate()
        .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))?;
    if candidate != before {
        let mut encoded = serde_json::json!({"schema": crate::developmental_read::FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER, "state": candidate});
        // No invented identities for foreign/opaque nested provider arrays. If
        // omitted data cannot remain in the same shape, refuse before effect.
        retain_empty_fields(&raw, &known, &mut encoded, "$", |_| None, |_| None)
            .map_err(ProjectDevelopmentStoreError::Native)?;
        publish(&transaction, &serde_json::to_vec_pretty(&encoded)?, false)?;
    }
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
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            "native attempts",
            "factory",
        )
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
            state
                .build
                .insert_claim(claim(&run_ref, "claim:rollback"))
                .unwrap();
            Err(ProjectDevelopmentStoreError::Native(
                "interrupted before commit".into(),
            ))
        });
        assert!(result.is_err());
        assert_eq!(before, fs::read(&path).unwrap());
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
            state
                .build
                .insert_claim(claim(&run_ref, "claim:committed"))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap();
        let after = fs::read(&path).unwrap();
        let stale = transact_developmental_state(&path, |state| {
            if state.build.revision() != expected {
                return Err(ProjectDevelopmentStoreError::Native(
                    "stale revision".into(),
                ));
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
