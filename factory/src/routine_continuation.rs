//! Factory continuation of one Journey from accepted AIKit invocation evidence.
//!
//! AIKit owns Routine/proof/trigger/authority-receipt semantics. Factory retains
//! that evidence unchanged and owns only the Journey -> bounded Run consequence.
//! No scheduler, delivery, Action execution, Activity, Return or DAY state is
//! inferred at this boundary.

use crate::core::identity::Ref;
use crate::core::run::{Run, RunRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::journey::{JourneyRef, JourneyStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use ulid::Ulid;

pub const AIKIT_ROUTINE_INVOCATION_CONTRACT: &str = "aikit.routine-invocation-evidence/v1";
/// Accepted AIKit owner revision containing this public contract.
pub const AIKIT_ROUTINE_INVOCATION_REVISION: &str = "7864b3c700b26e90e7b814295c9a2675aae8dbf7";
pub const AIKIT_ROUTINE_INVOCATION_SCHEMA_SHA256: &str =
    "69691cb240eb10a124a7afa9781445e506f4cb4e791b1f96b19d30becfdbad51";
pub const FACTORY_ROUTINE_CONTINUATION_REQUEST: &str = "factory.routine-continuation-request/v1";
pub const FACTORY_ROUTINE_CONTINUATION_CONTRACT: &str = "factory.routine-continuation/v1";
pub const FACTORY_ROUTINE_CONTINUATION_ADMISSION: &str =
    "factory.routine-continuation-admission/v1";
pub const FACTORY_ROUTINE_CONTINUATION_READING: &str = "factory.routine-continuation-reading/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum AikitRoutineTrigger {
    Manual,
    Schedule { schedule_ref: String },
    Event { event_ref: String },
    External { trigger_ref: String },
}

impl AikitRoutineTrigger {
    fn validate(&self) -> Result<(), RoutineContinuationError> {
        match self {
            Self::Manual => Ok(()),
            Self::Schedule { schedule_ref } => required(schedule_ref, "trigger.schedule_ref"),
            Self::Event { event_ref } => required(event_ref, "trigger.event_ref"),
            Self::External { trigger_ref } => required(trigger_ref, "trigger.trigger_ref"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AikitRoutineAuthorityValidation {
    pub validation_ref: String,
    pub authority_ref: String,
    pub authority_revision: String,
    pub validated_at: String,
    pub granted: bool,
    pub unattended: bool,
    pub standing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AikitRoutineProviderDelivery {
    pub provider: String,
    pub delivery_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AikitRoutineInvocationEvidence {
    pub schema: String,
    pub owner: String,
    pub invocation_ref: String,
    pub routine_ref: String,
    pub routine_source: String,
    pub routine_revision: String,
    pub routine_state: String,
    pub method_ref: String,
    pub method_revision: String,
    pub proof_ref: String,
    pub proof_standing: String,
    pub context_resolution_ref: String,
    pub trigger_observation_ref: String,
    pub trigger: AikitRoutineTrigger,
    pub trigger_observed_at: String,
    pub authority_validation: AikitRoutineAuthorityValidation,
    pub action_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_ref: Option<String>,
    pub context_scope_refs: Vec<String>,
    pub provider_deliveries: Vec<AikitRoutineProviderDelivery>,
}

impl AikitRoutineInvocationEvidence {
    fn same_invocation_basis(&self, other: &Self) -> bool {
        let mut left = self.clone();
        let mut right = other.clone();
        left.provider_deliveries.clear();
        right.provider_deliveries.clear();
        left == right
    }

    pub fn validate(&self) -> Result<(), RoutineContinuationError> {
        if self.schema != AIKIT_ROUTINE_INVOCATION_CONTRACT || self.owner != "aikit" {
            return Err(RoutineContinuationError::UnsupportedAikitContract);
        }
        if self.routine_state != "enabled" {
            return Err(RoutineContinuationError::RoutineNotEnabled(
                self.routine_state.clone(),
            ));
        }
        if self.proof_standing != "current-on-supplied-basis" {
            return Err(RoutineContinuationError::ProofNotCurrentOnSuppliedBasis(
                self.proof_standing.clone(),
            ));
        }
        for (field, value) in [
            ("invocation_ref", &self.invocation_ref),
            ("routine_ref", &self.routine_ref),
            ("routine_source", &self.routine_source),
            ("routine_revision", &self.routine_revision),
            ("method_ref", &self.method_ref),
            ("method_revision", &self.method_revision),
            ("proof_ref", &self.proof_ref),
            ("context_resolution_ref", &self.context_resolution_ref),
            ("trigger_observation_ref", &self.trigger_observation_ref),
        ] {
            required(value, field)?;
        }
        self.trigger.validate()?;
        let observed = timestamp(&self.trigger_observed_at, "trigger_observed_at")?;
        let authority = &self.authority_validation;
        for (field, value) in [
            ("authority.validation_ref", &authority.validation_ref),
            ("authority.authority_ref", &authority.authority_ref),
            (
                "authority.authority_revision",
                &authority.authority_revision,
            ),
        ] {
            required(value, field)?;
        }
        if !authority.granted || authority.standing != "owner-attested" {
            return Err(RoutineContinuationError::AuthorityNotOwnerAttested);
        }
        let validated = timestamp(&authority.validated_at, "authority.validated_at")?;
        if validated < observed {
            return Err(RoutineContinuationError::AuthorityPrecedesTrigger);
        }
        unique_nonempty(&self.action_refs, "action_refs", true)?;
        unique_nonempty(&self.context_scope_refs, "context_scope_refs", false)?;
        if let Some(profile) = &self.agent_profile_ref {
            required(profile, "agent_profile_ref")?;
        }
        let mut deliveries = BTreeSet::new();
        for delivery in &self.provider_deliveries {
            required(&delivery.provider, "provider_delivery.provider")?;
            required(&delivery.delivery_ref, "provider_delivery.delivery_ref")?;
            if !deliveries.insert(&delivery.delivery_ref) {
                return Err(RoutineContinuationError::DuplicateRef(
                    "provider_deliveries.delivery_ref".into(),
                ));
            }
            for (field, value) in [
                (
                    "provider_delivery.provider_job_id",
                    &delivery.provider_job_id,
                ),
                ("provider_delivery.restart_ref", &delivery.restart_ref),
            ] {
                if let Some(value) = value {
                    required(value, field)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRoutineContinuationRequest {
    pub contract: String,
    pub journey_ref: JourneyRef,
    pub destination: String,
    pub write_owner: String,
    pub invocation_evidence: AikitRoutineInvocationEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRoutineContinuation {
    pub contract: String,
    pub continuation_ref: Ref,
    pub revision: crate::core::identity::Revision,
    pub journey_ref: JourneyRef,
    pub journey_revision_before: crate::core::identity::Revision,
    pub run_ref: RunRef,
    pub run_destination: String,
    pub write_owner: String,
    pub admitted_at: String,
    pub owner_contract_revision: String,
    pub owner_schema_sha256: String,
    pub invocation_evidence: AikitRoutineInvocationEvidence,
    #[serde(default)]
    pub activity_refs: Vec<String>,
    #[serde(default)]
    pub return_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryRoutineContinuationAdmissionStatus {
    Applied,
    AlreadyApplied,
    DeliveryRecorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRoutineContinuationAdmission {
    pub contract: String,
    pub status: FactoryRoutineContinuationAdmissionStatus,
    pub continuation: FactoryRoutineContinuation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRoutineContinuationEdge {
    pub subject_ref: String,
    pub relation: String,
    pub object_ref: String,
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRoutineContinuationReading {
    pub contract: String,
    pub continuation: FactoryRoutineContinuation,
    pub traversal: Vec<FactoryRoutineContinuationEdge>,
}

impl FactoryRoutineContinuation {
    pub fn validate(&self) -> Result<(), RoutineContinuationError> {
        if self.contract != FACTORY_ROUTINE_CONTINUATION_CONTRACT
            || self.continuation_ref.kind() != "routine-continuation"
            || self.owner_contract_revision != AIKIT_ROUTINE_INVOCATION_REVISION
            || self.owner_schema_sha256 != AIKIT_ROUTINE_INVOCATION_SCHEMA_SHA256
        {
            return Err(RoutineContinuationError::InvalidStoredRelation);
        }
        required(&self.run_destination, "run_destination")?;
        required(&self.write_owner, "write_owner")?;
        timestamp(&self.admitted_at, "admitted_at")?;
        self.invocation_evidence.validate()?;
        if derived_run_ref(
            &self.journey_ref,
            &self.invocation_evidence.invocation_ref,
            &self.invocation_evidence.trigger_observed_at,
        )? != self.run_ref
            || derived_continuation_ref(&self.run_ref)? != self.continuation_ref
        {
            return Err(RoutineContinuationError::InvalidStoredRelation);
        }
        unique_nonempty(&self.activity_refs, "activity_refs", false)?;
        unique_nonempty(&self.return_refs, "return_refs", false)
    }

    pub fn reading(&self) -> FactoryRoutineContinuationReading {
        let invocation = &self.invocation_evidence;
        let mut traversal = vec![
            edge(
                &self.journey_ref.to_string(),
                "continued-by",
                &self.run_ref.to_string(),
                "factory",
            ),
            edge(
                &self.run_ref.to_string(),
                "admitted-from",
                &invocation.invocation_ref,
                "aikit",
            ),
            edge(
                &invocation.invocation_ref,
                "invokes",
                &invocation.routine_ref,
                "aikit",
            ),
            edge(
                &invocation.routine_ref,
                "proven-by",
                &invocation.proof_ref,
                "aikit",
            ),
            edge(
                &invocation.invocation_ref,
                "triggered-by",
                &invocation.trigger_observation_ref,
                "aikit",
            ),
            edge(
                &invocation.invocation_ref,
                "authority-attested-by",
                &invocation.authority_validation.validation_ref,
                "aikit",
            ),
        ];
        traversal.extend(invocation.provider_deliveries.iter().map(|delivery| {
            edge(
                &invocation.invocation_ref,
                "delivered-by",
                &delivery.delivery_ref,
                "provider",
            )
        }));
        traversal.extend(self.activity_refs.iter().map(|reference| {
            edge(
                &self.run_ref.to_string(),
                "activity",
                reference,
                "actuation",
            )
        }));
        traversal.extend(self.return_refs.iter().map(|reference| {
            edge(
                &self.run_ref.to_string(),
                "returned-as",
                reference,
                "actuation",
            )
        }));
        FactoryRoutineContinuationReading {
            contract: FACTORY_ROUTINE_CONTINUATION_READING.into(),
            continuation: self.clone(),
            traversal,
        }
    }
}

impl FactoryDevelopmentalState {
    pub fn admit_routine_continuation(
        &mut self,
        request: FactoryRoutineContinuationRequest,
    ) -> Result<FactoryRoutineContinuationAdmission, RoutineContinuationError> {
        self.admit_routine_continuation_at(
            request,
            DateTime::<Utc>::from(std::time::SystemTime::now()),
        )
    }

    pub(crate) fn admit_routine_continuation_at(
        &mut self,
        request: FactoryRoutineContinuationRequest,
        admitted_at: DateTime<Utc>,
    ) -> Result<FactoryRoutineContinuationAdmission, RoutineContinuationError> {
        let mut candidate = self.clone();
        let admission = candidate.apply_routine_continuation(request, admitted_at)?;
        candidate.validate().map_err(|error| {
            RoutineContinuationError::InvalidDevelopmentalState(error.to_string())
        })?;
        *self = candidate;
        Ok(admission)
    }

    fn apply_routine_continuation(
        &mut self,
        request: FactoryRoutineContinuationRequest,
        admitted_at: DateTime<Utc>,
    ) -> Result<FactoryRoutineContinuationAdmission, RoutineContinuationError> {
        if request.contract != FACTORY_ROUTINE_CONTINUATION_REQUEST {
            return Err(RoutineContinuationError::UnsupportedFactoryRequest);
        }
        required(&request.destination, "destination")?;
        required(&request.write_owner, "write_owner")?;
        request.invocation_evidence.validate()?;

        if let Some(index) = self.routine_continuations.iter().position(|item| {
            item.invocation_evidence.invocation_ref == request.invocation_evidence.invocation_ref
        }) {
            let existing = &self.routine_continuations[index];
            if existing.journey_ref != request.journey_ref
                || existing.run_destination != request.destination
                || existing.write_owner != request.write_owner
                || !existing
                    .invocation_evidence
                    .same_invocation_basis(&request.invocation_evidence)
            {
                return Err(RoutineContinuationError::InvocationConflict(
                    request.invocation_evidence.invocation_ref,
                ));
            }
            let mut merged = existing.clone();
            let mut changed = false;
            for delivery in request.invocation_evidence.provider_deliveries {
                if self
                    .routine_continuations
                    .iter()
                    .enumerate()
                    .any(|(other_index, other)| {
                        other_index != index
                            && other
                                .invocation_evidence
                                .provider_deliveries
                                .iter()
                                .any(|prior| prior.delivery_ref == delivery.delivery_ref)
                    })
                {
                    return Err(RoutineContinuationError::ProviderDeliveryConflict(
                        delivery.delivery_ref,
                    ));
                }
                if let Some(prior) = merged
                    .invocation_evidence
                    .provider_deliveries
                    .iter()
                    .find(|prior| prior.delivery_ref == delivery.delivery_ref)
                {
                    if prior != &delivery {
                        return Err(RoutineContinuationError::ProviderDeliveryConflict(
                            delivery.delivery_ref,
                        ));
                    }
                } else {
                    merged
                        .invocation_evidence
                        .provider_deliveries
                        .push(delivery);
                    changed = true;
                }
            }
            merged
                .invocation_evidence
                .provider_deliveries
                .sort_by(|left, right| left.delivery_ref.cmp(&right.delivery_ref));
            if changed {
                merged.revision = merged
                    .revision
                    .next()
                    .ok_or(RoutineContinuationError::RevisionOverflow)?;
            }
            merged.validate()?;
            self.routine_continuations[index] = merged.clone();
            return Ok(FactoryRoutineContinuationAdmission {
                contract: FACTORY_ROUTINE_CONTINUATION_ADMISSION.into(),
                status: if changed {
                    FactoryRoutineContinuationAdmissionStatus::DeliveryRecorded
                } else {
                    FactoryRoutineContinuationAdmissionStatus::AlreadyApplied
                },
                continuation: merged,
            });
        }

        for existing in &self.routine_continuations {
            if existing.invocation_evidence.trigger_observation_ref
                == request.invocation_evidence.trigger_observation_ref
            {
                return Err(RoutineContinuationError::TriggerObservationConflict(
                    request.invocation_evidence.trigger_observation_ref,
                ));
            }
            let prior = existing
                .invocation_evidence
                .provider_deliveries
                .iter()
                .map(|delivery| &delivery.delivery_ref);
            for candidate in &request.invocation_evidence.provider_deliveries {
                if prior
                    .clone()
                    .any(|identity| identity == &candidate.delivery_ref)
                {
                    return Err(RoutineContinuationError::ProviderDeliveryConflict(
                        candidate.delivery_ref.clone(),
                    ));
                }
            }
        }

        let journey_index = self
            .journeys
            .iter()
            .position(|journey| journey.journey_ref == request.journey_ref)
            .ok_or_else(|| {
                RoutineContinuationError::JourneyNotFound(request.journey_ref.to_string())
            })?;
        let journey = &self.journeys[journey_index];
        if !matches!(
            journey.status,
            JourneyStatus::Active | JourneyStatus::Paused
        ) {
            return Err(RoutineContinuationError::JourneyNotContinuable(
                journey.status,
            ));
        }
        let journey_revision_before = journey.revision;
        let run_ref = derived_run_ref(
            &request.journey_ref,
            &request.invocation_evidence.invocation_ref,
            &request.invocation_evidence.trigger_observed_at,
        )?;
        if self.build.run(&run_ref).is_some() {
            return Err(RoutineContinuationError::RunIdentityConflict(
                run_ref.to_string(),
            ));
        }
        let continuation_ref = derived_continuation_ref(&run_ref)?;
        let run = Run::new(
            run_ref.clone(),
            journey.project_ref.clone(),
            request.destination.clone(),
            request.write_owner.clone(),
        )?;
        self.build.insert_run(run)?;
        let basis_refs = continuation_basis(&request.invocation_evidence);
        self.journeys[journey_index].add_run(run_ref.clone(), basis_refs, Vec::new())?;
        let continuation = FactoryRoutineContinuation {
            contract: FACTORY_ROUTINE_CONTINUATION_CONTRACT.into(),
            continuation_ref,
            revision: crate::core::identity::Revision::INITIAL,
            journey_ref: request.journey_ref,
            journey_revision_before,
            run_ref,
            run_destination: request.destination,
            write_owner: request.write_owner,
            admitted_at: admitted_at.to_rfc3339(),
            owner_contract_revision: AIKIT_ROUTINE_INVOCATION_REVISION.into(),
            owner_schema_sha256: AIKIT_ROUTINE_INVOCATION_SCHEMA_SHA256.into(),
            invocation_evidence: request.invocation_evidence,
            activity_refs: Vec::new(),
            return_refs: Vec::new(),
        };
        continuation.validate()?;
        self.routine_continuations.push(continuation.clone());
        self.routine_continuations.sort_by(|left, right| {
            left.invocation_evidence
                .invocation_ref
                .cmp(&right.invocation_evidence.invocation_ref)
        });
        Ok(FactoryRoutineContinuationAdmission {
            contract: FACTORY_ROUTINE_CONTINUATION_ADMISSION.into(),
            status: FactoryRoutineContinuationAdmissionStatus::Applied,
            continuation,
        })
    }

    pub fn routine_continuation_reading(
        &self,
        invocation_ref: &str,
    ) -> Result<FactoryRoutineContinuationReading, RoutineContinuationError> {
        self.routine_continuations
            .iter()
            .find(|item| item.invocation_evidence.invocation_ref == invocation_ref)
            .map(FactoryRoutineContinuation::reading)
            .ok_or_else(|| RoutineContinuationError::InvocationNotFound(invocation_ref.into()))
    }
}

fn continuation_basis(evidence: &AikitRoutineInvocationEvidence) -> Vec<String> {
    let mut refs = vec![
        evidence.invocation_ref.clone(),
        evidence.routine_ref.clone(),
        evidence.proof_ref.clone(),
        evidence.trigger_observation_ref.clone(),
        evidence.authority_validation.validation_ref.clone(),
        evidence.context_resolution_ref.clone(),
    ];
    refs.extend(
        evidence
            .provider_deliveries
            .iter()
            .map(|item| item.delivery_ref.clone()),
    );
    refs.sort();
    refs.dedup();
    refs
}

fn derived_run_ref(
    journey_ref: &JourneyRef,
    invocation_ref: &str,
    observed_at: &str,
) -> Result<RunRef, RoutineContinuationError> {
    let observed = timestamp(observed_at, "trigger_observed_at")?;
    let timestamp_ms = u64::try_from(observed.timestamp_millis())
        .ok()
        .filter(|value| *value < (1_u64 << 48))
        .ok_or(RoutineContinuationError::TriggerTimestampOutOfRange)?;
    let id = deterministic_ulid(
        timestamp_ms,
        format!("{}\0{}", journey_ref, invocation_ref).as_bytes(),
    );
    RunRef::try_from(Ref::new("run", id)?).map_err(RoutineContinuationError::from)
}

fn derived_continuation_ref(run_ref: &RunRef) -> Result<Ref, RoutineContinuationError> {
    Ok(Ref::new(
        "routine-continuation",
        deterministic_ulid(
            run_ref.as_ref().id().timestamp_ms(),
            run_ref.to_string().as_bytes(),
        ),
    )?)
}

fn deterministic_ulid(timestamp_ms: u64, basis: &[u8]) -> Ulid {
    let digest = blake3::hash(basis);
    let mut random = [0_u8; 16];
    random[6..].copy_from_slice(&digest.as_bytes()[..10]);
    Ulid::from_parts(timestamp_ms, u128::from_be_bytes(random))
}

fn edge(
    subject: &str,
    relation: &str,
    object: &str,
    owner: &str,
) -> FactoryRoutineContinuationEdge {
    FactoryRoutineContinuationEdge {
        subject_ref: subject.into(),
        relation: relation.into(),
        object_ref: object.into(),
        owner: owner.into(),
    }
}

fn required(value: &str, field: &str) -> Result<(), RoutineContinuationError> {
    if value.trim().is_empty() || value != value.trim() {
        Err(RoutineContinuationError::InvalidField(field.into()))
    } else {
        Ok(())
    }
}

fn unique_nonempty(
    values: &[String],
    field: &str,
    required_values: bool,
) -> Result<(), RoutineContinuationError> {
    if required_values && values.is_empty() {
        return Err(RoutineContinuationError::InvalidField(field.into()));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        required(value, field)?;
        if !seen.insert(value) {
            return Err(RoutineContinuationError::DuplicateRef(field.into()));
        }
    }
    Ok(())
}

fn timestamp(value: &str, field: &str) -> Result<DateTime<Utc>, RoutineContinuationError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| RoutineContinuationError::InvalidTimestamp(field.into()))
}

#[derive(Debug)]
pub enum RoutineContinuationError {
    UnsupportedAikitContract,
    UnsupportedFactoryRequest,
    RoutineNotEnabled(String),
    ProofNotCurrentOnSuppliedBasis(String),
    AuthorityNotOwnerAttested,
    AuthorityPrecedesTrigger,
    InvalidField(String),
    InvalidTimestamp(String),
    TriggerTimestampOutOfRange,
    RevisionOverflow,
    DuplicateRef(String),
    InvalidStoredRelation,
    InvalidDevelopmentalState(String),
    JourneyNotFound(String),
    JourneyNotContinuable(JourneyStatus),
    InvocationConflict(String),
    TriggerObservationConflict(String),
    ProviderDeliveryConflict(String),
    RunIdentityConflict(String),
    InvocationNotFound(String),
    Identity(crate::core::identity::RefParseError),
    TypedIdentity(crate::core::run::TypedRefError),
    Run(crate::core::run::RunContractError),
    Build(crate::build::FactoryBuildError),
    Journey(crate::journey::JourneyError),
}

impl Display for RoutineContinuationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory Routine continuation error: {self:?}")
    }
}

impl Error for RoutineContinuationError {}

impl From<crate::core::identity::RefParseError> for RoutineContinuationError {
    fn from(value: crate::core::identity::RefParseError) -> Self {
        Self::Identity(value)
    }
}
impl From<crate::core::run::TypedRefError> for RoutineContinuationError {
    fn from(value: crate::core::run::TypedRefError) -> Self {
        Self::TypedIdentity(value)
    }
}
impl From<crate::core::run::RunContractError> for RoutineContinuationError {
    fn from(value: crate::core::run::RunContractError) -> Self {
        Self::Run(value)
    }
}
impl From<crate::build::FactoryBuildError> for RoutineContinuationError {
    fn from(value: crate::build::FactoryBuildError) -> Self {
        Self::Build(value)
    }
}
impl From<crate::journey::JourneyError> for RoutineContinuationError {
    fn from(value: crate::journey::JourneyError) -> Self {
        Self::Journey(value)
    }
}
