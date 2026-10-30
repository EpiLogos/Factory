//! Durable work custody: the obligation-bearing relation between a stable
//! World Position and developmental work (World inhabitation contract v1 §3).
//!
//! A Position is an address Central defines; Factory never mints one and never
//! derives one from an Agent, AgentSession or body. Custody records live in the
//! project's existing developmental state document and are written only through
//! its advisory lock and atomic publisher (`transact_developmental_state`). There
//! is no second store, queue or registry.
//!
//! State law (after OpenRig's durable queue): `in-progress` and `blocked` are
//! open; `released`, `completed` and `handed-off` are closed. A closed record
//! reopens only with an explicit `--reopen`, and only back into an open state.
//! A hand-off names its receiving Position and creates the successor custody in
//! the same locked write, so an obligation is never dropped between two holders.
//! Every change appends a transition and bumps the record's own revision, which
//! callers may pin with `--expected-revision`.
//!
//! Closure law (OpenRig `packages/daemon/src/domain/hot-potato-enforcer.ts:18-103`,
//! `validateClosure`, and the admits-map at `queue-repository.ts:2428-2453`): a
//! custody that closes carries a closure reason, one of OpenRig's own words —
//! `handed_off_to`, `blocked_on`, `denied`, `canceled`, `no-follow-on`,
//! `escalation`, `superseded`. `completed` and `released` refuse a close without
//! one, by name (`factory.custody.missing_closure_reason`); `handed_off_to`,
//! `blocked_on` and `escalation` also need a target, as does `superseded` (the
//! successor that replaced the work). A hand-off closes as `handed_off_to` with
//! the receiving Position as its target, as OpenRig's transactional hand-off
//! does (`queue-repository.ts:1558`). An open state stores no closure
//! (`closure_fields_not_admitted`). The closure is kept on the record and on the
//! transition that closed it, and a reopen clears it from the record. Records
//! closed before this law carry none and stay valid.
//!
//! A custody also carries how much its holder decides alone (DLW §19.4,
//! "Software Factory owns how much it decides alone: autonomy, the review gate
//! and what it holds"): its autonomy (suggest | review | auto), its review gate
//! (diffs | approvals | both) and whom it escalates to. Each is set when the
//! custody is assigned, kept verbatim, and absent until set — Factory never
//! supplies the manual's starting value on a record nobody set. They are not
//! changed by `update`: that command admits only the holding occupant, and the
//! holder deciding how much it may decide alone is not this law's to grant. A
//! hand-off does not carry them to the receiver, whose holding is its own.

use crate::core::run::RunRef;
use crate::core::run::WorkflowUnitRef;
use crate::developmental_read::FactoryDevelopmentalState;
use crate::journey::JourneyRef;
use crate::project_development_store::{
    transact_developmental_state, ProjectDevelopmentStoreError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FACTORY_WORK_CUSTODY: &str = "factory.work-custody/v1";
pub const FACTORY_WORK_CUSTODY_RECEIPT: &str = "factory.work-custody-receipt/v1";
pub const FACTORY_WORK_CUSTODY_LISTING: &str = "factory.work-custody-listing/v1";
pub const FACTORY_REFUSAL: &str = "factory.refusal/v1";

pub const ASSIGN_USAGE: &str = "factory development custody assign [<state>] --position <central:position:…> --work <ref> --reason <text> [--run <run-ref> [--workflow-unit <ref>]] [--journey <ref>] [--origin-communique <ref>] [--state in-progress|blocked] [--custody <factory:custody:…>] [--autonomy suggest|review|auto] [--review-gate diffs|approvals|both] [--escalate-to <person-ref>] [--json]";
pub const UPDATE_USAGE: &str = "factory development custody update [<state>] --custody <factory:custody:…> --state <in-progress|blocked|released|completed|handed-off> --reason <text> [--closure-reason handed_off_to|blocked_on|denied|canceled|no-follow-on|escalation|superseded [--closure-target <ref>]] [--reopen] [--to-position <central:position:…>] [--expected-revision N] [--json]";

const CUSTODY_PREFIX: &str = "factory:custody:";

/// How much the holder decides alone (DLW §19.4 Custody, Autonomy: "Suggest:
/// the agent proposes only. Review: it works and you check each result. Auto:
/// it works and lands changes with receipts.").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CustodyAutonomy {
    Suggest,
    Review,
    Auto,
}

impl CustodyAutonomy {
    pub const ALL: [CustodyAutonomy; 3] = [Self::Suggest, Self::Review, Self::Auto];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Suggest => "suggest",
            Self::Review => "review",
            Self::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == value)
    }
}

/// What waits for the person before a change lands (DLW §19.4 Custody, Review
/// gate: Diffs · Approvals · Both).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewGate {
    Diffs,
    Approvals,
    Both,
}

impl ReviewGate {
    pub const ALL: [ReviewGate; 3] = [Self::Diffs, Self::Approvals, Self::Both];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diffs => "diffs",
            Self::Approvals => "approvals",
            Self::Both => "both",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|gate| gate.as_str() == value)
    }
}
const POSITION_PREFIX: &str = "central:position:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CustodyState {
    InProgress,
    Blocked,
    Released,
    Completed,
    HandedOff,
}

impl CustodyState {
    pub const ALL: [CustodyState; 5] = [
        Self::InProgress,
        Self::Blocked,
        Self::Released,
        Self::Completed,
        Self::HandedOff,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in-progress",
            Self::Blocked => "blocked",
            Self::Released => "released",
            Self::Completed => "completed",
            Self::HandedOff => "handed-off",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }

    /// Closed custody no longer carries the obligation.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Released | Self::Completed | Self::HandedOff)
    }
}

impl Display for CustodyState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why a custody closed: OpenRig's closure reasons, spelled exactly as OpenRig
/// spells them (`hot-potato-enforcer.ts:18-28` `CLOSURE_REASONS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClosureReason {
    #[serde(rename = "handed_off_to")]
    HandedOffTo,
    #[serde(rename = "blocked_on")]
    BlockedOn,
    #[serde(rename = "denied")]
    Denied,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "no-follow-on")]
    NoFollowOn,
    #[serde(rename = "escalation")]
    Escalation,
    #[serde(rename = "superseded")]
    Superseded,
}

impl ClosureReason {
    pub const ALL: [ClosureReason; 7] = [
        Self::HandedOffTo,
        Self::BlockedOn,
        Self::Denied,
        Self::Canceled,
        Self::NoFollowOn,
        Self::Escalation,
        Self::Superseded,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HandedOffTo => "handed_off_to",
            Self::BlockedOn => "blocked_on",
            Self::Denied => "denied",
            Self::Canceled => "canceled",
            Self::NoFollowOn => "no-follow-on",
            Self::Escalation => "escalation",
            Self::Superseded => "superseded",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == value)
    }

    /// The valid words, as OpenRig's refusal lists them.
    pub fn words() -> String {
        Self::ALL
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `handed_off_to`, `blocked_on` and `escalation` name where the work went
    /// (`hot-potato-enforcer.ts:86-96`); `superseded` names what replaced it
    /// (`queue-repository.ts:2446-2453`).
    pub fn requires_target(self) -> bool {
        matches!(
            self,
            Self::HandedOffTo | Self::BlockedOn | Self::Escalation | Self::Superseded
        )
    }
}

impl Display for ClosureReason {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The closure a closed custody carries: its reason and, where the reason
/// needs one, its target (OpenRig `closure_reason` / `closure_target`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyClosure {
    pub reason: ClosureReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyOrigin {
    pub communique_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyHandoff {
    pub custody_ref: String,
    pub position_ref: String,
}

/// One appended state change. `from` is absent only for the assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyTransition {
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<CustodyState>,
    pub to: CustodyState,
    pub reason: String,
    pub at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reopen: bool,
    /// The occupant that made this change, when a body occupying a World
    /// Position made it (`OI_POSITION_REF` / `OI_OCCUPANT_GENERATION`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<CustodyActor>,
    /// The closure this change closed the custody with; only on a change into
    /// a closed state, and absent on changes made before the closure law.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure: Option<CustodyClosure>,
}

/// A body acting on custody from inside a World Position occupancy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyActor {
    pub position_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_ref: Option<String>,
}

impl CustodyActor {
    /// The occupancy stamped into this process by `aikit inhabit`, if any.
    pub fn from_env() -> Option<Self> {
        let position_ref = std::env::var("OI_POSITION_REF")
            .ok()
            .filter(|value| !value.trim().is_empty())?;
        Some(Self {
            position_ref,
            generation_ref: std::env::var("OI_OCCUPANT_GENERATION")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }
}

/// `factory.work-custody/v1`. Field names follow the cross-owner contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryWorkCustody {
    pub schema: String,
    pub custody_ref: String,
    pub position_ref: String,
    pub work_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<RunRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journey_ref: Option<JourneyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_unit_ref: Option<WorkflowUnitRef>,
    pub state: CustodyState,
    pub assigned_at_unix_ms: i64,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<CustodyOrigin>,
    pub revision: u64,
    pub updated_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handed_off_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handed_off_to: Option<CustodyHandoff>,
    pub transitions: Vec<CustodyTransition>,
    /// How much the holder decides alone; absent until an assignment sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<CustodyAutonomy>,
    /// What waits for the person before a change lands; absent until set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_gate: Option<ReviewGate>,
    /// Whom the holder asks when it needs a decision (DLW §19.4 Escalate to:
    /// "A person"), as the assigner names them; absent until set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalate_to: Option<String>,
    /// Why the custody closed, while it stands closed (the closure law above);
    /// cleared by a reopen, absent on custody closed before the law.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure: Option<CustodyClosure>,
}

impl FactoryWorkCustody {
    /// The settings the assignment gave, compared as one.
    fn settings(&self) -> (Option<CustodyAutonomy>, Option<ReviewGate>, Option<&str>) {
        (self.autonomy, self.review_gate, self.escalate_to.as_deref())
    }

    /// The same obligation, regardless of when or why it was assigned.
    fn same_work_as(&self, other: &Self) -> bool {
        self.position_ref == other.position_ref
            && self.work_ref == other.work_ref
            && self.run_ref == other.run_ref
            && self.journey_ref == other.journey_ref
            && self.workflow_unit_ref == other.workflow_unit_ref
    }
}

/// Three-part refusal (fact, consequence, action) with a stable code. Every
/// semantic refusal is raised before any write. Actual publication uncertainty
/// is carried separately and must not claim the source stayed unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Refusal(Box<RefusalData>);

/// The owned refusal fields. Boxing this body keeps native cause provenance
/// without enlarging every custody Result on the stack; serialization and
/// ordinary field access remain the existing refusal contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalData {
    pub schema: String,
    pub code: String,
    pub fact: String,
    pub consequence: String,
    pub action: String,
    #[serde(
        default,
        rename = "publicationUncertainty",
        skip_serializing_if = "Option::is_none"
    )]
    pub publication_uncertainty: Option<crate::NativePublicationDetails>,
    #[serde(skip)]
    publication_cause: Option<std::sync::Arc<crate::NativePublicationUncertainty>>,
}

impl std::ops::Deref for Refusal {
    type Target = RefusalData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Refusal {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for Refusal {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.code == other.code
            && self.fact == other.fact
            && self.consequence == other.consequence
            && self.action == other.action
            && self.publication_uncertainty == other.publication_uncertainty
    }
}
impl Eq for Refusal {}

pub const NOTHING_PERSISTED: &str = "Nothing was persisted; the developmental state is unchanged.";
pub const NOTHING_READ: &str = "No reading was produced; the developmental state is unchanged.";

impl Refusal {
    pub fn new(
        code: &str,
        fact: impl Into<String>,
        consequence: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self(Box::new(RefusalData {
            schema: FACTORY_REFUSAL.into(),
            code: code.into(),
            fact: fact.into(),
            consequence: consequence.into(),
            action: action.into(),
            publication_uncertainty: None,
            publication_cause: None,
        }))
    }

    pub(crate) fn unchanged(
        code: &str,
        fact: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self::new(code, fact, NOTHING_PERSISTED, action)
    }
}

impl Display for Refusal {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}\n  consequence: {}\n  action: {}\n  code: {}",
            self.fact, self.consequence, self.action, self.code
        )
    }
}

impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.publication_cause
            .as_ref()
            .map(|cause| cause.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssignRequest {
    pub position_ref: String,
    pub work_ref: String,
    pub reason: String,
    pub run_ref: Option<RunRef>,
    pub journey_ref: Option<JourneyRef>,
    pub workflow_unit_ref: Option<WorkflowUnitRef>,
    pub origin_communique_ref: Option<String>,
    pub initial_state: Option<CustodyState>,
    /// Caller-chosen identity makes a retried assignment replay-safe.
    pub custody_ref: Option<String>,
    pub autonomy: Option<CustodyAutonomy>,
    pub review_gate: Option<ReviewGate>,
    pub escalate_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateRequest {
    pub custody_ref: String,
    pub state: Option<CustodyState>,
    pub reason: String,
    pub reopen: bool,
    pub to_position_ref: Option<String>,
    pub expected_revision: Option<u64>,
    /// The occupant acting, when the caller is a body occupying a Position.
    /// The published `update` command requires this, and Actuation must still
    /// call the generation current. In-process Factory transitions pass
    /// `None`; a missing stamp is not operator authority on the command.
    pub actor: Option<CustodyActor>,
    /// Why the custody closes (`--closure-reason`); required by `completed`
    /// and `released`, implied by `handed-off`, refused by an open state.
    pub closure_reason: Option<ClosureReason>,
    /// Where the work went or what replaced it (`--closure-target`).
    pub closure_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyReceipt {
    pub schema: String,
    /// `applied`, `already-applied` (replayed assignment) or `unchanged`.
    pub result: String,
    pub custody: FactoryWorkCustody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor: Option<FactoryWorkCustody>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyListing {
    pub schema: String,
    pub project_ref: String,
    pub filter: CustodyListingFilter,
    pub count: usize,
    pub custody: Vec<FactoryWorkCustody>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CustodyListingFilter {
    pub position_ref: Option<String>,
    pub run_ref: Option<String>,
    pub state: Option<CustodyState>,
}

pub fn validate_position_ref(value: &str) -> Result<(), String> {
    let rest = value.strip_prefix(POSITION_PREFIX).unwrap_or("");
    if rest.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(format!(
            "`{value}` is not a World Position ref; Positions are addressed as central:position:<world>:<slug>"
        ));
    }
    Ok(())
}

fn validate_custody_ref(value: &str) -> Result<(), String> {
    let uuid = value.strip_prefix(CUSTODY_PREFIX).unwrap_or("");
    let bytes = uuid.as_bytes();
    let shaped = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
            }
        });
    if !shaped {
        return Err(format!(
            "`{value}` is not a custody ref; custody is addressed as factory:custody:<uuid>"
        ));
    }
    Ok(())
}

/// A time-ordered UUID (version 7) built from a fresh ULID's millisecond
/// timestamp and randomness, so no further dependency is needed.
fn new_custody_ref() -> String {
    let mut bits = ulid::Ulid::new().0;
    bits = (bits & !(0xF_u128 << 76)) | (0x7_u128 << 76);
    bits = (bits & !(0x3_u128 << 62)) | (0x2_u128 << 62);
    let hex = format!("{bits:032x}");
    format!(
        "{CUSTODY_PREFIX}{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Structural invariants checked whenever the developmental state is read or
/// written. Referential checks (Run, Journey, WorkflowUnit) happen when custody
/// is assigned and again whenever current work is derived, where a failure is
/// reported rather than hidden.
/// Whom a holder escalates to is named by the assigner (DLW §19.4: "A person");
/// Factory keeps the ref verbatim and only refuses an empty or multi-line one.
pub fn validate_escalate_to(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.trim() != value || value.contains(['\n', '\r']) {
        return Err(format!(
            "--escalate-to {value:?} is not a person's ref: give one non-empty ref, without surrounding spaces or line breaks"
        ));
    }
    Ok(())
}

pub fn validate_custody_collection(records: &[FactoryWorkCustody]) -> Result<(), String> {
    let mut identities = BTreeSet::new();
    for record in records {
        let label = &record.custody_ref;
        if record.schema != FACTORY_WORK_CUSTODY {
            return Err(format!("custody {label} has schema {}", record.schema));
        }
        validate_custody_ref(label)?;
        if !identities.insert(label.as_str()) {
            return Err(format!("custody {label} is recorded more than once"));
        }
        validate_position_ref(&record.position_ref)?;
        if record.work_ref.trim().is_empty() || record.reason.trim().is_empty() {
            return Err(format!("custody {label} has an empty work ref or reason"));
        }
        if record.workflow_unit_ref.is_some() && record.run_ref.is_none() {
            return Err(format!(
                "custody {label} names a WorkflowUnit without its Run"
            ));
        }
        if record
            .origin
            .as_ref()
            .is_some_and(|origin| origin.communique_ref.trim().is_empty())
        {
            return Err(format!("custody {label} has an empty origin communique"));
        }
        let last = record.transitions.last();
        if record.revision == 0
            || record.transitions.len() as u64 != record.revision
            || last.map(|transition| transition.to) != Some(record.state)
            || record
                .transitions
                .first()
                .and_then(|first| first.from)
                .is_some()
            || record
                .transitions
                .iter()
                .enumerate()
                .any(|(index, transition)| transition.revision != index as u64 + 1)
        {
            return Err(format!(
                "custody {label} transition history does not match its state and revision"
            ));
        }
        if let Some(person) = &record.escalate_to {
            validate_escalate_to(person).map_err(|detail| format!("custody {label}: {detail}"))?;
        }
        if (record.state == CustodyState::HandedOff) != record.handed_off_to.is_some() {
            return Err(format!(
                "custody {label} hand-off target does not match its state"
            ));
        }
        if let Some(closure) = &record.closure {
            validate_closure_record(label, record.state, closure)?;
            if record.state == CustodyState::HandedOff
                && (closure.reason != ClosureReason::HandedOffTo
                    || closure.target.as_deref()
                        != record
                            .handed_off_to
                            .as_ref()
                            .map(|handoff| handoff.position_ref.as_str()))
            {
                return Err(format!(
                    "custody {label} is handed off but its closure does not name handed_off_to and the receiving Position"
                ));
            }
        }
        for transition in &record.transitions {
            if let Some(closure) = &transition.closure {
                validate_closure_record(label, transition.to, closure)?;
            }
        }
    }
    Ok(())
}

fn validate_closure_record(
    label: &str,
    state: CustodyState,
    closure: &CustodyClosure,
) -> Result<(), String> {
    if !state.is_terminal() {
        return Err(format!(
            "custody {label} carries a closure on the open state '{state}'"
        ));
    }
    if closure.reason.requires_target()
        && closure
            .target
            .as_deref()
            .is_none_or(|target| target.trim().is_empty())
    {
        return Err(format!(
            "custody {label} closed as {} without its closure target",
            closure.reason
        ));
    }
    Ok(())
}

/// The closure a change into `target` carries, or the refusal by name
/// (OpenRig `validateClosure`, `hot-potato-enforcer.ts:59-103`, and the
/// admits-map, `queue-repository.ts:2428-2453`). Raised before any write.
fn closure_for(
    current: &FactoryWorkCustody,
    target: CustodyState,
    request: &UpdateRequest,
) -> Result<Option<CustodyClosure>, Refusal> {
    let reference = &current.custody_ref;
    let given_target = request
        .closure_target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if !target.is_terminal() {
        if request.closure_reason.is_some() || request.closure_target.is_some() {
            return Err(Refusal::unchanged(
                "factory.custody.closure_fields_not_admitted",
                format!(
                    "custody {reference} would move to the open state '{target}'; a closure reason and target persist only on a close (completed, released or handed-off)"
                ),
                format!(
                    "close it instead: factory development custody update <state> --custody {reference} --state completed --closure-reason <reason> --reason <text>; or drop --closure-reason and --closure-target"
                ),
            ));
        }
        return Ok(None);
    }
    if target == CustodyState::HandedOff {
        if request
            .closure_reason
            .is_some_and(|reason| reason != ClosureReason::HandedOffTo)
            || request.closure_target.is_some()
        {
            return Err(Refusal::unchanged(
                "factory.custody.closure_fields_not_admitted",
                format!(
                    "custody {reference} would be handed off; a hand-off closes as handed_off_to, and its target is the receiving Position (--to-position)"
                ),
                format!(
                    "drop --closure-target and any other --closure-reason: factory development custody update <state> --custody {reference} --state handed-off --to-position <central:position:…> --reason <text>"
                ),
            ));
        }
        return Ok(Some(CustodyClosure {
            reason: ClosureReason::HandedOffTo,
            target: request.to_position_ref.clone(),
        }));
    }
    let Some(reason) = request.closure_reason else {
        return Err(Refusal::unchanged(
            "factory.custody.missing_closure_reason",
            format!(
                "custody {reference} cannot close as '{target}' without a closure reason; valid values: {}",
                ClosureReason::words()
            ),
            format!(
                "name why it closes: factory development custody update <state> --custody {reference} --state {target} --closure-reason <reason> [--closure-target <ref>] --reason <text>"
            ),
        ));
    };
    if reason.requires_target() && given_target.is_none() {
        return Err(Refusal::unchanged(
            "factory.custody.missing_closure_target",
            format!("closure_reason={reason} requires closure_target"),
            format!(
                "name where the work went (or what replaced it): factory development custody update <state> --custody {reference} --state {target} --closure-reason {reason} --closure-target <ref> --reason <text>"
            ),
        ));
    }
    Ok(Some(CustodyClosure {
        reason,
        target: given_target,
    }))
}

/// Assignment references must resolve in this state now. Derivation checks
/// them again later, so drift is refused there instead of guessed around.
pub(crate) fn resolve_work_address(
    state: &FactoryDevelopmentalState,
    run_ref: Option<&RunRef>,
    journey_ref: Option<&JourneyRef>,
    workflow_unit_ref: Option<&WorkflowUnitRef>,
) -> Result<(), (String, String)> {
    if workflow_unit_ref.is_some() && run_ref.is_none() {
        return Err((
            "factory.custody.workflow_unit_requires_run".into(),
            "a WorkflowUnit is a work node only within a Run, and no --run was given".into(),
        ));
    }
    if let Some(run_ref) = run_ref {
        let run = state.build.run(run_ref).ok_or_else(|| {
            (
                "factory.custody.unknown_run".into(),
                format!("Run {run_ref} is not in this Factory project's developmental state"),
            )
        })?;
        if let Some(unit) = workflow_unit_ref {
            let present = run
                .map()
                .nodes()
                .values()
                .any(|node| node.semantic_ref.as_ref() == Some(unit.as_ref()));
            if !present {
                return Err((
                    "factory.custody.unknown_workflow_unit".into(),
                    format!("WorkflowUnit {unit} is not a node of Run {run_ref}'s RunMap"),
                ));
            }
        }
    }
    if let Some(journey_ref) = journey_ref {
        let journey = state
            .journeys
            .iter()
            .find(|journey| &journey.journey_ref == journey_ref)
            .ok_or_else(|| {
                (
                    "factory.custody.unknown_journey".into(),
                    format!("Journey {journey_ref} is not in this Factory project's developmental state"),
                )
            })?;
        if let Some(run_ref) = run_ref {
            if !journey.runs.iter().any(|link| &link.run_ref == run_ref) {
                return Err((
                    "factory.custody.run_not_in_journey".into(),
                    format!("Run {run_ref} is not linked to Journey {journey_ref}"),
                ));
            }
        }
    }
    Ok(())
}

/// Validate and apply one assignment to an in-memory state.
pub fn assign_in(
    state: &mut FactoryDevelopmentalState,
    request: AssignRequest,
    now_unix_ms: i64,
) -> Result<CustodyReceipt, Refusal> {
    let usage = ASSIGN_USAGE;
    if let Err(detail) = validate_position_ref(&request.position_ref) {
        return Err(Refusal::unchanged(
            "factory.custody.invalid_position",
            detail,
            usage,
        ));
    }
    if request.work_ref.trim().is_empty() || request.reason.trim().is_empty() {
        return Err(Refusal::unchanged(
            "factory.custody.usage",
            "custody assignment needs a non-empty --work and --reason",
            usage,
        ));
    }
    if request
        .origin_communique_ref
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(Refusal::unchanged(
            "factory.custody.usage",
            "--origin-communique was given without a ref",
            usage,
        ));
    }
    let initial = request.initial_state.unwrap_or(CustodyState::InProgress);
    if initial.is_terminal() {
        return Err(Refusal::unchanged(
            "factory.custody.invalid_transition",
            format!("custody cannot be assigned directly into the closed state {initial}"),
            "assign with --state in-progress (the default) or --state blocked",
        ));
    }
    if let Err((code, fact)) = resolve_work_address(
        state,
        request.run_ref.as_ref(),
        request.journey_ref.as_ref(),
        request.workflow_unit_ref.as_ref(),
    ) {
        return Err(Refusal::unchanged(
            &code,
            fact,
            "read the Run and its units with `factory development run <state> <run-ref>` and `factory development workflow-units <state> <run-ref>`, then assign with refs that exist",
        ));
    }
    if let Some(reference) = &request.custody_ref {
        if let Err(detail) = validate_custody_ref(reference) {
            return Err(Refusal::unchanged("factory.custody.usage", detail, usage));
        }
    }
    if let Some(person) = &request.escalate_to {
        if let Err(detail) = validate_escalate_to(person) {
            return Err(Refusal::unchanged("factory.custody.usage", detail, usage));
        }
    }
    let custody_ref = request.custody_ref.clone().unwrap_or_else(new_custody_ref);
    let record = FactoryWorkCustody {
        schema: FACTORY_WORK_CUSTODY.into(),
        custody_ref: custody_ref.clone(),
        position_ref: request.position_ref,
        work_ref: request.work_ref,
        run_ref: request.run_ref,
        journey_ref: request.journey_ref,
        workflow_unit_ref: request.workflow_unit_ref,
        state: initial,
        assigned_at_unix_ms: now_unix_ms,
        reason: request.reason.clone(),
        origin: request
            .origin_communique_ref
            .map(|communique_ref| CustodyOrigin { communique_ref }),
        revision: 1,
        updated_at_unix_ms: now_unix_ms,
        handed_off_from: None,
        handed_off_to: None,
        transitions: vec![CustodyTransition {
            revision: 1,
            from: None,
            to: initial,
            reason: request.reason,
            at_unix_ms: now_unix_ms,
            reopen: false,
            actor: None,
            closure: None,
        }],
        autonomy: request.autonomy,
        review_gate: request.review_gate,
        escalate_to: request.escalate_to,
        closure: None,
    };
    if let Some(existing) = state
        .work_custody
        .iter()
        .find(|existing| existing.custody_ref == custody_ref)
    {
        // Replay of the same assignment returns the retained record; any other
        // content under that identity is a conflict, never a silent overwrite.
        if existing.same_work_as(&record)
            && existing.origin == record.origin
            && existing.settings() == record.settings()
            && existing.transitions.first().map(|first| &first.reason) == Some(&record.reason)
        {
            return Ok(CustodyReceipt {
                schema: FACTORY_WORK_CUSTODY_RECEIPT.into(),
                result: "already-applied".into(),
                custody: existing.clone(),
                successor: None,
            });
        }
        return Err(Refusal::unchanged(
            "factory.custody.identity_conflict",
            format!("custody {custody_ref} already exists with different work, Position, origin or settings"),
            format!("read it with `factory development custody list <state> --position {}` or assign without --custody to mint a new ref", existing.position_ref),
        ));
    }
    if let Some(open) = state
        .work_custody
        .iter()
        .find(|existing| !existing.state.is_terminal() && existing.same_work_as(&record))
    {
        return Err(Refusal::unchanged(
            "factory.custody.duplicate",
            format!(
                "Position {} already holds {} custody {} for this work",
                open.position_ref, open.state, open.custody_ref
            ),
            format!(
                "change that record instead: factory development custody update <state> --custody {} --state <state> --reason <text>",
                open.custody_ref
            ),
        ));
    }
    state.work_custody.push(record.clone());
    Ok(CustodyReceipt {
        schema: FACTORY_WORK_CUSTODY_RECEIPT.into(),
        result: "applied".into(),
        custody: record,
        successor: None,
    })
}

/// Validate and apply one state change to an in-memory state.
pub fn update_in(
    state: &mut FactoryDevelopmentalState,
    request: UpdateRequest,
    now_unix_ms: i64,
) -> Result<CustodyReceipt, Refusal> {
    let usage = UPDATE_USAGE;
    let Some(target) = request.state else {
        return Err(Refusal::unchanged(
            "factory.custody.usage",
            "custody update needs --state",
            usage,
        ));
    };
    if request.reason.trim().is_empty() {
        return Err(Refusal::unchanged(
            "factory.custody.usage",
            "custody update needs a non-empty --reason",
            usage,
        ));
    }
    let index = state
        .work_custody
        .iter()
        .position(|record| record.custody_ref == request.custody_ref)
        .ok_or_else(|| {
            Refusal::unchanged(
                "factory.custody.not_found",
                format!(
                    "custody {} is not in this developmental state",
                    request.custody_ref
                ),
                "list the retained custody with `factory development custody list <state>`",
            )
        })?;
    let current = state.work_custody[index].clone();
    if let Some(actor) = &request.actor {
        if actor
            .generation_ref
            .as_deref()
            .map(str::trim)
            .filter(|generation| !generation.is_empty())
            .is_none()
        {
            return Err(Refusal::unchanged(
                "factory.custody.generation_required",
                format!(
                    "custody {} is held by {}; a position stamp without the current occupant generation is not authority",
                    current.custody_ref, current.position_ref
                ),
                "inhabit the holding Position so this process carries OI_POSITION_REF and OI_OCCUPANT_GENERATION, then retry",
            ));
        }
        if actor.position_ref != current.position_ref {
            return Err(Refusal::unchanged(
                "factory.custody.not_holder",
                format!(
                    "custody {} is held by {}; this body occupies {}, which holds no standing over it",
                    current.custody_ref, current.position_ref, actor.position_ref
                ),
                format!(
                    "tell the holder instead: aikit gateway send --to {} --body <what should change and why>",
                    current.position_ref
                ),
            ));
        }
    }
    if let Some(expected) = request.expected_revision {
        if expected != current.revision {
            return Err(Refusal::unchanged(
                "factory.custody.revision_conflict",
                format!(
                    "custody {} is at revision {}, not the expected revision {expected} (another writer changed it first)",
                    current.custody_ref, current.revision
                ),
                format!(
                    "re-read it with `factory development custody list <state> --position {}` and retry against revision {}",
                    current.position_ref, current.revision
                ),
            ));
        }
    }
    if request.to_position_ref.is_some() && target != CustodyState::HandedOff {
        return Err(Refusal::unchanged(
            "factory.custody.usage",
            "--to-position is only meaningful with --state handed-off",
            usage,
        ));
    }
    let closure = closure_for(&current, target, &request)?;
    // A retried request for the state the record is already in is a replay.
    // A hand-off replays only toward the same receiver.
    let same_receiver = target != CustodyState::HandedOff
        || request.to_position_ref.as_deref()
            == current
                .handed_off_to
                .as_ref()
                .map(|handoff| handoff.position_ref.as_str());
    if target == current.state && !request.reopen && same_receiver {
        if let (Some(recorded), Some(asked)) = (&current.closure, &closure) {
            if recorded != asked {
                return Err(Refusal::unchanged(
                    "factory.custody.invalid_transition",
                    format!(
                        "custody {} is already closed as '{}' with closure {}{}; a replay cannot change why it closed",
                        current.custody_ref,
                        current.state,
                        recorded.reason,
                        recorded
                            .target
                            .as_deref()
                            .map(|target| format!(" → {target}"))
                            .unwrap_or_default()
                    ),
                    format!(
                        "reopen it first if the closure was wrong: factory development custody update <state> --custody {} --state in-progress --reopen --reason <why>",
                        current.custody_ref
                    ),
                ));
            }
        }
        return Ok(CustodyReceipt {
            schema: FACTORY_WORK_CUSTODY_RECEIPT.into(),
            result: "unchanged".into(),
            custody: current,
            successor: None,
        });
    }
    if current.state.is_terminal() {
        if target.is_terminal() {
            return Err(Refusal::unchanged(
                "factory.custody.invalid_transition",
                format!(
                    "custody {} is closed as '{}'; it cannot move to the closed state '{target}' (a reopen returns it to in-progress or blocked)",
                    current.custody_ref, current.state
                ),
                format!(
                    "reopen into an open state first: factory development custody update <state> --custody {} --state in-progress --reopen --reason <why>",
                    current.custody_ref
                ),
            ));
        }
        if !request.reopen {
            return Err(Refusal::unchanged(
                "factory.custody.reopen_required",
                format!(
                    "custody {} is '{}'; state '{target}' would reopen a closed obligation",
                    current.custody_ref, current.state
                ),
                format!(
                    "re-run deliberately with --reopen: factory development custody update <state> --custody {} --state {target} --reopen --reason <why>",
                    current.custody_ref
                ),
            ));
        }
    } else if request.reopen {
        return Err(Refusal::unchanged(
            "factory.custody.reopen_not_closed",
            format!(
                "custody {} is '{}', which is open; --reopen applies only to released, completed or handed-off custody",
                current.custody_ref, current.state
            ),
            format!(
                "drop --reopen: factory development custody update <state> --custody {} --state {target} --reason <text>",
                current.custody_ref
            ),
        ));
    }
    let mut successor = None;
    if target == CustodyState::HandedOff {
        let Some(receiver) = request.to_position_ref.clone() else {
            return Err(Refusal::unchanged(
                "factory.custody.handoff_target_required",
                format!(
                    "custody {} cannot be handed off without a receiving Position",
                    current.custody_ref
                ),
                format!(
                    "name the receiver: factory development custody update <state> --custody {} --state handed-off --to-position <central:position:…> --reason <text>; to let the work go without a receiver use --state released",
                    current.custody_ref
                ),
            ));
        };
        if let Err(detail) = validate_position_ref(&receiver) {
            return Err(Refusal::unchanged(
                "factory.custody.invalid_position",
                detail,
                usage,
            ));
        }
        if receiver == current.position_ref {
            return Err(Refusal::unchanged(
                "factory.custody.invalid_transition",
                format!(
                    "custody {} is already held by {receiver}; a hand-off needs a different Position",
                    current.custody_ref
                ),
                "name another receiving Position with --to-position",
            ));
        }
        let successor_ref = new_custody_ref();
        successor = Some(FactoryWorkCustody {
            schema: FACTORY_WORK_CUSTODY.into(),
            custody_ref: successor_ref,
            position_ref: receiver,
            work_ref: current.work_ref.clone(),
            run_ref: current.run_ref.clone(),
            journey_ref: current.journey_ref.clone(),
            workflow_unit_ref: current.workflow_unit_ref.clone(),
            state: CustodyState::InProgress,
            assigned_at_unix_ms: now_unix_ms,
            reason: request.reason.clone(),
            origin: current.origin.clone(),
            revision: 1,
            updated_at_unix_ms: now_unix_ms,
            handed_off_from: Some(current.custody_ref.clone()),
            handed_off_to: None,
            transitions: vec![CustodyTransition {
                revision: 1,
                from: None,
                to: CustodyState::InProgress,
                reason: request.reason.clone(),
                at_unix_ms: now_unix_ms,
                reopen: false,
                actor: None,
                closure: None,
            }],
            // The receiver's holding is its own: how much it decides alone is
            // not carried over from the Position that handed it off.
            autonomy: None,
            review_gate: None,
            escalate_to: None,
            closure: None,
        });
    }
    let record = &mut state.work_custody[index];
    record.revision += 1;
    record.transitions.push(CustodyTransition {
        revision: record.revision,
        from: Some(record.state),
        to: target,
        reason: request.reason,
        at_unix_ms: now_unix_ms,
        reopen: request.reopen,
        actor: request.actor.clone(),
        closure: closure.clone(),
    });
    record.state = target;
    record.closure = closure;
    record.updated_at_unix_ms = now_unix_ms;
    record.handed_off_to = successor.as_ref().map(|next| CustodyHandoff {
        custody_ref: next.custody_ref.clone(),
        position_ref: next.position_ref.clone(),
    });
    let updated = record.clone();
    if let Some(next) = &successor {
        state.work_custody.push(next.clone());
    }
    Ok(CustodyReceipt {
        schema: FACTORY_WORK_CUSTODY_RECEIPT.into(),
        result: "applied".into(),
        custody: updated,
        successor,
    })
}

/// Every custody record matching the filter, uncapped, in stable ref order.
pub fn list_in(state: &FactoryDevelopmentalState, filter: CustodyListingFilter) -> CustodyListing {
    let mut custody = state
        .work_custody
        .iter()
        .filter(|record| {
            filter
                .position_ref
                .as_ref()
                .is_none_or(|position| &record.position_ref == position)
                && filter.run_ref.as_ref().is_none_or(|run| {
                    record.run_ref.as_ref().map(ToString::to_string).as_ref() == Some(run)
                })
                && filter.state.is_none_or(|wanted| record.state == wanted)
        })
        .cloned()
        .collect::<Vec<_>>();
    custody.sort_by(|left, right| left.custody_ref.cmp(&right.custody_ref));
    CustodyListing {
        schema: FACTORY_WORK_CUSTODY_LISTING.into(),
        project_ref: state.project_ref().to_string(),
        filter,
        count: custody.len(),
        custody,
    }
}

/// Run one custody mutation inside the developmental provider's lock. A
/// refusal leaves the state untouched, so nothing is published.
fn transact(
    path: &Path,
    operation: impl FnOnce(&mut FactoryDevelopmentalState) -> Result<CustodyReceipt, Refusal>,
) -> Result<CustodyReceipt, Refusal> {
    transact_developmental_state(path, |state| Ok(operation(state)))
        .map_err(|error| state_refusal(path, error))?
}

pub fn assign(path: &Path, request: AssignRequest) -> Result<CustodyReceipt, Refusal> {
    let now = now_unix_ms();
    transact(path, |state| assign_in(state, request, now))
}

/// The published custody command. A missing occupancy stamp is a refusal,
/// not an operator. The generation has to be the one Actuation still calls current.
pub fn update(path: &Path, request: UpdateRequest) -> Result<CustodyReceipt, Refusal> {
    if let Some(actor) = request.actor.as_ref() {
        let actor = published_actor(Some(actor))?;
        confirm_current_generation(actor)?;
    }
    let now = now_unix_ms();
    transact(path, |state| update_in(state, request, now))
}

fn published_actor(actor: Option<&CustodyActor>) -> Result<&CustodyActor, Refusal> {
    let Some(actor) = actor else {
        return Err(Refusal::unchanged(
            "factory.custody.actor_required",
            "a custody change with no occupancy stamp is not an operator",
            "inhabit the holding Position and retry from that body",
        ));
    };
    if actor
        .generation_ref
        .as_deref()
        .map(str::trim)
        .filter(|generation| !generation.is_empty())
        .is_none()
    {
        return Err(Refusal::unchanged(
            "factory.custody.generation_required",
            format!(
                "this body names {} but carries no occupant generation",
                actor.position_ref
            ),
            "inhabit so OI_OCCUPANT_GENERATION is set, then retry",
        ));
    }
    Ok(actor)
}

fn actuation_binary() -> PathBuf {
    for key in [
        "ACTUATION_BIN",
        "OI_ACTUATION_BIN",
        "FACTORY_NATIVE_ACTUATION_BIN",
    ] {
        if let Some(value) = std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            return PathBuf::from(value);
        }
    }
    PathBuf::from("actuation")
}

pub(crate) fn confirm_current_generation(actor: &CustodyActor) -> Result<(), Refusal> {
    let generation = actor
        .generation_ref
        .as_deref()
        .map(str::trim)
        .filter(|generation| !generation.is_empty())
        .expect("published_actor already required a generation");
    let binary = actuation_binary();
    let output = Command::new(&binary)
        .args([
            "occupancy",
            "verify",
            "--position",
            &actor.position_ref,
            "--generation",
            generation,
            "--json",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return Err(Refusal::unchanged(
                "factory.custody.occupancy_unverified",
                format!(
                    "Actuation could not be run at {} to verify {} @ {generation}: {error}",
                    binary.display(),
                    actor.position_ref
                ),
                "set ACTUATION_BIN to the actuation executable and retry from the holding body",
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let current = output.status.success()
        && serde_json::from_str::<serde_json::Value>(&stdout)
            .ok()
            .and_then(|value| value.get("ok").and_then(|ok| ok.as_bool()))
            == Some(true);
    if current {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        stdout.trim().to_string()
    };
    Err(Refusal::unchanged(
        "factory.custody.occupancy_unverified",
        format!(
            "{} @ {generation} is not Actuation's current occupant ({detail})",
            actor.position_ref
        ),
        format!(
            "read the tenure with `actuation occupancy read --position {}` and retry as that generation",
            actor.position_ref
        ),
    ))
}

pub(crate) fn state_refusal(path: &Path, error: ProjectDevelopmentStoreError) -> Refusal {
    let fact = format!(
        "the developmental state at {} could not be read or written: {error}",
        path.display()
    );
    if let ProjectDevelopmentStoreError::PublicationUncertain(cause) = error {
        let mut refusal = Refusal::new("factory.publication_uncertain", fact,
            "A candidate source was published; durability or native readback is unknown.",
            "inspect the original native source before another mutation; do not retry automatically");
        refusal.publication_uncertainty = Some(cause.details());
        refusal.publication_cause = Some(std::sync::Arc::new(cause));
        return refusal;
    }
    Refusal::new("factory.state.unavailable",fact,
        "The outcome was not applied; the state file was not replaced.",
        "check the path with `factory project locate <project-root>` and retry against the statePath it reports")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FactoryBuildState;
    use crate::core::run::{Project, ProjectRef};

    const P: &str = "central:position:project:O-I:factory-guardian";
    const Q: &str = "central:position:project:O-I:reviewer";

    fn state() -> FactoryDevelopmentalState {
        let project: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();
        FactoryDevelopmentalState::new(FactoryBuildState::empty(Project::new(project)), vec![])
            .unwrap()
    }

    fn request(work: &str) -> AssignRequest {
        AssignRequest {
            position_ref: P.into(),
            work_ref: work.into(),
            reason: "assigned".into(),
            ..AssignRequest::default()
        }
    }

    /// A lawful update: a close into `completed` or `released` names why.
    fn update(custody_ref: &str, to: CustodyState) -> UpdateRequest {
        UpdateRequest {
            custody_ref: custody_ref.into(),
            state: Some(to),
            reason: format!("to {to}"),
            closure_reason: matches!(to, CustodyState::Completed | CustodyState::Released)
                .then_some(ClosureReason::NoFollowOn),
            ..UpdateRequest::default()
        }
    }

    #[test]
    fn a_close_without_a_closure_reason_is_refused_by_name_and_writes_nothing() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let before = state.clone();
        for to in [CustodyState::Completed, CustodyState::Released] {
            let mut bare = update(&reference, to);
            bare.closure_reason = None;
            let refusal = update_in(&mut state, bare, 2).unwrap_err();
            assert_eq!(refusal.code, "factory.custody.missing_closure_reason");
            assert_eq!(refusal.consequence, NOTHING_PERSISTED);
            // The refusal names every word OpenRig admits.
            for word in ClosureReason::ALL {
                assert!(refusal.fact.contains(word.as_str()), "{}", refusal.fact);
            }
            assert!(refusal.action.contains("--closure-reason"));
        }
        assert_eq!(state, before);
    }

    #[test]
    fn a_reason_that_names_where_the_work_went_needs_its_target() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        for reason in [
            ClosureReason::HandedOffTo,
            ClosureReason::BlockedOn,
            ClosureReason::Escalation,
            ClosureReason::Superseded,
        ] {
            let mut closing = update(&reference, CustodyState::Completed);
            closing.closure_reason = Some(reason);
            closing.closure_target = Some("  ".into());
            assert_eq!(
                refusal_code(update_in(&mut state, closing, 2)),
                "factory.custody.missing_closure_target"
            );
        }
        let mut escalated = update(&reference, CustodyState::Completed);
        escalated.closure_reason = Some(ClosureReason::Escalation);
        escalated.closure_target = Some("central:person:frank".into());
        let closed = update_in(&mut state, escalated, 3).unwrap().custody;
        let closure = CustodyClosure {
            reason: ClosureReason::Escalation,
            target: Some("central:person:frank".into()),
        };
        assert_eq!(closed.closure.as_ref(), Some(&closure));
        assert_eq!(
            closed.transitions.last().unwrap().closure.as_ref(),
            Some(&closure)
        );
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn an_open_state_stores_no_closure_and_a_reopen_clears_it() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let mut parked = update(&reference, CustodyState::Blocked);
        parked.closure_reason = Some(ClosureReason::Denied);
        assert_eq!(
            refusal_code(update_in(&mut state, parked, 2)),
            "factory.custody.closure_fields_not_admitted"
        );
        let mut targeted = update(&reference, CustodyState::Blocked);
        targeted.closure_target = Some("work:b".into());
        assert_eq!(
            refusal_code(update_in(&mut state, targeted, 2)),
            "factory.custody.closure_fields_not_admitted"
        );
        let mut denied = update(&reference, CustodyState::Released);
        denied.closure_reason = Some(ClosureReason::Denied);
        let closed = update_in(&mut state, denied, 3).unwrap().custody;
        assert_eq!(closed.closure.unwrap().reason, ClosureReason::Denied);
        // A replay that changes why it closed is refused; the same closure replays.
        assert_eq!(
            refusal_code(update_in(
                &mut state,
                update(&reference, CustodyState::Released),
                4
            )),
            "factory.custody.invalid_transition"
        );
        let mut same = update(&reference, CustodyState::Released);
        same.closure_reason = Some(ClosureReason::Denied);
        assert_eq!(update_in(&mut state, same, 4).unwrap().result, "unchanged");
        let mut reopen = update(&reference, CustodyState::InProgress);
        reopen.reopen = true;
        let reopened = update_in(&mut state, reopen, 5).unwrap().custody;
        assert!(reopened.closure.is_none());
        // The history keeps the closure the close carried.
        assert_eq!(
            reopened.transitions[1].closure.as_ref().map(|c| c.reason),
            Some(ClosureReason::Denied)
        );
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn a_hand_off_closes_as_handed_off_to_its_receiver() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let mut wrong = update(&reference, CustodyState::HandedOff);
        wrong.to_position_ref = Some(Q.into());
        wrong.closure_reason = Some(ClosureReason::Canceled);
        assert_eq!(
            refusal_code(update_in(&mut state, wrong, 2)),
            "factory.custody.closure_fields_not_admitted"
        );
        let mut handoff = update(&reference, CustodyState::HandedOff);
        handoff.to_position_ref = Some(Q.into());
        let receipt = update_in(&mut state, handoff, 3).unwrap();
        assert_eq!(
            receipt.custody.closure,
            Some(CustodyClosure {
                reason: ClosureReason::HandedOffTo,
                target: Some(Q.into()),
            })
        );
        assert!(receipt.successor.unwrap().closure.is_none());
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn a_closure_on_an_open_record_or_without_its_target_is_refused_on_read() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let mut forged = state.work_custody.clone();
        forged[0].closure = Some(CustodyClosure {
            reason: ClosureReason::NoFollowOn,
            target: None,
        });
        assert!(validate_custody_collection(&forged)
            .unwrap_err()
            .contains("open state"));
        update_in(&mut state, update(&reference, CustodyState::Completed), 2).unwrap();
        let mut untargeted = state.work_custody.clone();
        untargeted[0].closure = Some(CustodyClosure {
            reason: ClosureReason::BlockedOn,
            target: None,
        });
        assert!(validate_custody_collection(&untargeted)
            .unwrap_err()
            .contains("closure target"));
        // A custody closed before the closure law carries none and stays valid.
        let mut historical = state.work_custody.clone();
        historical[0].closure = None;
        historical[0].transitions[1].closure = None;
        validate_custody_collection(&historical).unwrap();
    }

    fn refusal_code<T: std::fmt::Debug>(result: Result<T, Refusal>) -> String {
        let refusal = result.unwrap_err();
        assert_eq!(refusal.consequence, NOTHING_PERSISTED);
        assert!(!refusal.fact.is_empty() && !refusal.action.is_empty());
        refusal.code.clone()
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn actual_custody_public_dispatch_keeps_native_postpublication_cause() {
        use crate::developmental_read::FactoryDevelopmentalFileProvider;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("native-custody.json");
        let retained = root.path().join("retained-committed-custody.json");
        FactoryDevelopmentalFileProvider::create_new(&path, state()).unwrap();
        let physical = std::fs::canonicalize(&path).unwrap();
        let observed = retained.clone();
        crate::native_file_transaction::observe_next_publication(move |published| {
            std::fs::rename(published, observed).unwrap();
        });
        let args = vec![
            "development".into(),
            "custody".into(),
            "assign".into(),
            path.display().to_string(),
            "--position".into(),
            P.into(),
            "--work".into(),
            "work:real-dispatch-publication".into(),
            "--reason".into(),
            "Native public dispatch effect test".into(),
            "--json".into(),
        ];
        let error = crate::cli::execute_cli(&args, None).unwrap_err();
        let cause = crate::native_publication_uncertainty(&error).unwrap();
        assert_eq!(cause.source_path, physical);
        assert_eq!(cause.cause.raw_os_error(), Some(libc::ENOENT));
        let failure = error.native_publication_failure().unwrap();
        assert_eq!(failure["error"]["details"]["published"], true);
        assert_eq!(failure["error"]["details"]["automatic_retry"], false);
        assert_eq!(
            FactoryDevelopmentalFileProvider::open(&retained)
                .unwrap()
                .state()
                .work_custody
                .len(),
            1
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn actual_custody_cli_commit_never_returns_unchanged_after_readback_failure() {
        use crate::developmental_read::FactoryDevelopmentalFileProvider;
        use std::os::unix::fs::PermissionsExt;
        for move_after_publication in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("native-custody.json");
            let retained = root.path().join("retained-committed-custody.json");
            FactoryDevelopmentalFileProvider::create_new(&path, state()).unwrap();
            let physical = std::fs::canonicalize(&path).unwrap();
            let observer = retained.clone();
            crate::native_file_transaction::observe_next_publication(move |published| {
                if move_after_publication {
                    std::fs::rename(published, observer).unwrap();
                } else {
                    std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                }
            });
            let args = vec![
                "custody".into(),
                "assign".into(),
                path.display().to_string(),
                "--position".into(),
                P.into(),
                "--work".into(),
                "work:real-publication-proof".into(),
                "--reason".into(),
                "Native source effect test".into(),
            ];
            let refusal = crate::inhabitation_cli::execute(&args, true).unwrap_err();
            assert_eq!(refusal.code, "factory.publication_uncertain");
            assert_ne!(refusal.consequence, NOTHING_PERSISTED);
            assert!(refusal.consequence.contains("published"));
            let cause = crate::native_publication_uncertainty(&refusal).unwrap();
            let value = serde_json::to_value(&refusal).unwrap();
            assert_eq!(
                value["publicationUncertainty"]["source_path"],
                physical.display().to_string()
            );
            assert_eq!(value["publicationUncertainty"]["published"], true);
            assert_eq!(value["publicationUncertainty"]["outcome"], "unknown");
            assert_eq!(value["publicationUncertainty"]["automatic_retry"], false);
            if move_after_publication {
                assert_eq!(cause.cause.raw_os_error(), Some(libc::ENOENT));
            }
            let retained_path = if move_after_publication {
                &retained
            } else {
                &path
            };
            let current = FactoryDevelopmentalFileProvider::open(retained_path).unwrap();
            assert_eq!(current.state().work_custody.len(), 1);
            assert_eq!(
                current.state().work_custody[0].work_ref,
                "work:real-publication-proof"
            );
            // Cloning the existing structured result must keep the original native cause.
            assert_eq!(refusal.clone(), refusal);
            assert!(crate::native_publication_uncertainty(&refusal.clone()).is_some());
        }
    }

    #[test]
    fn minted_refs_are_time_ordered_uuids() {
        let reference = new_custody_ref();
        validate_custody_ref(&reference).unwrap();
        let uuid = reference.strip_prefix(CUSTODY_PREFIX).unwrap();
        assert_eq!(&uuid[14..15], "7", "version 7");
        assert!(
            matches!(&uuid[19..20], "8" | "9" | "a" | "b"),
            "RFC variant"
        );
    }

    #[test]
    fn assignment_refuses_malformed_or_unresolvable_work() {
        let mut state = state();
        let mut bad_position = request("work:a");
        bad_position.position_ref = "agent/factory-guardian".into();
        assert_eq!(
            refusal_code(assign_in(&mut state, bad_position, 1)),
            "factory.custody.invalid_position"
        );
        let mut unit_without_run = request("work:a");
        unit_without_run.workflow_unit_ref =
            Some("workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAB".parse().unwrap());
        assert_eq!(
            refusal_code(assign_in(&mut state, unit_without_run, 1)),
            "factory.custody.workflow_unit_requires_run"
        );
        let mut unknown_run = request("work:a");
        unknown_run.run_ref = Some("run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap());
        assert_eq!(
            refusal_code(assign_in(&mut state, unknown_run, 1)),
            "factory.custody.unknown_run"
        );
        let mut closed = request("work:a");
        closed.initial_state = Some(CustodyState::Completed);
        assert_eq!(
            refusal_code(assign_in(&mut state, closed, 1)),
            "factory.custody.invalid_transition"
        );
        assert!(state.work_custody.is_empty());
    }

    #[test]
    fn open_duplicates_are_refused_and_named_replays_are_idempotent() {
        let mut state = state();
        let mut named = request("work:a");
        named.custody_ref = Some("factory:custody:0190f5c2-0000-7000-8000-000000000001".into());
        let first = assign_in(&mut state, named.clone(), 1).unwrap();
        assert_eq!(first.result, "applied");
        let replay = assign_in(&mut state, named.clone(), 2).unwrap();
        assert_eq!(replay.result, "already-applied");
        assert_eq!(replay.custody, first.custody);
        let mut conflicting = named;
        conflicting.work_ref = "work:b".into();
        assert_eq!(
            refusal_code(assign_in(&mut state, conflicting, 3)),
            "factory.custody.identity_conflict"
        );
        assert_eq!(
            refusal_code(assign_in(&mut state, request("work:a"), 4)),
            "factory.custody.duplicate"
        );
        assert_eq!(state.work_custody.len(), 1);
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn closed_custody_reopens_only_explicitly_and_only_into_an_open_state() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        update_in(&mut state, update(&reference, CustodyState::Completed), 2).unwrap();
        let replay = update_in(&mut state, update(&reference, CustodyState::Completed), 3).unwrap();
        assert_eq!(replay.result, "unchanged");
        assert_eq!(replay.custody.revision, 2);
        assert_eq!(
            refusal_code(update_in(
                &mut state,
                update(&reference, CustodyState::Released),
                3
            )),
            "factory.custody.invalid_transition"
        );
        assert_eq!(
            refusal_code(update_in(
                &mut state,
                update(&reference, CustodyState::InProgress),
                3
            )),
            "factory.custody.reopen_required"
        );
        let mut reclose = update(&reference, CustodyState::Released);
        reclose.reopen = true;
        assert_eq!(
            refusal_code(update_in(&mut state, reclose, 3)),
            "factory.custody.invalid_transition"
        );
        let mut reopen = update(&reference, CustodyState::Blocked);
        reopen.reopen = true;
        let reopened = update_in(&mut state, reopen.clone(), 4).unwrap().custody;
        assert_eq!(reopened.state, CustodyState::Blocked);
        assert_eq!(reopened.revision, 3);
        assert!(reopened.transitions.last().unwrap().reopen);
        assert_eq!(
            refusal_code(update_in(&mut state, reopen, 5)),
            "factory.custody.reopen_not_closed"
        );
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn unchanged_state_and_stale_revisions_write_nothing() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let before = state.clone();
        let same = update_in(&mut state, update(&reference, CustodyState::InProgress), 2).unwrap();
        assert_eq!(same.result, "unchanged");
        assert_eq!(state, before);
        let mut stale = update(&reference, CustodyState::Blocked);
        stale.expected_revision = Some(7);
        assert_eq!(
            refusal_code(update_in(&mut state, stale, 3)),
            "factory.custody.revision_conflict"
        );
        let mut pinned = update(&reference, CustodyState::Blocked);
        pinned.expected_revision = Some(1);
        assert_eq!(
            update_in(&mut state, pinned, 4).unwrap().custody.revision,
            2
        );
    }

    #[test]
    fn only_the_holding_position_may_change_custody_and_its_change_is_attributed() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let holder = state.work_custody[0].position_ref.clone();
        let before = state.clone();
        let mut foreign = update(&reference, CustodyState::Completed);
        foreign.actor = Some(CustodyActor {
            position_ref: "central:position:project:O-I:aikit-guardian".into(),
            generation_ref: Some("actuation:generation:other".into()),
        });
        assert_eq!(
            refusal_code(update_in(&mut state, foreign, 2)),
            "factory.custody.not_holder"
        );
        assert_eq!(state, before, "a foreign occupant writes nothing");

        let mut own = update(&reference, CustodyState::Completed);
        own.actor = Some(CustodyActor {
            position_ref: holder.clone(),
            generation_ref: Some("actuation:generation:holder".into()),
        });
        let receipt = update_in(&mut state, own, 3).unwrap();
        let last = receipt.custody.transitions.last().unwrap();
        assert_eq!(last.actor.as_ref().unwrap().position_ref, holder);
        assert_eq!(
            last.actor.as_ref().unwrap().generation_ref.as_deref(),
            Some("actuation:generation:holder")
        );
    }

    #[test]
    fn a_position_stamp_without_a_generation_writes_nothing() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let holder = state.work_custody[0].position_ref.clone();
        let before = state.clone();
        let mut bare = update(&reference, CustodyState::Completed);
        bare.actor = Some(CustodyActor {
            position_ref: holder,
            generation_ref: None,
        });
        assert_eq!(
            refusal_code(update_in(&mut state, bare, 2)),
            "factory.custody.generation_required"
        );
        assert_eq!(state, before);
        assert_eq!(
            published_actor(None).unwrap_err().code,
            "factory.custody.actor_required"
        );
    }

    #[test]
    fn custody_update_accepts_only_the_generation_actuation_calls_current() {
        let actor = CustodyActor {
            position_ref: "central:position:project:O-I:factory-guardian".into(),
            generation_ref: Some("actuation:generation:holder".into()),
        };
        let dir =
            std::env::temp_dir().join(format!("factory-custody-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("actuation");
        crate::test_executable::write_executable(
            &binary,
            "#!/bin/sh\ncase \"$*\" in\n  *actuation:generation:holder*) echo '{\"ok\":true,\"verb\":\"verify\"}' ;;\n  *) echo '{\"ok\":false}' >&2; exit 2 ;;\nesac\n",
        );
        let prior = std::env::var("ACTUATION_BIN").ok();
        std::env::set_var("ACTUATION_BIN", &binary);
        let accepted = confirm_current_generation(&actor);
        let stale = CustodyActor {
            generation_ref: Some("actuation:generation:old".into()),
            ..actor
        };
        let refused = confirm_current_generation(&stale);
        match prior {
            Some(value) => std::env::set_var("ACTUATION_BIN", value),
            None => std::env::remove_var("ACTUATION_BIN"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(accepted.is_ok(), "{accepted:?}");
        assert_eq!(
            refused.unwrap_err().code,
            "factory.custody.occupancy_unverified"
        );
    }

    #[test]
    fn hand_off_names_a_receiver_and_creates_the_successor_in_the_same_write() {
        let mut state = state();
        let reference = assign_in(&mut state, request("work:a"), 1)
            .unwrap()
            .custody
            .custody_ref;
        assert_eq!(
            refusal_code(update_in(
                &mut state,
                update(&reference, CustodyState::HandedOff),
                2
            )),
            "factory.custody.handoff_target_required"
        );
        let mut to_self = update(&reference, CustodyState::HandedOff);
        to_self.to_position_ref = Some(P.into());
        assert_eq!(
            refusal_code(update_in(&mut state, to_self, 2)),
            "factory.custody.invalid_transition"
        );
        let mut misplaced = update(&reference, CustodyState::Released);
        misplaced.to_position_ref = Some(Q.into());
        assert_eq!(
            refusal_code(update_in(&mut state, misplaced, 2)),
            "factory.custody.usage"
        );
        let mut handoff = update(&reference, CustodyState::HandedOff);
        handoff.to_position_ref = Some(Q.into());
        let receipt = update_in(&mut state, handoff.clone(), 3).unwrap();
        let replay = update_in(&mut state, handoff, 4).unwrap();
        assert_eq!(replay.result, "unchanged");
        assert!(replay.successor.is_none());
        let mut elsewhere = update(&reference, CustodyState::HandedOff);
        elsewhere.to_position_ref = Some("central:position:project:O-I:third".into());
        assert_eq!(
            refusal_code(update_in(&mut state, elsewhere, 5)),
            "factory.custody.invalid_transition"
        );
        let successor = receipt.successor.unwrap();
        assert_eq!(successor.position_ref, Q);
        assert_eq!(successor.state, CustodyState::InProgress);
        assert_eq!(
            successor.handed_off_from.as_deref(),
            Some(reference.as_str())
        );
        assert_eq!(
            receipt.custody.handed_off_to,
            Some(CustodyHandoff {
                custody_ref: successor.custody_ref.clone(),
                position_ref: Q.into()
            })
        );
        assert_eq!(state.work_custody.len(), 2);
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn tampered_history_is_refused_on_read() {
        let mut state = state();
        assign_in(&mut state, request("work:a"), 1).unwrap();
        let mut tampered = state.work_custody.clone();
        tampered[0].state = CustodyState::Completed;
        assert!(validate_custody_collection(&tampered).is_err());
        let mut duplicated = state.work_custody.clone();
        duplicated.push(duplicated[0].clone());
        assert!(validate_custody_collection(&duplicated).is_err());
    }

    /// DLW §19.4 Custody: "Software Factory owns how much it decides alone:
    /// autonomy, the review gate and what it holds" — Autonomy Suggest · Review
    /// · Auto, Review gate Diffs · Approvals · Both, Escalate to "A person".
    fn settled(work: &str) -> AssignRequest {
        AssignRequest {
            autonomy: Some(CustodyAutonomy::Suggest),
            review_gate: Some(ReviewGate::Approvals),
            escalate_to: Some("central:person:frank".into()),
            ..request(work)
        }
    }

    #[test]
    fn an_assignment_keeps_its_settings_verbatim_and_absent_until_set() {
        let mut state = state();
        let set = assign_in(&mut state, settled("work:set"), 1)
            .unwrap()
            .custody;
        assert_eq!(set.autonomy, Some(CustodyAutonomy::Suggest));
        assert_eq!(set.review_gate, Some(ReviewGate::Approvals));
        assert_eq!(set.escalate_to.as_deref(), Some("central:person:frank"));
        let wire = serde_json::to_value(&set).unwrap();
        assert_eq!(wire["autonomy"], "suggest");
        assert_eq!(wire["review_gate"], "approvals");
        assert_eq!(wire["escalate_to"], "central:person:frank");
        let unset = assign_in(&mut state, request("work:unset"), 2)
            .unwrap()
            .custody;
        let wire = serde_json::to_value(&unset).unwrap();
        for key in ["autonomy", "review_gate", "escalate_to"] {
            assert!(
                wire.get(key).is_none(),
                "{key}: absent until set, never the manual's starting value"
            );
        }
        // A record written before these fields existed reads back unchanged.
        let old: FactoryWorkCustody = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(old, unset);
        validate_custody_collection(&state.work_custody).unwrap();
        for (value, words) in [
            ("suggest", Some(CustodyAutonomy::Suggest)),
            ("review", Some(CustodyAutonomy::Review)),
            ("auto", Some(CustodyAutonomy::Auto)),
            ("Auto", None),
        ] {
            assert_eq!(CustodyAutonomy::parse(value), words, "{value}");
        }
        for (value, gate) in [
            ("diffs", Some(ReviewGate::Diffs)),
            ("approvals", Some(ReviewGate::Approvals)),
            ("both", Some(ReviewGate::Both)),
            ("all", None),
        ] {
            assert_eq!(ReviewGate::parse(value), gate, "{value}");
        }
    }

    #[test]
    fn a_replayed_assignment_needs_the_same_settings_and_a_bad_person_ref_is_refused() {
        let mut state = state();
        let first = assign_in(
            &mut state,
            AssignRequest {
                custody_ref: Some(
                    "factory:custody:0a1b2c3d-0000-4000-8000-00000000000a".to_string(),
                ),
                ..settled("work:r")
            },
            1,
        )
        .unwrap();
        assert_eq!(first.result, "applied");
        let again = assign_in(
            &mut state,
            AssignRequest {
                custody_ref: Some(first.custody.custody_ref.clone()),
                ..settled("work:r")
            },
            2,
        )
        .unwrap();
        assert_eq!(again.result, "already-applied");
        let other = AssignRequest {
            custody_ref: Some(first.custody.custody_ref.clone()),
            autonomy: Some(CustodyAutonomy::Auto),
            ..settled("work:r")
        };
        assert_eq!(
            refusal_code(assign_in(&mut state, other, 3)),
            "factory.custody.identity_conflict"
        );
        for person in ["", "  ", " central:person:frank", "central:person:\nfrank"] {
            let bad = AssignRequest {
                escalate_to: Some(person.into()),
                ..request("work:bad")
            };
            assert_eq!(
                refusal_code(assign_in(&mut state, bad, 4)),
                "factory.custody.usage",
                "{person:?}"
            );
        }
        assert_eq!(
            state.work_custody.len(),
            1,
            "nothing persisted by a refusal"
        );
        let mut tampered = state.work_custody.clone();
        tampered[0].escalate_to = Some(String::new());
        assert!(validate_custody_collection(&tampered).is_err());
    }

    #[test]
    fn a_hand_off_does_not_carry_the_settings_and_a_state_change_keeps_them() {
        let mut state = state();
        let reference = assign_in(&mut state, settled("work:h"), 1)
            .unwrap()
            .custody
            .custody_ref;
        let blocked = update_in(&mut state, update(&reference, CustodyState::Blocked), 2).unwrap();
        assert_eq!(
            blocked.custody.autonomy,
            Some(CustodyAutonomy::Suggest),
            "a state change keeps the holder's settings"
        );
        let handed = update_in(
            &mut state,
            UpdateRequest {
                to_position_ref: Some(Q.into()),
                ..update(&reference, CustodyState::HandedOff)
            },
            3,
        )
        .unwrap();
        let successor = handed.successor.unwrap();
        assert_eq!(
            (
                successor.autonomy,
                successor.review_gate,
                successor.escalate_to.clone()
            ),
            (None, None, None),
            "the receiver's holding is its own"
        );
        assert_eq!(handed.custody.review_gate, Some(ReviewGate::Approvals));
        validate_custody_collection(&state.work_custody).unwrap();
    }

    #[test]
    fn the_cli_takes_the_settings_on_assign_and_refuses_a_word_it_does_not_own() {
        use crate::developmental_read::FactoryDevelopmentalFileProvider;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("custody-settings.json");
        FactoryDevelopmentalFileProvider::create_new(&path, state()).unwrap();
        let run = |extra: &[&str]| {
            let mut args: Vec<String> = [
                "custody",
                "assign",
                path.to_str().unwrap(),
                "--position",
                P,
                "--work",
                "work:cli",
                "--reason",
                "assigned",
            ]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
            args.extend(extra.iter().map(|arg| arg.to_string()));
            crate::inhabitation_cli::execute(&args, true)
        };
        let refused = run(&["--autonomy", "sometimes"]).unwrap_err();
        assert_eq!(refused.code, "factory.custody.usage");
        assert!(
            refused.fact.contains("suggest, review or auto"),
            "{}",
            refused.fact
        );
        assert_eq!(
            run(&["--review-gate", "everything"]).unwrap_err().code,
            "factory.custody.usage"
        );
        let answer: serde_json::Value = serde_json::from_str(
            &run(&[
                "--autonomy",
                "auto",
                "--review-gate",
                "diffs",
                "--escalate-to",
                "central:person:frank",
            ])
            .unwrap(),
        )
        .unwrap();
        assert_eq!(answer["schema"], FACTORY_WORK_CUSTODY_RECEIPT);
        assert_eq!(answer["custody"]["autonomy"], "auto");
        assert_eq!(answer["custody"]["review_gate"], "diffs");
        assert_eq!(answer["custody"]["escalate_to"], "central:person:frank");
        let kept = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        assert_eq!(
            kept.state().work_custody[0].autonomy,
            Some(CustodyAutonomy::Auto),
            "kept in the developmental state"
        );
        // `update` does not take them: the holder deciding how much it decides alone is not that law's to grant.
        let update: Vec<String> = [
            "custody",
            "update",
            path.to_str().unwrap(),
            "--custody",
            "factory:custody:x",
            "--state",
            "blocked",
            "--reason",
            "r",
            "--autonomy",
            "auto",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let refusal = crate::inhabitation_cli::execute(&update, true).unwrap_err();
        assert!(
            refusal.fact.contains("unknown option --autonomy"),
            "{}",
            refusal.fact
        );
    }
}
