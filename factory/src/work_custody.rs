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
use std::path::Path;

pub const FACTORY_WORK_CUSTODY: &str = "factory.work-custody/v1";
pub const FACTORY_WORK_CUSTODY_RECEIPT: &str = "factory.work-custody-receipt/v1";
pub const FACTORY_WORK_CUSTODY_LISTING: &str = "factory.work-custody-listing/v1";
pub const FACTORY_REFUSAL: &str = "factory.refusal/v1";

pub const ASSIGN_USAGE: &str = "factory development custody assign [<state>] --position <central:position:…> --work <ref> --reason <text> [--run <run-ref> [--workflow-unit <ref>]] [--journey <ref>] [--origin-communique <ref>] [--state in-progress|blocked] [--custody <factory:custody:…>] [--json]";
pub const UPDATE_USAGE: &str = "factory development custody update [<state>] --custody <factory:custody:…> --state <in-progress|blocked|released|completed|handed-off> --reason <text> [--reopen] [--to-position <central:position:…>] [--expected-revision N] [--json]";

const CUSTODY_PREFIX: &str = "factory:custody:";
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
}

impl FactoryWorkCustody {
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
/// refusal is raised before any write, so the consequence always says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub schema: String,
    pub code: String,
    pub fact: String,
    pub consequence: String,
    pub action: String,
}

pub const NOTHING_PERSISTED: &str = "Nothing was persisted; the developmental state is unchanged.";
pub const NOTHING_READ: &str = "No reading was produced; the developmental state is unchanged.";

impl Refusal {
    pub fn new(
        code: &str,
        fact: impl Into<String>,
        consequence: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            schema: FACTORY_REFUSAL.into(),
            code: code.into(),
            fact: fact.into(),
            consequence: consequence.into(),
            action: action.into(),
        }
    }

    fn unchanged(code: &str, fact: impl Into<String>, action: impl Into<String>) -> Self {
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

impl std::error::Error for Refusal {}

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
    /// Only the Position holding the custody may change it.
    pub actor: Option<CustodyActor>,
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
        if (record.state == CustodyState::HandedOff) != record.handed_off_to.is_some() {
            return Err(format!(
                "custody {label} hand-off target does not match its state"
            ));
        }
    }
    Ok(())
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
        }],
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
            format!("custody {custody_ref} already exists with different work, Position or origin"),
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
    // A retried request for the state the record is already in is a replay.
    // A hand-off replays only toward the same receiver.
    let same_receiver = target != CustodyState::HandedOff
        || request.to_position_ref.as_deref()
            == current
                .handed_off_to
                .as_ref()
                .map(|handoff| handoff.position_ref.as_str());
    if target == current.state && !request.reopen && same_receiver {
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
            }],
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
    });
    record.state = target;
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
        .map_err(|error| state_refusal(path, &error))?
}

pub fn assign(path: &Path, request: AssignRequest) -> Result<CustodyReceipt, Refusal> {
    let now = now_unix_ms();
    transact(path, |state| assign_in(state, request, now))
}

pub fn update(path: &Path, request: UpdateRequest) -> Result<CustodyReceipt, Refusal> {
    let now = now_unix_ms();
    transact(path, |state| update_in(state, request, now))
}

pub(crate) fn state_refusal(path: &Path, error: &ProjectDevelopmentStoreError) -> Refusal {
    Refusal::new(
        "factory.state.unavailable",
        format!(
            "the developmental state at {} could not be read or written: {error}",
            path.display()
        ),
        "The outcome was not applied; the state file was not replaced.",
        "check the path with `factory project locate <project-root>` and retry against the statePath it reports",
    )
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

    fn update(custody_ref: &str, to: CustodyState) -> UpdateRequest {
        UpdateRequest {
            custody_ref: custody_ref.into(),
            state: Some(to),
            reason: format!("to {to}"),
            ..UpdateRequest::default()
        }
    }

    fn refusal_code<T: std::fmt::Debug>(result: Result<T, Refusal>) -> String {
        let refusal = result.unwrap_err();
        assert_eq!(refusal.consequence, NOTHING_PERSISTED);
        assert!(!refusal.fact.is_empty() && !refusal.action.is_empty());
        refusal.code
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
}
