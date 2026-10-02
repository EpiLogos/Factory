//! Persistent first-party local provider for the Factory Build read model.
//!
//! The provider owns persistence of canonical [`FactoryBuildState`] and delegates
//! all semantic mutation to the Factory-owned action executor. External hosts may
//! hold this provider handle, but they do not read/write the state file directly
//! and never become semantic owners of the stored Project/Run/Candidate state.

use crate::build::{
    FactoryActionAuthority, FactoryActionExecutor, FactoryActionInvocation, FactoryActionReceipt,
    FactoryBuildError, FactoryBuildSelection, FactoryBuildSnapshot, FactoryBuildState,
    FactoryBuildViewProvider,
};
use crate::native_file_transaction::{
    lock_name, read_native, retain_empty_fields, unsupported_field, NativeFileTransaction,
    PublicationError,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

pub const FACTORY_BUILD_LOCAL_PROVIDER_STATE: &str = "factory.build-local-provider-state/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredFactoryBuildState {
    schema: String,
    state: FactoryBuildState,
}

/// Product-owned local provider binding suitable for a first-party development
/// host. Persistence is an implementation detail of the Factory provider rather
/// than a mutable GUI/O:I store.
#[derive(Debug)]
pub struct FactoryBuildFileProvider {
    path: PathBuf,
    selection: FactoryBuildSelection,
    state: FactoryBuildState,
}

impl FactoryBuildFileProvider {
    pub fn create(
        path: impl Into<PathBuf>,
        selection: FactoryBuildSelection,
        state: FactoryBuildState,
    ) -> Result<Self, FactoryBuildProviderError> {
        let provider = Self {
            path: path.into(),
            selection,
            state,
        };
        provider.validate_selection()?;
        let transaction = provider.lock()?;
        provider.persist_state(&transaction, &provider.state, true)?;
        Ok(provider)
    }

    pub fn open(
        path: impl Into<PathBuf>,
        selection: FactoryBuildSelection,
    ) -> Result<Self, FactoryBuildProviderError> {
        let path = path.into();
        let state = Self::read_state(&path, false)?;
        let provider = Self {
            path,
            selection,
            state,
        };
        provider.validate_selection()?;
        Ok(provider)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn selection(&self) -> &FactoryBuildSelection {
        &self.selection
    }

    pub fn snapshot(&self) -> Result<FactoryBuildSnapshot, FactoryBuildProviderError> {
        Ok(FactoryBuildViewProvider.snapshot(&self.state, &self.selection)?)
    }

    /// Re-read the Factory-owned canonical state file. This is the explicit local
    /// change-observation seam; no watcher or ACTIVE state is fabricated.
    pub fn refresh(&mut self) -> Result<FactoryBuildSnapshot, FactoryBuildProviderError> {
        let candidate = Self::read_state(&self.path, false)?;
        let snapshot = FactoryBuildViewProvider.snapshot(&candidate, &self.selection)?;
        self.state = candidate;
        Ok(snapshot)
    }

    pub fn execute_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, FactoryBuildProviderError> {
        if invocation.run_ref != self.selection.run_ref {
            return Err(FactoryBuildProviderError::SelectionMismatch(format!(
                "Action Run {} does not match provider-selected Run {}",
                invocation.run_ref, self.selection.run_ref
            )));
        }
        let transaction = self.lock()?;
        let mut candidate = Self::decode_state(
            transaction.current().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Build provider state not found")
            })?,
            true,
        )?;
        FactoryBuildViewProvider.snapshot(&candidate, &self.selection)?;
        let receipt = FactoryActionExecutor.execute(&mut candidate, invocation, authority)?;
        FactoryBuildViewProvider.snapshot(&candidate, &self.selection)?;
        self.persist_state(&transaction, &candidate, false)?;
        self.state = candidate;
        Ok(receipt)
    }

    fn validate_selection(&self) -> Result<(), FactoryBuildProviderError> {
        FactoryBuildViewProvider
            .snapshot(&self.state, &self.selection)
            .map(|_| ())
            .map_err(FactoryBuildProviderError::Factory)
    }

    fn lock(&self) -> Result<NativeFileTransaction, FactoryBuildProviderError> {
        Ok(NativeFileTransaction::acquire(
            &self.path,
            &lock_name(&self.path)?,
            true,
        )?)
    }

    fn read_state(
        path: &Path,
        for_mutation: bool,
    ) -> Result<FactoryBuildState, FactoryBuildProviderError> {
        Self::decode_state(&read_native(path)?, for_mutation)
    }

    fn decode_state(
        input: &[u8],
        for_mutation: bool,
    ) -> Result<FactoryBuildState, FactoryBuildProviderError> {
        let stored: StoredFactoryBuildState = serde_json::from_slice(input)?;
        if stored.schema != FACTORY_BUILD_LOCAL_PROVIDER_STATE {
            return Err(FactoryBuildProviderError::UnsupportedSchema(stored.schema));
        }
        if for_mutation {
            let raw: serde_json::Value = serde_json::from_slice(input)?;
            if let Some(field) = unsupported_field(&raw, &serde_json::to_value(&stored)?, "$") {
                return Err(FactoryBuildProviderError::UnsupportedField(field));
            }
        }
        Ok(stored.state)
    }

    fn persist_state(
        &self,
        transaction: &NativeFileTransaction,
        state: &FactoryBuildState,
        create_new: bool,
    ) -> Result<(), FactoryBuildProviderError> {
        let stored = StoredFactoryBuildState {
            schema: FACTORY_BUILD_LOCAL_PROVIDER_STATE.into(),
            state: state.clone(),
        };
        let mut encoded = serde_json::to_value(&stored)?;
        if let Some(input) = transaction.current() {
            let raw: serde_json::Value = serde_json::from_slice(input)?;
            let previous: StoredFactoryBuildState = serde_json::from_slice(input)?;
            // Build records are keyed by the existing canonical maps. An array
            // with omitted data may only retain its unchanged native shape;
            // this owner grants no positional extension-reassociation authority.
            retain_empty_fields(
                &raw,
                &serde_json::to_value(previous)?,
                &mut encoded,
                "$",
                |_| None,
                |_| None,
            )
            .map_err(FactoryBuildProviderError::UnsupportedField)?;
        }
        match transaction.publish(&serde_json::to_vec_pretty(&encoded)?, create_new) {
            Ok(()) => Ok(()),
            Err(PublicationError::Before(error)) => Err(error.into()),
            Err(PublicationError::Uncertain(error)) => {
                Err(FactoryBuildProviderError::PublicationUncertain(error))
            }
        }
    }
}

#[derive(Debug)]
pub enum FactoryBuildProviderError {
    Io(io::Error),
    Json(serde_json::Error),
    Factory(FactoryBuildError),
    UnsupportedSchema(String),
    UnsupportedField(String),
    PublicationUncertain(crate::NativePublicationUncertainty),
    SelectionMismatch(String),
}

impl From<io::Error> for FactoryBuildProviderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FactoryBuildProviderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<FactoryBuildError> for FactoryBuildProviderError {
    fn from(error: FactoryBuildError) -> Self {
        Self::Factory(error)
    }
}

impl Display for FactoryBuildProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Factory Build provider I/O error: {error}"),
            Self::Json(error) => write!(formatter, "Factory Build provider JSON error: {error}"),
            Self::Factory(error) => write!(formatter, "{error}"),
            Self::UnsupportedSchema(schema) => {
                write!(
                    formatter,
                    "unsupported Factory Build provider schema `{schema}`"
                )
            }
            Self::UnsupportedField(field) => write!(formatter, "Factory Build mutation refused: unsupported stored field {field}; retained bytes unchanged"),
            Self::PublicationUncertain(error) => write!(formatter, "Factory Build publication is uncertain after replacement; read back owner state before retry: {error}"),
            Self::SelectionMismatch(detail) => write!(formatter, "{detail}"),
        }
    }
}

impl Error for FactoryBuildProviderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PublicationUncertain(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}
