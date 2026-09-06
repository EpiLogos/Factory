//! Factory intake of composed agent capability through stable refs.
//!
//! A Factory Run/Commission never accepts a copied agent-configuration blob.
//! Composed capability is admitted only as two typed stable refs plus recorded
//! evidence envelopes from their owners:
//!
//! * the **authored source ref** — a Central AgentProfile
//!   (`central.agent-profile/<scope>/<profile-ref>@<revision-or-latest>`);
//! * the **effective composition ref** — an AIKit SkillSet projected at an
//!   exact generation (`aikit.skillset/<set-name>@<generation-id>`).
//!
//! Both grammars are derived from the pinned owner surfaces (Central
//! `agent-profile.list`/`agent-profile.read` inputs; AIKit `set show <NAME>`
//! identity and the `gen_…` `GenerationId` constructor), not invented here.
//! The evidence envelopes are recorded from those pinned revisions by the
//! conformance lane; the intake re-checks ref/evidence identity at admit time.
//!
//! Failure honesty (the K2 retirement-aware law): unresolved, retired and
//! otherwise withheld SkillSet members surface as explicit intake facts with
//! the resolver's own reason, never silently dropped.

use crate::core::run::RunRef;
use crate::journey_praxis::CENTRAL_AGENT_PROFILE_SCHEMA;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

pub const AGENT_CAPABILITY_INTAKE_SCHEMA: &str = "factory.agent-capability-intake/v1";
pub const AGENT_CAPABILITY_INTAKE_FIXTURE_SCHEMA: &str =
    "factory.agent-capability-intake-fixture/v1";
pub const AUTHORED_SOURCE_REF_PREFIX: &str = "central.agent-profile";
pub const EFFECTIVE_COMPOSITION_REF_PREFIX: &str = "aikit.skillset";
/// Authored-source revision wildcard. When a ref says `latest`, the revision the
/// authored evidence actually resolved to is retained on the intake record.
pub const AUTHORED_REVISION_LATEST: &str = "latest";

/// Authored residence scope of a Central AgentProfile. This is Central's source
/// relation (the store selector of `agent-profile.list`/`read`), not a runtime
/// AIKit Profile and not a new Agent identity namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProfileScope {
    Personal,
    Project,
}

impl AgentProfileScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Project => "project",
        }
    }

    fn parse(value: &str) -> Result<Self, AgentCapabilityIntakeError> {
        match value {
            "personal" => Ok(Self::Personal),
            "project" => Ok(Self::Project),
            other => Err(AgentCapabilityIntakeError::RefGrammar(format!(
                "AgentProfile scope must be personal or project, got {other}"
            ))),
        }
    }
}

/// Authored source ref: `central.agent-profile/<scope>/<profile-ref>@<revision>`.
///
/// `<scope>` is the Central store selector; `<profile-ref>` is the exact Central
/// AgentProfile store key (e.g. `agent-profile:researcher`); `<revision>` is the
/// exact Central revision string, or `latest` to bind to whatever revision the
/// authored evidence resolved to at intake. Profile refs and revisions cannot
/// contain `@`, and profile refs cannot contain `/`, so the grammar round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredAgentProfileRef {
    pub scope: AgentProfileScope,
    pub profile_ref: String,
    pub revision: String,
}

impl AuthoredAgentProfileRef {
    pub fn new(
        scope: AgentProfileScope,
        profile_ref: impl Into<String>,
        revision: impl Into<String>,
    ) -> Result<Self, AgentCapabilityIntakeError> {
        let reference = Self {
            scope,
            profile_ref: profile_ref.into(),
            revision: revision.into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn is_latest(&self) -> bool {
        self.revision == AUTHORED_REVISION_LATEST
    }

    fn validate(&self) -> Result<(), AgentCapabilityIntakeError> {
        if self.profile_ref.trim().is_empty() {
            return Err(AgentCapabilityIntakeError::RefGrammar(
                "authored source profile ref cannot be empty".into(),
            ));
        }
        if self.profile_ref.contains('@') || self.profile_ref.contains('/') {
            return Err(AgentCapabilityIntakeError::RefGrammar(format!(
                "authored source profile ref {profile_ref} cannot contain '@' or '/'",
                profile_ref = self.profile_ref
            )));
        }
        if self.revision.trim().is_empty() || self.revision.contains('@') {
            return Err(AgentCapabilityIntakeError::RefGrammar(format!(
                "authored source revision {revision} cannot be empty or contain '@'",
                revision = self.revision
            )));
        }
        Ok(())
    }
}

impl Display for AuthoredAgentProfileRef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{AUTHORED_SOURCE_REF_PREFIX}/{scope}/{profile_ref}@{revision}",
            scope = self.scope.as_str(),
            profile_ref = self.profile_ref,
            revision = self.revision
        )
    }
}

impl FromStr for AuthoredAgentProfileRef {
    type Err = AgentCapabilityIntakeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let rest = value
            .strip_prefix(AUTHORED_SOURCE_REF_PREFIX)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(|| {
                AgentCapabilityIntakeError::RefGrammar(format!(
                    "authored source ref must start with {AUTHORED_SOURCE_REF_PREFIX}/, got {value}"
                ))
            })?;
        let (scope, tail) = rest.split_once('/').ok_or_else(|| {
            AgentCapabilityIntakeError::RefGrammar(format!(
                "authored source ref {value} must carry <scope>/<profile-ref>@<revision>"
            ))
        })?;
        let (profile_ref, revision) = tail.rsplit_once('@').ok_or_else(|| {
            AgentCapabilityIntakeError::RefGrammar(format!(
                "authored source ref {value} must bind its revision with '@'"
            ))
        })?;
        Self::new(AgentProfileScope::parse(scope)?, profile_ref, revision)
    }
}

impl Serialize for AuthoredAgentProfileRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AuthoredAgentProfileRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Effective composition ref: `aikit.skillset/<set-name>@<generation-id>`.
///
/// `<set-name>` is the exact `aikit set show <NAME>` set identity;
/// `<generation-id>` is the exact AIKit content-addressed `GenerationId`
/// (`gen_…`, minted by `GenerationId::from_hash`). Set names cannot contain
/// `@` or `/` so the grammar round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSkillSetRef {
    pub set_name: String,
    pub generation_id: String,
}

impl EffectiveSkillSetRef {
    pub fn new(
        set_name: impl Into<String>,
        generation_id: impl Into<String>,
    ) -> Result<Self, AgentCapabilityIntakeError> {
        let reference = Self {
            set_name: set_name.into(),
            generation_id: generation_id.into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    fn validate(&self) -> Result<(), AgentCapabilityIntakeError> {
        if self.set_name.trim().is_empty()
            || self.set_name.contains('@')
            || self.set_name.contains('/')
        {
            return Err(AgentCapabilityIntakeError::RefGrammar(format!(
                "effective composition set name {set_name} cannot be empty or contain '@' or '/'",
                set_name = self.set_name
            )));
        }
        if !self.generation_id.starts_with("gen_") || self.generation_id.len() <= 4 {
            return Err(AgentCapabilityIntakeError::RefGrammar(format!(
                "effective composition generation id {generation_id} must be an AIKit GenerationId (gen_…)",
                generation_id = self.generation_id
            )));
        }
        Ok(())
    }
}

impl Display for EffectiveSkillSetRef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{EFFECTIVE_COMPOSITION_REF_PREFIX}/{set_name}@{generation_id}",
            set_name = self.set_name,
            generation_id = self.generation_id
        )
    }
}

impl FromStr for EffectiveSkillSetRef {
    type Err = AgentCapabilityIntakeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let rest = value
            .strip_prefix(EFFECTIVE_COMPOSITION_REF_PREFIX)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(|| {
                AgentCapabilityIntakeError::RefGrammar(format!(
                    "effective composition ref must start with {EFFECTIVE_COMPOSITION_REF_PREFIX}/, got {value}"
                ))
            })?;
        let (set_name, generation_id) = rest.rsplit_once('@').ok_or_else(|| {
            AgentCapabilityIntakeError::RefGrammar(format!(
                "effective composition ref {value} must bind its generation with '@'"
            ))
        })?;
        Self::new(set_name, generation_id)
    }
}

impl Serialize for EffectiveSkillSetRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for EffectiveSkillSetRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// The intake contract: composed agent capability as typed stable refs only.
/// There is no config-blob field, and none may be added; the Central profile
/// body and the AIKit resolution body stay with their owners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityRef {
    pub schema: String,
    /// Semantic Agent identity as carried by the Central AgentProfile source.
    pub agent_ref: String,
    pub authored_source: AuthoredAgentProfileRef,
    pub effective_composition: EffectiveSkillSetRef,
}

impl AgentCapabilityRef {
    pub fn new(
        agent_ref: impl Into<String>,
        authored_source: AuthoredAgentProfileRef,
        effective_composition: EffectiveSkillSetRef,
    ) -> Result<Self, AgentCapabilityIntakeError> {
        let agent_ref = agent_ref.into();
        if agent_ref.trim().is_empty() {
            return Err(AgentCapabilityIntakeError::InvalidText(
                "agent ref cannot be empty".into(),
            ));
        }
        Ok(Self {
            schema: AGENT_CAPABILITY_INTAKE_SCHEMA.into(),
            agent_ref,
            authored_source,
            effective_composition,
        })
    }
}

/// Standing of one SkillSet member, mirroring the pinned AIKit resolver's own
/// withheld-reason vocabulary (`aikit_core::skillset::WithheldReason` /
/// `UnavailableReason`, kebab-case serde tags) plus the projected side of
/// `SetProjection`. Factory never forms a second availability opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSetMemberStanding {
    Projected,
    NotSelected,
    NotInCatalog,
    RetiredStanding,
    TrustRequired,
    Quarantined,
    DeniedByPolicy,
    PlatformUnsupported,
    NoSupportedTarget,
    Blocked,
    DependencyUnavailable,
}

impl SkillSetMemberStanding {
    /// The exact AIKit withheld-reason tag this standing mirrors.
    pub fn aikit_reason_tag(self) -> Option<&'static str> {
        match self {
            Self::Projected => None,
            Self::NotSelected => Some("not-selected"),
            Self::NotInCatalog => Some("not-in-catalog"),
            Self::RetiredStanding => Some("retired-standing"),
            Self::TrustRequired => Some("trust-required"),
            Self::Quarantined => Some("quarantined"),
            Self::DeniedByPolicy => Some("denied-by-policy"),
            Self::PlatformUnsupported => Some("platform-unsupported"),
            Self::NoSupportedTarget => Some("no-supported-target"),
            Self::Blocked => Some("blocked"),
            Self::DependencyUnavailable => Some("dependency-unavailable"),
        }
    }

    pub fn is_withheld(self) -> bool {
        self != Self::Projected
    }

    /// Retired on its Control ground (K2): never projected, never hidden.
    pub fn is_retired(self) -> bool {
        self == Self::RetiredStanding
    }

    /// Unresolved: the member is not present in any registry.
    pub fn is_unresolved(self) -> bool {
        self == Self::NotInCatalog
    }

    fn from_withheld_reason(value: &Value) -> Result<Self, AgentCapabilityIntakeError> {
        let kind = value
            .get("withheld")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AgentCapabilityIntakeError::InvalidEvidence(
                    "withheld member must carry the resolver's own withheld kind".into(),
                )
            })?;
        match kind {
            "not-selected" => return Ok(Self::NotSelected),
            "unavailable" => {}
            other => {
                return Err(AgentCapabilityIntakeError::InvalidEvidence(format!(
                    "unknown withheld kind {other}"
                )))
            }
        }
        let reason = value.get("reason").and_then(Value::as_str).ok_or_else(|| {
            AgentCapabilityIntakeError::InvalidEvidence(
                "unavailable member must carry the resolver's own reason tag".into(),
            )
        })?;
        match reason {
            "not-in-catalog" => Ok(Self::NotInCatalog),
            "retired-standing" => Ok(Self::RetiredStanding),
            "trust-required" => Ok(Self::TrustRequired),
            "quarantined" => Ok(Self::Quarantined),
            "denied-by-policy" => Ok(Self::DeniedByPolicy),
            "platform-unsupported" => Ok(Self::PlatformUnsupported),
            "no-supported-target" => Ok(Self::NoSupportedTarget),
            "blocked" => Ok(Self::Blocked),
            "dependency-unavailable" => Ok(Self::DependencyUnavailable),
            other => Err(AgentCapabilityIntakeError::InvalidEvidence(format!(
                "unknown unavailable reason tag {other}"
            ))),
        }
    }
}

/// One SkillSet member as an intake fact. The reason is the pinned resolver's
/// own sentence, retained verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSetMemberFact {
    pub capability_ref: String,
    pub standing: SkillSetMemberStanding,
    /// The exact AIKit withheld-reason tag when the member did not project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withheld_kind: Option<String>,
    /// The resolver's own reason sentence when the member did not project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SkillSetMemberFact {
    pub fn is_withheld(&self) -> bool {
        self.standing.is_withheld()
    }

    pub fn is_retired(&self) -> bool {
        self.standing.is_retired()
    }

    pub fn is_unresolved(&self) -> bool {
        self.standing.is_unresolved()
    }
}

/// Authored evidence recorded from the pinned Central revision's AgentProfile
/// reading (`agent-profile.read` over the real AgentProfileStore). Factory
/// parses this shape; it never copies the profile body into its own records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredProfileEvidence {
    pub schema: String,
    pub scope: AgentProfileScope,
    pub profile_ref: String,
    pub revision: String,
    pub agent_ref: String,
    pub world_ref: String,
    #[serde(default)]
    pub skill_set_refs: Vec<String>,
}

impl AuthoredProfileEvidence {
    /// Parse the `profile` body of a recorded authored envelope.
    pub fn from_profile_body(profile: &Value) -> Result<Self, AgentCapabilityIntakeError> {
        let text = |field: &str| {
            profile
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| {
                    AgentCapabilityIntakeError::InvalidEvidence(format!(
                        "authored profile body is missing {field}"
                    ))
                })
        };
        let scope = AgentProfileScope::parse(&text("scope")?)?;
        Ok(Self {
            schema: text("schema")?,
            scope,
            profile_ref: text("ref")?,
            revision: text("revision")?,
            agent_ref: text("agent_ref")?,
            world_ref: text("world_ref")?,
            skill_set_refs: profile
                .get("skill_set_refs")
                .map(|refs| {
                    refs.as_array()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
        })
    }
}

/// One withheld member as recorded by the pinned AIKit revision: the
/// capability, the resolver's own structured standing and its own reason
/// sentence, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithheldMemberEvidence {
    pub capability_ref: String,
    pub standing: SkillSetMemberStanding,
    pub reason: String,
}

/// Effective evidence recorded from the pinned AIKit revision: the `set show`
/// reply (projected and withheld members with the resolver's own reasons)
/// bound to one exact generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveCompositionEvidence {
    pub set_name: String,
    pub generation_id: String,
    #[serde(default)]
    pub projected: Vec<String>,
    #[serde(default)]
    pub withheld: Vec<WithheldMemberEvidence>,
}

impl EffectiveCompositionEvidence {
    /// Parse a recorded effective envelope carrying the `set show` data shape.
    pub fn from_envelope(envelope: &Value) -> Result<Self, AgentCapabilityIntakeError> {
        let set_name = envelope
            .get("set")
            .and_then(|set| set.get("name"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| {
                AgentCapabilityIntakeError::InvalidEvidence(
                    "effective envelope is missing set.name".into(),
                )
            })?;
        let generation_id = envelope
            .get("generation")
            .and_then(|generation| generation.get("generation_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| {
                AgentCapabilityIntakeError::InvalidEvidence(
                    "effective envelope is missing generation.generation_id".into(),
                )
            })?;
        let projected = envelope
            .get("projected")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let withheld = match envelope.get("withheld").and_then(Value::as_array) {
            None => Vec::new(),
            Some(values) => values
                .iter()
                .map(|member| {
                    let withheld_reason = member.get("withheld_reason").ok_or_else(|| {
                        AgentCapabilityIntakeError::InvalidEvidence(
                            "withheld member must carry the resolver's structured withheld_reason"
                                .into(),
                        )
                    })?;
                    Ok(WithheldMemberEvidence {
                        capability_ref: member
                            .get("capability")
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                            .map(ToString::to_string)
                            .ok_or_else(|| {
                                AgentCapabilityIntakeError::InvalidEvidence(
                                    "withheld member is missing capability".into(),
                                )
                            })?,
                        standing: SkillSetMemberStanding::from_withheld_reason(withheld_reason)?,
                        reason: member
                            .get("reason")
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                            .map(ToString::to_string)
                            .ok_or_else(|| {
                                AgentCapabilityIntakeError::InvalidEvidence(
                                    "withheld member is missing the resolver's reason".into(),
                                )
                            })?,
                    })
                })
                .collect::<Result<Vec<_>, AgentCapabilityIntakeError>>()?,
        };
        Ok(Self {
            set_name,
            generation_id,
            projected,
            withheld,
        })
    }
}

/// The admitted intake record for one Run: typed stable refs plus every member
/// fact the effective composition replied about. Retired, unresolved and
/// otherwise withheld members are intake facts; none are ever dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityIntake {
    pub schema: String,
    pub run_ref: RunRef,
    pub capability: AgentCapabilityRef,
    /// The exact authored revision the evidence resolved to at intake (equals
    /// the ref revision unless the ref said `latest`).
    pub resolved_authored_revision: String,
    pub members: Vec<SkillSetMemberFact>,
}

impl AgentCapabilityIntake {
    /// Admit composed capability for one Run from stable refs plus recorded
    /// evidence. Fails loudly when the evidence does not answer to the refs:
    /// the authored evidence must name the same profile/revision/Agent, and the
    /// effective evidence must be the same set at the same generation.
    pub fn admit(
        run_ref: RunRef,
        capability: AgentCapabilityRef,
        authored: &AuthoredProfileEvidence,
        effective: &EffectiveCompositionEvidence,
    ) -> Result<Self, AgentCapabilityIntakeError> {
        if capability.schema != AGENT_CAPABILITY_INTAKE_SCHEMA {
            return Err(AgentCapabilityIntakeError::Schema(capability.schema));
        }
        if authored.schema != CENTRAL_AGENT_PROFILE_SCHEMA {
            return Err(AgentCapabilityIntakeError::AuthoredContract(
                authored.schema.clone(),
            ));
        }
        let authored_source = &capability.authored_source;
        if authored.scope != authored_source.scope
            || authored.profile_ref != authored_source.profile_ref
        {
            return Err(AgentCapabilityIntakeError::AuthoredEvidenceMismatch {
                expected: authored_source.to_string(),
                actual: format!(
                    "{AUTHORED_SOURCE_REF_PREFIX}/{scope}/{profile_ref}@{revision}",
                    scope = authored.scope.as_str(),
                    profile_ref = authored.profile_ref,
                    revision = authored.revision
                ),
            });
        }
        if !authored_source.is_latest() && authored.revision != authored_source.revision {
            return Err(AgentCapabilityIntakeError::AuthoredRevisionMismatch {
                expected: authored_source.revision.clone(),
                actual: authored.revision.clone(),
            });
        }
        if authored.agent_ref != capability.agent_ref {
            return Err(AgentCapabilityIntakeError::AgentMismatch {
                capability: capability.agent_ref.clone(),
                authored: authored.agent_ref.clone(),
            });
        }
        let composition = &capability.effective_composition;
        if effective.set_name != composition.set_name
            || effective.generation_id != composition.generation_id
        {
            return Err(AgentCapabilityIntakeError::EffectiveEvidenceMismatch {
                expected: composition.to_string(),
                actual: format!(
                    "{EFFECTIVE_COMPOSITION_REF_PREFIX}/{set_name}@{generation_id}",
                    set_name = effective.set_name,
                    generation_id = effective.generation_id
                ),
            });
        }

        let mut members: Vec<SkillSetMemberFact> = effective
            .projected
            .iter()
            .map(|capability_ref| SkillSetMemberFact {
                capability_ref: capability_ref.clone(),
                standing: SkillSetMemberStanding::Projected,
                withheld_kind: None,
                reason: None,
            })
            .collect();
        for member in &effective.withheld {
            members.push(SkillSetMemberFact {
                capability_ref: member.capability_ref.clone(),
                standing: member.standing,
                withheld_kind: member.standing.aikit_reason_tag().map(ToString::to_string),
                reason: Some(member.reason.clone()),
            });
        }
        if members.is_empty() {
            return Err(AgentCapabilityIntakeError::EmptyComposition(format!(
                "effective composition {composition} replied with no members"
            )));
        }

        Ok(Self {
            schema: AGENT_CAPABILITY_INTAKE_SCHEMA.into(),
            run_ref,
            capability,
            resolved_authored_revision: authored.revision.clone(),
            members,
        })
    }

    /// Retired members as intake facts (K2: never silently dropped).
    pub fn retired_members(&self) -> Vec<&SkillSetMemberFact> {
        self.members
            .iter()
            .filter(|fact| fact.is_retired())
            .collect()
    }

    /// Unresolved members (not present in any registry) as intake facts.
    pub fn unresolved_members(&self) -> Vec<&SkillSetMemberFact> {
        self.members
            .iter()
            .filter(|fact| fact.is_unresolved())
            .collect()
    }

    /// Every member that did not project, with the resolver's own reason.
    pub fn withheld_members(&self) -> Vec<&SkillSetMemberFact> {
        self.members
            .iter()
            .filter(|fact| fact.is_withheld())
            .collect()
    }

    pub fn projected_members(&self) -> Vec<&SkillSetMemberFact> {
        self.members
            .iter()
            .filter(|fact| !fact.is_withheld())
            .collect()
    }
}

/// Renderer-neutral reading of an admitted intake. Same facts, no authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityIntakeReading {
    pub schema: String,
    pub run_ref: RunRef,
    pub authored_source: AuthoredAgentProfileRef,
    pub effective_composition: EffectiveSkillSetRef,
    pub agent_ref: String,
    pub resolved_authored_revision: String,
    pub members: Vec<SkillSetMemberFact>,
    pub retired_capability_refs: Vec<String>,
    pub unresolved_capability_refs: Vec<String>,
}

impl From<&AgentCapabilityIntake> for AgentCapabilityIntakeReading {
    fn from(intake: &AgentCapabilityIntake) -> Self {
        Self {
            schema: AGENT_CAPABILITY_INTAKE_SCHEMA.into(),
            run_ref: intake.run_ref.clone(),
            authored_source: intake.capability.authored_source.clone(),
            effective_composition: intake.capability.effective_composition.clone(),
            agent_ref: intake.capability.agent_ref.clone(),
            resolved_authored_revision: intake.resolved_authored_revision.clone(),
            retired_capability_refs: intake
                .retired_members()
                .iter()
                .map(|fact| fact.capability_ref.clone())
                .collect(),
            unresolved_capability_refs: intake
                .unresolved_members()
                .iter()
                .map(|fact| fact.capability_ref.clone())
                .collect(),
            members: intake.members.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCapabilityIntakeError {
    InvalidText(String),
    RefGrammar(String),
    Schema(String),
    AuthoredContract(String),
    InvalidEvidence(String),
    AuthoredEvidenceMismatch {
        expected: String,
        actual: String,
    },
    AuthoredRevisionMismatch {
        expected: String,
        actual: String,
    },
    AgentMismatch {
        capability: String,
        authored: String,
    },
    EffectiveEvidenceMismatch {
        expected: String,
        actual: String,
    },
    EmptyComposition(String),
}

impl Display for AgentCapabilityIntakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::RefGrammar(message) => write!(formatter, "invalid capability ref: {message}"),
            Self::Schema(schema) => {
                write!(formatter, "unsupported agent capability schema {schema}")
            }
            Self::AuthoredContract(schema) => write!(
                formatter,
                "authored evidence must be {CENTRAL_AGENT_PROFILE_SCHEMA}, got {schema}"
            ),
            Self::InvalidEvidence(message) => write!(formatter, "invalid evidence envelope: {message}"),
            Self::AuthoredEvidenceMismatch { expected, actual } => write!(
                formatter,
                "authored evidence {actual} does not answer to authored source ref {expected}"
            ),
            Self::AuthoredRevisionMismatch { expected, actual } => write!(
                formatter,
                "authored evidence revision {actual} does not answer to ref revision {expected}"
            ),
            Self::AgentMismatch { capability, authored } => write!(
                formatter,
                "capability names Agent {capability} but authored evidence names {authored}"
            ),
            Self::EffectiveEvidenceMismatch { expected, actual } => write!(
                formatter,
                "effective evidence {actual} does not answer to effective composition ref {expected}"
            ),
            Self::EmptyComposition(reference) => write!(
                formatter,
                "effective composition {reference} replied with no members"
            ),
        }
    }
}

impl Error for AgentCapabilityIntakeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn authored_ref() -> AuthoredAgentProfileRef {
        "central.agent-profile/personal/agent-profile:factory-researcher@profile-rev-1"
            .parse()
            .unwrap()
    }

    fn effective_ref() -> EffectiveSkillSetRef {
        "aikit.skillset/factory-intake@gen_8b8c6ee18ce734a7"
            .parse()
            .unwrap()
    }

    fn authored_evidence() -> AuthoredProfileEvidence {
        AuthoredProfileEvidence::from_profile_body(&json!({
            "schema": CENTRAL_AGENT_PROFILE_SCHEMA,
            "scope": "personal",
            "ref": "agent-profile:factory-researcher",
            "revision": "profile-rev-1",
            "agent_ref": "agent:factory-researcher",
            "world_ref": "world:personal",
            "skill_set_refs": ["skill-set:factory-intake"]
        }))
        .unwrap()
    }

    fn effective_evidence() -> EffectiveCompositionEvidence {
        EffectiveCompositionEvidence::from_envelope(&json!({
            "set": {"name": "factory-intake", "provenance": "composed"},
            "generation": {"generation_id": "gen_8b8c6ee18ce734a7"},
            "projected": ["skill/factory/return-review"],
            "withheld": [
                {
                    "capability": "skill/factory/legacy-orientation",
                    "withheld_reason": {"withheld": "unavailable", "reason": "retired-standing", "retirement_reason": "Superseded by the composed AgentProfile orientation source; retired by owner."},
                    "reason": "skill/factory/legacy-orientation — withheld-retired on its Control ground: Superseded by the composed AgentProfile orientation source; retired by owner."
                },
                {
                    "capability": "skill/factory/unresolved-member",
                    "withheld_reason": {"withheld": "unavailable", "reason": "not-in-catalog"},
                    "reason": "skill/factory/unresolved-member — not present in any registry"
                },
                {
                    "capability": "skill/factory/unreviewed-return",
                    "withheld_reason": {"withheld": "unavailable", "reason": "trust-required"},
                    "reason": "skill/factory/unreviewed-return — this revision has not been reviewed"
                }
            ]
        }))
        .unwrap()
    }

    fn run_ref() -> RunRef {
        "run:01ARZ3NDEKTSV4RRFFQ69G5FB0".parse().unwrap()
    }

    #[test]
    fn authored_source_ref_grammar_round_trips() {
        let reference = authored_ref();
        assert_eq!(reference.scope, AgentProfileScope::Personal);
        assert_eq!(reference.profile_ref, "agent-profile:factory-researcher");
        assert_eq!(reference.revision, "profile-rev-1");
        let text = reference.to_string();
        assert_eq!(text.parse::<AuthoredAgentProfileRef>().unwrap(), reference);
        assert!("central.agent-profile/project/agent-profile:x@latest"
            .parse::<AuthoredAgentProfileRef>()
            .unwrap()
            .is_latest());
    }

    #[test]
    fn authored_source_ref_rejects_forged_grammar() {
        assert!("aikit.profile/personal/a@r"
            .parse::<AuthoredAgentProfileRef>()
            .is_err());
        assert!("central.agent-profile/planet/a@r"
            .parse::<AuthoredAgentProfileRef>()
            .is_err());
        assert!("central.agent-profile/personal/no-revision"
            .parse::<AuthoredAgentProfileRef>()
            .is_err());
        assert!("central.agent-profile/personal/a@b@c"
            .parse::<AuthoredAgentProfileRef>()
            .is_err());
    }

    #[test]
    fn effective_composition_ref_grammar_round_trips() {
        let reference = effective_ref();
        assert_eq!(reference.set_name, "factory-intake");
        assert_eq!(reference.generation_id, "gen_8b8c6ee18ce734a7");
        assert_eq!(
            reference
                .to_string()
                .parse::<EffectiveSkillSetRef>()
                .unwrap(),
            reference
        );
    }

    #[test]
    fn effective_composition_ref_requires_aikit_generation_identity() {
        assert!("aikit.skillset/s@generation-1"
            .parse::<EffectiveSkillSetRef>()
            .is_err());
        assert!("aikit.skillset/s@gen_"
            .parse::<EffectiveSkillSetRef>()
            .is_err());
        assert!("central.skillset/s@gen_abcdef0123456789"
            .parse::<EffectiveSkillSetRef>()
            .is_err());
    }

    #[test]
    fn agent_capability_ref_serializes_refs_by_grammar_not_by_blob() {
        let capability =
            AgentCapabilityRef::new("agent:factory-researcher", authored_ref(), effective_ref())
                .unwrap();
        let value = serde_json::to_value(&capability).unwrap();
        assert_eq!(
            value["authored_source"],
            json!("central.agent-profile/personal/agent-profile:factory-researcher@profile-rev-1")
        );
        assert_eq!(
            value["effective_composition"],
            json!("aikit.skillset/factory-intake@gen_8b8c6ee18ce734a7")
        );
        assert!(value.get("config").is_none() && value.get("profile").is_none());
        let round: AgentCapabilityRef = serde_json::from_value(value).unwrap();
        assert_eq!(round, capability);
    }

    #[test]
    fn admit_binds_refs_to_evidence_and_keeps_every_member_as_a_fact() {
        let intake = AgentCapabilityIntake::admit(
            run_ref(),
            AgentCapabilityRef::new("agent:factory-researcher", authored_ref(), effective_ref())
                .unwrap(),
            &authored_evidence(),
            &effective_evidence(),
        )
        .unwrap();

        assert_eq!(intake.members.len(), 4);
        assert_eq!(intake.projected_members().len(), 1);
        assert_eq!(intake.withheld_members().len(), 3);
        assert_eq!(intake.retired_members().len(), 1);
        assert_eq!(intake.unresolved_members().len(), 1);
        let retired = intake.retired_members()[0];
        assert_eq!(retired.capability_ref, "skill/factory/legacy-orientation");
        assert_eq!(retired.withheld_kind.as_deref(), Some("retired-standing"));
        assert!(retired.reason.as_deref().unwrap().contains(
            "Superseded by the composed AgentProfile orientation source; retired by owner."
        ));
        assert_eq!(
            intake.unresolved_members()[0].capability_ref,
            "skill/factory/unresolved-member"
        );
        assert_eq!(intake.resolved_authored_revision, "profile-rev-1");
    }

    #[test]
    fn admit_fails_loudly_when_evidence_does_not_answer_to_the_refs() {
        let capability =
            AgentCapabilityRef::new("agent:factory-researcher", authored_ref(), effective_ref())
                .unwrap();

        let mut wrong_profile = authored_evidence();
        wrong_profile.profile_ref = "agent-profile:someone-else".into();
        assert!(matches!(
            AgentCapabilityIntake::admit(
                run_ref(),
                capability.clone(),
                &wrong_profile,
                &effective_evidence()
            ),
            Err(AgentCapabilityIntakeError::AuthoredEvidenceMismatch { .. })
        ));

        let mut wrong_revision = authored_evidence();
        wrong_revision.revision = "profile-rev-9".into();
        assert!(matches!(
            AgentCapabilityIntake::admit(
                run_ref(),
                capability.clone(),
                &wrong_revision,
                &effective_evidence()
            ),
            Err(AgentCapabilityIntakeError::AuthoredRevisionMismatch { .. })
        ));

        let mut wrong_agent = authored_evidence();
        wrong_agent.agent_ref = "agent:someone-else".into();
        assert!(matches!(
            AgentCapabilityIntake::admit(
                run_ref(),
                capability.clone(),
                &wrong_agent,
                &effective_evidence()
            ),
            Err(AgentCapabilityIntakeError::AgentMismatch { .. })
        ));

        let mut wrong_generation = effective_evidence();
        wrong_generation.generation_id = "gen_0000000000000000".into();
        assert!(matches!(
            AgentCapabilityIntake::admit(
                run_ref(),
                capability,
                &authored_evidence(),
                &wrong_generation
            ),
            Err(AgentCapabilityIntakeError::EffectiveEvidenceMismatch { .. })
        ));
    }

    #[test]
    fn latest_revision_binds_to_whatever_the_authored_evidence_resolved_to() {
        let latest: AuthoredAgentProfileRef =
            "central.agent-profile/personal/agent-profile:factory-researcher@latest"
                .parse()
                .unwrap();
        let intake = AgentCapabilityIntake::admit(
            run_ref(),
            AgentCapabilityRef::new("agent:factory-researcher", latest, effective_ref()).unwrap(),
            &authored_evidence(),
            &effective_evidence(),
        )
        .unwrap();
        assert_eq!(intake.resolved_authored_revision, "profile-rev-1");
    }

    #[test]
    fn reading_surfaces_retirement_and_unresolved_lists_for_renderers() {
        let intake = AgentCapabilityIntake::admit(
            run_ref(),
            AgentCapabilityRef::new("agent:factory-researcher", authored_ref(), effective_ref())
                .unwrap(),
            &authored_evidence(),
            &effective_evidence(),
        )
        .unwrap();
        let reading = AgentCapabilityIntakeReading::from(&intake);
        assert_eq!(
            reading.retired_capability_refs,
            vec!["skill/factory/legacy-orientation".to_string()]
        );
        assert_eq!(
            reading.unresolved_capability_refs,
            vec!["skill/factory/unresolved-member".to_string()]
        );
        assert_eq!(reading.members.len(), 4);
    }

    #[test]
    fn evidence_rejects_envelopes_missing_owner_identity() {
        assert!(AuthoredProfileEvidence::from_profile_body(&json!({
            "schema": "central.agent-profile/v2",
            "scope": "personal",
            "ref": "agent-profile:x",
            "revision": "r1",
            "agent_ref": "agent:x",
            "world_ref": "world:personal"
        }))
        .is_ok());
        assert!(AuthoredProfileEvidence::from_profile_body(&json!({
            "schema": CENTRAL_AGENT_PROFILE_SCHEMA,
            "scope": "galactic",
            "ref": "agent-profile:x",
            "revision": "r1",
            "agent_ref": "agent:x",
            "world_ref": "world:personal"
        }))
        .is_err());
        assert!(EffectiveCompositionEvidence::from_envelope(&json!({
            "set": {"name": "s"},
            "projected": []
        }))
        .is_err());
        assert!(EffectiveCompositionEvidence::from_envelope(&json!({
            "set": {"name": "s"},
            "generation": {"generation_id": "gen_8b8c6ee18ce734a7"},
            "withheld": [{"capability": "skill/x", "reason": "no structured tag"}]
        }))
        .is_err());
    }

    #[test]
    fn standing_vocabulary_is_the_resolvers_own() {
        assert_eq!(
            SkillSetMemberStanding::RetiredStanding.aikit_reason_tag(),
            Some("retired-standing")
        );
        assert!(SkillSetMemberStanding::RetiredStanding.is_withheld());
        assert!(SkillSetMemberStanding::NotInCatalog.is_unresolved());
        assert!(!SkillSetMemberStanding::Projected.is_withheld());
        assert_eq!(
            serde_json::to_value(SkillSetMemberStanding::RetiredStanding).unwrap(),
            json!("retired-standing")
        );
    }

    #[test]
    fn structured_withheld_reasons_classify_through_the_pinned_vocabulary() {
        let cases = [
            (
                json!({"withheld": "not-selected"}),
                SkillSetMemberStanding::NotSelected,
            ),
            (
                json!({"withheld": "unavailable", "reason": "not-in-catalog"}),
                SkillSetMemberStanding::NotInCatalog,
            ),
            (
                json!({"withheld": "unavailable", "reason": "retired-standing", "retirement_reason": "r"}),
                SkillSetMemberStanding::RetiredStanding,
            ),
            (
                json!({"withheld": "unavailable", "reason": "trust-required"}),
                SkillSetMemberStanding::TrustRequired,
            ),
            (
                json!({"withheld": "unavailable", "reason": "dependency-unavailable"}),
                SkillSetMemberStanding::DependencyUnavailable,
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(
                SkillSetMemberStanding::from_withheld_reason(&value).unwrap(),
                expected
            );
        }
        assert!(
            SkillSetMemberStanding::from_withheld_reason(&json!({"withheld": "mystery"})).is_err()
        );
    }
}
