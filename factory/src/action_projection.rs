//! Projection-neutral execution contract for Factory-owned Actions.
//!
//! Human UI, situated Agent and external/headless callers may present different
//! interaction bodies, but they all resolve here to the same canonical Factory
//! Action invocation and the same [`FactoryBuildFileProvider`] semantic handler.
//! Projection/caller identity is retained as lineage; it never replaces Action,
//! subject, Run, authority or Capability identity.

use crate::build::{
    FactoryActionAuthority, FactoryActionInvocation, FactoryActionReceipt, FACTORY_NATIVE_OWNER,
};
use crate::build_provider::{FactoryBuildFileProvider, FactoryBuildProviderError};
use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub const FACTORY_ACTION_PROJECTION_CONTRACT: &str = "factory.action-projection/v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryActionProjectionKind {
    DesktopHuman,
    SituatedAgent,
    Headless,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionCaller {
    pub caller_ref: String,
    pub projection_kind: FactoryActionProjectionKind,
    /// Ordered outer -> inner attribution chain. The final entry must be the
    /// effective caller. A projection may extend lineage, but never erase it.
    #[serde(default)]
    pub lineage: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectedFactoryActionAuthority {
    pub authority_ref: String,
    pub native_owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_ref: Option<String>,
    #[serde(default)]
    pub capability_granted: bool,
    #[serde(default)]
    pub action_authorised: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionProjectionRequest {
    pub contract: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub action_ref: String,
    pub subject_ref: String,
    pub run_ref: String,
    pub authority: ProjectedFactoryActionAuthority,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionProjectionReceipt {
    pub contract: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub action_ref: String,
    pub subject_ref: String,
    pub run_ref: String,
    pub authority_ref: String,
    pub native_result: FactoryActionReceipt,
}

/// Execute a projected call through the canonical persistent Factory provider.
///
/// This is intentionally the only projection-specific step: after validating
/// attribution, it constructs the existing canonical invocation/authority and
/// delegates through the selected first-party provider to `FactoryActionExecutor`.
/// No projection or provider implements Action meaning itself.
pub trait FactoryProjectedActionProvider {
    type Error: Error + Send + Sync + 'static;

    fn execute_projected_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, Self::Error>;
}

impl FactoryProjectedActionProvider for FactoryBuildFileProvider {
    type Error = FactoryBuildProviderError;

    fn execute_projected_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, Self::Error> {
        self.execute_action(invocation, authority)
    }
}

pub fn execute_projected_factory_action<P>(
    provider: &mut P,
    request: &FactoryActionProjectionRequest,
) -> Result<FactoryActionProjectionReceipt, FactoryActionProjectionError>
where
    P: FactoryProjectedActionProvider,
{
    validate_request(request)?;
    let run_ref = RunRef::from_str(&request.run_ref)
        .map_err(|error| FactoryActionProjectionError::InvalidRunRef(error.to_string()))?;

    let invocation = FactoryActionInvocation {
        action_ref: request.action_ref.clone(),
        subject_ref: request.subject_ref.clone(),
        run_ref,
    };
    let authority = FactoryActionAuthority {
        authority_ref: request.authority.authority_ref.clone(),
        native_owner: request.authority.native_owner.clone(),
        capability_ref: request.authority.capability_ref.clone(),
        capability_granted: request.authority.capability_granted,
        action_authorised: request.authority.action_authorised,
    };
    let native_result = provider
        .execute_projected_action(&invocation, &authority)
        .map_err(|error| FactoryActionProjectionError::Provider(error.to_string()))?;

    if native_result.action_ref != request.action_ref
        || native_result.subject_ref != request.subject_ref
        || native_result.authority_ref != request.authority.authority_ref
    {
        return Err(FactoryActionProjectionError::NativeResultIdentityDrift);
    }

    Ok(FactoryActionProjectionReceipt {
        contract: FACTORY_ACTION_PROJECTION_CONTRACT.into(),
        projection_ref: request.projection_ref.clone(),
        caller: request.caller.clone(),
        action_ref: request.action_ref.clone(),
        subject_ref: request.subject_ref.clone(),
        run_ref: request.run_ref.clone(),
        authority_ref: request.authority.authority_ref.clone(),
        native_result,
    })
}

fn validate_request(
    request: &FactoryActionProjectionRequest,
) -> Result<(), FactoryActionProjectionError> {
    if request.contract != FACTORY_ACTION_PROJECTION_CONTRACT {
        return Err(FactoryActionProjectionError::UnsupportedContract(
            request.contract.clone(),
        ));
    }
    if request.projection_ref.trim().is_empty() {
        return Err(FactoryActionProjectionError::MissingProjectionRef);
    }
    if request.caller.caller_ref.trim().is_empty() {
        return Err(FactoryActionProjectionError::MissingCallerRef);
    }
    if request.caller.lineage.last().map(String::as_str) != Some(request.caller.caller_ref.as_str())
    {
        return Err(FactoryActionProjectionError::CallerLineageMismatch);
    }
    if request.authority.native_owner != FACTORY_NATIVE_OWNER {
        return Err(FactoryActionProjectionError::WrongNativeOwner(
            request.authority.native_owner.clone(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum FactoryActionProjectionError {
    UnsupportedContract(String),
    MissingProjectionRef,
    MissingCallerRef,
    CallerLineageMismatch,
    WrongNativeOwner(String),
    InvalidRunRef(String),
    NativeResultIdentityDrift,
    Provider(String),
}

impl Display for FactoryActionProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedContract(contract) => {
                write!(formatter, "unsupported Factory Action projection contract `{contract}`")
            }
            Self::MissingProjectionRef => write!(formatter, "Factory Action projection requires projection_ref"),
            Self::MissingCallerRef => write!(formatter, "Factory Action projection requires caller_ref"),
            Self::CallerLineageMismatch => write!(formatter, "Factory Action caller lineage must terminate at caller_ref"),
            Self::WrongNativeOwner(owner) => write!(formatter, "Factory Action projection cannot substitute native owner `{owner}`"),
            Self::InvalidRunRef(error) => write!(formatter, "invalid Factory Run ref: {error}"),
            Self::NativeResultIdentityDrift => write!(formatter, "Factory native Action result drifted from projected Action/subject/authority identity"),
            Self::Provider(error) => formatter.write_str(error),
        }
    }
}

impl Error for FactoryActionProjectionError {}
