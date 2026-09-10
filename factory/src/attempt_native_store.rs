//! Attempts use the existing first-party developmental provider document and
//! transaction lock. Only coordinator/attempt metadata is retained beside the
//! canonical Run. The authored source below is an explicitly historical basis;
//! current source remains `FactoryDevelopmentalState.workflow_sources`.

use crate::attempt_runtime::{
    apply_operation, reading_for, validate_action_request, validate_state,
    FactoryAttemptActionReceipt, FactoryAttemptActionRequest, FactoryAttemptError,
    FactoryAttemptOperation, FactoryAttemptReading, FactoryAttemptRecord, FactoryAttemptSeed,
    PersistedAttemptAction, StoredAttemptState, FACTORY_ATTEMPT_ACTION, FACTORY_ATTEMPT_STATE,
};
use crate::build::FactoryBuildState;
use crate::core::run::{Project, RunRef};
use crate::developmental_read::{FactoryDevelopmentalFileProvider, FactoryDevelopmentalState};
use crate::orchestration::{ExecutableOrchestration, OrchestrationSnapshot};
use crate::project_development_store::{
    read_developmental_state, transact_developmental_state, ProjectDevelopmentStoreError,
};
use crate::workflow::{compile_workflow, WorkflowSource};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const FACTORY_RUN_ATTEMPTS: &str = "factory.native-run-attempts/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRunAttempts {
    schema: String,
    workflow_source: WorkflowSource,
    snapshot: OrchestrationSnapshot,
    attempts: BTreeMap<String, FactoryAttemptRecord>,
    #[serde(default)]
    action_receipts: BTreeMap<String, PersistedAttemptAction>,
}

impl FactoryRunAttempts {
    fn from_view(state: &StoredAttemptState) -> Self {
        Self {
            schema: FACTORY_RUN_ATTEMPTS.into(),
            workflow_source: state.workflow_source.clone(),
            snapshot: state.snapshot.clone(),
            attempts: state.attempts.clone(),
            action_receipts: state.action_receipts.clone(),
        }
    }

    pub(crate) fn validate_identity(&self, run_ref: &RunRef) -> Result<(), FactoryAttemptError> {
        if self.schema != FACTORY_RUN_ATTEMPTS || self.snapshot.run_ref() != run_ref {
            return Err(invalid(
                "native attempt metadata does not belong to its canonical Run",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileAttemptStore {
    path: PathBuf,
    run_ref: RunRef,
}

impl FileAttemptStore {
    /// Controlled fresh native World creation. This never overwrites a provider
    /// or claims that a seeded Run, Commission or attempt has executed.
    pub fn initialize(
        path: impl Into<PathBuf>,
        seed: FactoryAttemptSeed,
    ) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        if path.exists() {
            return Err(FactoryAttemptError::AlreadyExists(path));
        }
        let workflow = compile_workflow(seed.workflow_source.clone())?;
        let engine = ExecutableOrchestration::new(workflow, seed.run)?;
        let run_ref = engine.run().reference().clone();
        let build = FactoryBuildState::new(
            Project::new(engine.run().project_ref().clone()),
            engine.run().clone(),
        )
        .map_err(|error| invalid(&error.to_string()))?;
        let mut native = FactoryDevelopmentalState::new(build, vec![])
            .and_then(|state| state.with_workflow_sources(vec![seed.workflow_source.clone()]))
            .map_err(|error| invalid(&error.to_string()))?;
        let view = StoredAttemptState {
            schema: FACTORY_ATTEMPT_STATE.into(),
            revision: native.build.revision().get(),
            run: engine.run().clone(),
            workflow_source: seed.workflow_source,
            snapshot: engine.snapshot(),
            attempts: BTreeMap::new(),
            action_receipts: BTreeMap::new(),
        };
        validate_state(&view)?;
        native
            .attempt_states
            .insert(run_ref.clone(), FactoryRunAttempts::from_view(&view));
        FactoryDevelopmentalFileProvider::create_new(&path, native)
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(Self { path, run_ref })
    }

    /// Attach the coordinator to a Run and source already admitted by Factory's
    /// native Commission/developmental operations. The caller supplies identities,
    /// not a substitute Run, source document or mutation authority.
    pub fn attach(
        path: impl Into<PathBuf>,
        run_ref: RunRef,
        source_ref: &str,
    ) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        transact_developmental_state(&path, |native| {
            (|| -> Result<(), FactoryAttemptError> {
                if let Some(existing) = native.attempt_states.get(&run_ref) {
                    if existing.workflow_source.source.reference.to_string() != source_ref {
                        return Err(invalid("Run already has another retained workflow basis"));
                    }
                    view_for(native, &run_ref)?;
                    return Ok(());
                }
                let run = native.build.run(&run_ref).cloned().ok_or_else(|| {
                    invalid("canonical Run is not present in the native provider")
                })?;
                let source = native
                    .workflow_sources
                    .iter()
                    .find(|source| source.source.reference.to_string() == source_ref)
                    .cloned()
                    .ok_or_else(|| {
                        invalid("workflow source is not admitted in the native provider")
                    })?;
                let engine =
                    ExecutableOrchestration::new(compile_workflow(source.clone())?, run.clone())?;
                native
                    .build
                    .replace_attempt_run(run.revision(), engine.run().clone())
                    .map_err(|error| invalid(&error.to_string()))?;
                let view = StoredAttemptState {
                    schema: FACTORY_ATTEMPT_STATE.into(),
                    revision: native.build.revision().get(),
                    run: engine.run().clone(),
                    workflow_source: source,
                    snapshot: engine.snapshot(),
                    attempts: BTreeMap::new(),
                    action_receipts: BTreeMap::new(),
                };
                validate_state(&view)?;
                native
                    .attempt_states
                    .insert(run_ref.clone(), FactoryRunAttempts::from_view(&view));
                Ok(())
            })()
            .map_err(store_error)
        })
        .map_err(native_error)?;
        Self::open_run(path, run_ref)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        let native = read_developmental_state(&path).map_err(native_error)?;
        if native.attempt_states.len() != 1 {
            return Err(invalid("select a canonical Run explicitly when the provider has zero or multiple attempt fields"));
        }
        let run_ref = native
            .attempt_states
            .keys()
            .next()
            .expect("one checked entry")
            .clone();
        view_for(&native, &run_ref)?;
        Ok(Self { path, run_ref })
    }

    pub fn open_run(
        path: impl Into<PathBuf>,
        run_ref: RunRef,
    ) -> Result<Self, FactoryAttemptError> {
        let path = path.into();
        let native = read_developmental_state(&path).map_err(native_error)?;
        view_for(&native, &run_ref)?;
        Ok(Self { path, run_ref })
    }

    pub fn reading(&self) -> Result<FactoryAttemptReading, FactoryAttemptError> {
        let native = read_developmental_state(&self.path).map_err(native_error)?;
        let view = view_for(&native, &self.run_ref)?;
        let mut reading = reading_for(&view)?;
        reading.source_current = source_is_current(&native, &view);
        Ok(reading)
    }

    pub fn apply(
        &mut self,
        request: FactoryAttemptActionRequest,
    ) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
        validate_action_request(&request)?;
        if request.run_ref != self.run_ref {
            return Err(FactoryAttemptError::RunMismatch {
                addressed: request.run_ref.to_string(),
                stored: self.run_ref.to_string(),
            });
        }
        let request_digest = blake3::hash(&serde_json::to_vec(&request)?)
            .to_hex()
            .to_string();
        transact_developmental_state(&self.path, |native| {
            (|| -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
                let mut view = view_for(native, &request.run_ref)?;
                if let Some(applied) = view.action_receipts.get(&request.projection_ref) {
                    if applied.request_digest != request_digest {
                        return Err(invalid("projection identity was already used for another Action request"));
                    }
                    return Ok(applied.receipt.clone());
                }
                if view.revision != request.expected_revision {
                    return Err(FactoryAttemptError::RevisionConflict { expected: request.expected_revision, actual: view.revision });
                }
                if requires_current_source(&request.operation) && !source_is_current(native, &view) {
                    return Err(invalid("current authored source differs from retained attempt basis; explicit re-resolution is required"));
                }
                crate::attempt_application::validate_native_action(&reading_for(&view)?, &view.run, &request)?;
                let previous_revision = view.revision;
                let previous_run_revision = view.run.revision();
                let (operation, attempt_refs) = apply_operation(&mut view, request.operation.clone())?;
                native.build.replace_attempt_run(previous_run_revision, view.run.clone())
                    .map_err(|error| invalid(&error.to_string()))?;
                view.revision = native.build.revision().get();
                let receipt = FactoryAttemptActionReceipt {
                    contract: FACTORY_ATTEMPT_ACTION.into(), projection_ref: request.projection_ref.clone(),
                    run_ref: request.run_ref.clone(), previous_revision, next_revision: view.revision,
                    operation, attempt_refs,
                    standing: "native Factory developmental transaction; source/Run/attempt committed together; owner observations are not whole-feature acceptance".into(),
                };
                view.action_receipts.insert(request.projection_ref.clone(), PersistedAttemptAction {
                    request_digest: request_digest.clone(), request: request.clone(), receipt: receipt.clone(),
                });
                validate_state(&view)?;
                // Other Runs in this same native provider cannot simultaneously
                // reserve the same source subject for shared writing.
                for (other_run, other) in &native.attempt_states {
                    if other_run == &request.run_ref { continue; }
                    for (subject, unit) in other.snapshot.writer_reservations() {
                        if view.snapshot.writer_reservations().contains_key(subject) {
                            return Err(invalid(&format!("SharedWriterConflict: {subject} is reserved by {other_run} / {unit}")));
                        }
                    }
                }
                native.attempt_states.insert(request.run_ref.clone(), FactoryRunAttempts::from_view(&view));
                Ok(receipt)
            })().map_err(store_error)
        }).map_err(native_error)
    }
}

fn view_for(
    native: &FactoryDevelopmentalState,
    run_ref: &RunRef,
) -> Result<StoredAttemptState, FactoryAttemptError> {
    let retained = native.attempt_states.get(run_ref).ok_or_else(|| {
        invalid("Run has no native attempt field; attach its admitted source first")
    })?;
    retained.validate_identity(run_ref)?;
    let run = native
        .build
        .run(run_ref)
        .cloned()
        .ok_or_else(|| invalid("attempt field has no canonical Run"))?;
    let view = StoredAttemptState {
        schema: FACTORY_ATTEMPT_STATE.into(),
        revision: native.build.revision().get(),
        run,
        workflow_source: retained.workflow_source.clone(),
        snapshot: retained.snapshot.clone(),
        attempts: retained.attempts.clone(),
        action_receipts: retained.action_receipts.clone(),
    };
    validate_state(&view)?;
    Ok(view)
}

fn source_is_current(native: &FactoryDevelopmentalState, view: &StoredAttemptState) -> bool {
    native.workflow_sources.iter().any(|source| {
        source.source == view.workflow_source.source
            && source.workflow_key == view.workflow_source.workflow_key
    })
}

fn requires_current_source(operation: &FactoryAttemptOperation) -> bool {
    matches!(
        operation,
        FactoryAttemptOperation::StartSerial { .. }
            | FactoryAttemptOperation::StartFork { .. }
            | FactoryAttemptOperation::BindDispatch { .. }
            | FactoryAttemptOperation::Retry { .. }
            | FactoryAttemptOperation::ReturnArtifact { .. }
            | FactoryAttemptOperation::IncorporateLateResult { .. }
    )
}

fn invalid(message: &str) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}
fn store_error(error: FactoryAttemptError) -> ProjectDevelopmentStoreError {
    ProjectDevelopmentStoreError::Native(error.to_string())
}
fn native_error(error: ProjectDevelopmentStoreError) -> FactoryAttemptError {
    invalid(&error.to_string())
}
