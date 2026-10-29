//! How a task stands, read from evidence (`factory.task-activity/v1`).
//!
//! Factory runs nothing, so it keeps no live oracle: this is a read. The caller
//! carries the owners' own readings verbatim — AIKit's encounter status for the
//! task's session, the session's check-ins (its temporal events), whether AIKit
//! binds the task to the session — and Factory answers how the task stands,
//! with the evidence that decided it. Factory adds its own durable record when
//! it is asked against a developmental state: the custody of the task's work,
//! and the closure a closed custody carries (`work_custody.rs`).
//!
//! Ported from OpenRig (`mvschwarz/openrig@c8fca9d5`, Apache-2.0):
//!
//! - the three axes and their values: activity, session and resumability, with
//!   needs-input as a count and a reason, never a state
//!   (`packages/daemon/src/domain/activity-taxonomy.ts:22-57`), and the one
//!   bridge to the display value (`:64-74`, `deriveDisplayActivity`);
//! - the evidence ladder: the rungs in arbitration rank, rung trust, and what
//!   an adapter declares about itself (`activity-taxonomy.ts:78-125,183-188`);
//! - the arbitration: a rung's trust, with the generic sampling floor when no
//!   inventory is declared (`seat-activity-service.ts:384-389`), the newest
//!   activity-bearing evidence per rung (`:394-406`), the highest
//!   authoritative rung with usable evidence and hook evidence time-bounded
//!   (`:439-450`), needs-input from chrome over hooks over self-report
//!   (`:562-572`), and the rungs as the reading shows them (`:503-516`);
//! - sampling: a source is working while its last activity is younger than the
//!   window, idle at its prompt once it is not (`seat-activity-service.ts:96-102,130`);
//! - the stall, which OpenRig calls parked: a derived diagnosis, never a stored
//!   state — (idle at the prompt or needs-input pending) × an open obligation,
//!   and unknown activity is indeterminate, never a guessed "not parked"
//!   (`packages/daemon/src/domain/parked-query.ts:143-182`);
//! - the launch posture beside, never folded into, the activity: a session not
//!   yet ready keeps its work waiting (`types.ts:640` `startupStatus`, and the
//!   TUI's precedence `packages/tui/src/hydrate.ts:212-277`).
//!
//! Not ported, because each needs memory across reads that a read has not:
//! the sampling debounce (`seat-activity-service.ts:530-556`), trial-rung
//! promotion (`:412-434`) and the cross-rung contradiction downgrade
//! (`:477-501`). A trial rung is therefore reported and never consulted, as
//! OpenRig does before promotion.
//!
//! Factory's own part is the mapping onto the manual's five words (DLW §19.1,
//! "A task is drawn by how it stands"): a closed custody is Done; a session
//! AIKit reports closed is Stopped; a session not ready is Queued with AIKit's
//! reason; a stall is Stalled and lights a retry; working is Running; and where
//! no evidence decides, no word is drawn.

use crate::developmental_read::FactoryDevelopmentalState;
use crate::work_custody::{CustodyClosure, CustodyState, Refusal, NOTHING_READ};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FACTORY_TASK_ACTIVITY_REQUEST: &str = "factory.task-activity-request/v1";
pub const FACTORY_TASK_ACTIVITY: &str = "factory.task-activity/v1";
pub const TASK_ACTIVITY_USAGE: &str =
    "factory development task-activity [<state>] --request <file|-> [--json]";

/// The check-in bound: a session that has not checked in for this long is
/// idle at its prompt. Carried over from the bound the shell drew by
/// (`agentRunModel.ts` `HEARTBEAT_HORIZON_MS`, 90 s), which named itself chosen,
/// not measured; a request may name its own (`bound_ms`), as OpenRig lets a
/// seat name its silence window (`types.ts:923-929`).
pub const CHECK_IN_BOUND_MS: i64 = 90_000;
/// Hook evidence older than this no longer decides working or idle
/// (`seat-activity-service.ts:641`).
pub const HOOK_AUTHORITY_WINDOW_MS: i64 = 15_000;

// ── The taxonomy (activity-taxonomy.ts:22-74) ──

/// Activity axis: what a present agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityValue {
    Working,
    IdleAtPrompt,
    Unknown,
}

/// Session axis: does a process exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPresence {
    Present,
    Detached,
    Exited,
    Absent,
}

/// Resumability axis, kept apart from reachability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resumability {
    Live,
    Resumable,
    ContextWalled,
}

/// Needs-input as a count and a short reason, never a state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NeedsInput {
    pub count: u64,
    pub reason: Option<String>,
}

/// The human-facing activity value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayActivity {
    Working,
    Idle,
    NeedsInput,
    Unknown,
}

/// The one bridge from the typed axes to the display value: needs-input shows
/// whenever its count is positive.
pub fn derive_display_activity(
    activity: ActivityValue,
    needs_input: &NeedsInput,
) -> DisplayActivity {
    if needs_input.count > 0 {
        return DisplayActivity::NeedsInput;
    }
    match activity {
        ActivityValue::IdleAtPrompt => DisplayActivity::Idle,
        ActivityValue::Working => DisplayActivity::Working,
        ActivityValue::Unknown => DisplayActivity::Unknown,
    }
}

// ── The evidence ladder (activity-taxonomy.ts:78-125) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceRung {
    SelfReport,
    LifecycleHooks,
    NeedsInputChrome,
    WindowSampling,
}

/// The rungs in arbitration rank for working or idle, top first. Needs-input
/// chrome decides only needs-input.
pub const EVIDENCE_RUNG_RANK: [EvidenceRung; 3] = [
    EvidenceRung::SelfReport,
    EvidenceRung::LifecycleHooks,
    EvidenceRung::WindowSampling,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RungTrust {
    Authoritative,
    Trial,
    IdentityOnly,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleCoverage {
    Full,
    Partial,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RungDeclaration {
    pub rung: EvidenceRung,
    pub lifecycle_coverage: LifecycleCoverage,
    pub initial_trust: RungTrust,
}

/// What an adapter declares about itself: arbitration ranks what each source
/// can actually observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AdapterRungInventory {
    pub adapter_id: &'static str,
    pub runtime: &'static str,
    pub rungs: &'static [RungDeclaration],
}

/// The generic floor: sampling only (`activity-taxonomy.ts:183-188`).
pub const TMUX_GENERIC_RUNG_INVENTORY: AdapterRungInventory = AdapterRungInventory {
    adapter_id: "tmux-generic",
    runtime: "tmux-generic",
    rungs: &[RungDeclaration {
        rung: EvidenceRung::WindowSampling,
        lifecycle_coverage: LifecycleCoverage::Full,
        initial_trust: RungTrust::Authoritative,
    }],
};

/// AIKit's encounter host, declared here: OpenRig has no AIKit adapter. Its
/// pending permission requests are positive evidence that the session waits
/// for the person (every request is held by the host until it is decided), so
/// that rung is authoritative. Its lane state (`Resident`, `TurnInFlight`)
/// says a turn is open at the protocol, not that the agent is producing — a
/// hung tool keeps `TurnInFlight` — so it covers the lifecycle only in part and
/// is identity-only, never consulted for working or idle (herdr's cut: only
/// full coverage is ever authoritative). The session's check-ins are the
/// sampling floor: working or idle is decided by the check-in bound.
pub const AIKIT_ENCOUNTER_RUNG_INVENTORY: AdapterRungInventory = AdapterRungInventory {
    adapter_id: "aikit-encounter",
    runtime: "aikit-encounter",
    rungs: &[
        RungDeclaration {
            rung: EvidenceRung::SelfReport,
            lifecycle_coverage: LifecycleCoverage::Partial,
            initial_trust: RungTrust::IdentityOnly,
        },
        RungDeclaration {
            rung: EvidenceRung::NeedsInputChrome,
            lifecycle_coverage: LifecycleCoverage::Full,
            initial_trust: RungTrust::Authoritative,
        },
        RungDeclaration {
            rung: EvidenceRung::WindowSampling,
            lifecycle_coverage: LifecycleCoverage::Full,
            initial_trust: RungTrust::Authoritative,
        },
    ],
};

/// One piece of evidence for the ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityEvidence {
    pub rung: EvidenceRung,
    pub source_id: String,
    pub seq: u64,
    pub observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<ActivityValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_input: Option<NeedsInput>,
}

/// One rung as the reading shows it (`seat-activity-service.ts:503-516`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RungView {
    pub rung: EvidenceRung,
    pub source_id: String,
    pub trust: RungTrust,
    pub last_evidence_at_unix_ms: Option<i64>,
}

/// The arbitrated answer for one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArbitratedActivity {
    pub activity: ActivityValue,
    pub needs_input: NeedsInput,
    /// Which rung decided the activity: confidence made visible.
    pub decided_by: Option<EvidenceRung>,
    pub rungs: Vec<RungView>,
}

/// A rung's trust: declared trust when an inventory exists; without one the
/// generic floor is authoritative and every other rung identity-only.
pub fn rung_trust(inventory: Option<&AdapterRungInventory>, rung: EvidenceRung) -> RungTrust {
    match inventory {
        Some(inventory) => inventory
            .rungs
            .iter()
            .find(|declared| declared.rung == rung)
            .map(|declared| declared.initial_trust)
            .unwrap_or(RungTrust::Absent),
        None if rung == EvidenceRung::WindowSampling => RungTrust::Authoritative,
        None => RungTrust::IdentityOnly,
    }
}

/// The newest evidence for a rung; `activity_only` keeps only evidence that
/// says something about working or idle.
fn latest_by_rung(
    evidence: &[ActivityEvidence],
    rung: EvidenceRung,
    activity_only: bool,
) -> Option<&ActivityEvidence> {
    evidence
        .iter()
        .filter(|item| item.rung == rung && (!activity_only || item.activity.is_some()))
        .fold(None, |best: Option<&ActivityEvidence>, item| match best {
            Some(best) if best.seq >= item.seq => Some(best),
            _ => Some(item),
        })
}

/// Arbitrate the ladder at `now`.
pub fn arbitrate(
    inventory: Option<&AdapterRungInventory>,
    evidence: &[ActivityEvidence],
    now_unix_ms: i64,
) -> ArbitratedActivity {
    let candidate = EVIDENCE_RUNG_RANK.iter().find_map(|rung| {
        if rung_trust(inventory, *rung) != RungTrust::Authoritative {
            return None;
        }
        let found = latest_by_rung(evidence, *rung, true)?;
        if *rung == EvidenceRung::LifecycleHooks
            && now_unix_ms - found.observed_at_unix_ms > HOOK_AUTHORITY_WINDOW_MS
        {
            return None;
        }
        Some(found)
    });
    let needs_input = [
        EvidenceRung::NeedsInputChrome,
        EvidenceRung::LifecycleHooks,
        EvidenceRung::SelfReport,
    ]
    .iter()
    .find_map(|rung| {
        let found = latest_by_rung(evidence, *rung, false)?;
        let needs = found.needs_input.as_ref()?;
        (rung_trust(inventory, *rung) == RungTrust::Authoritative).then(|| needs.clone())
    })
    .unwrap_or_default();
    let mut rungs: Vec<EvidenceRung> = inventory
        .map(|inventory| {
            inventory
                .rungs
                .iter()
                .map(|declared| declared.rung)
                .collect()
        })
        .unwrap_or_default();
    for item in evidence {
        if !rungs.contains(&item.rung) {
            rungs.push(item.rung);
        }
    }
    rungs.sort();
    ArbitratedActivity {
        activity: candidate
            .and_then(|found| found.activity)
            .unwrap_or(ActivityValue::Unknown),
        needs_input,
        decided_by: candidate.map(|found| found.rung),
        rungs: rungs
            .into_iter()
            .map(|rung| {
                let latest = latest_by_rung(evidence, rung, false);
                RungView {
                    rung,
                    source_id: latest
                        .map(|found| found.source_id.clone())
                        .unwrap_or_else(|| {
                            format!(
                                "{}:{}",
                                inventory.map(|i| i.adapter_id).unwrap_or("undeclared"),
                                serde_json::to_value(rung)
                                    .ok()
                                    .and_then(|value| value.as_str().map(str::to_owned))
                                    .unwrap_or_default()
                            )
                        }),
                    trust: rung_trust(inventory, rung),
                    last_evidence_at_unix_ms: latest.map(|found| found.observed_at_unix_ms),
                }
            })
            .collect(),
    }
}

/// The sampling rung's own decision: working while the last activity is
/// younger than the window; a negative age (clock skew) reads as working.
pub fn sampled_activity(
    last_activity_unix_ms: i64,
    now_unix_ms: i64,
    window_ms: i64,
) -> ActivityValue {
    if now_unix_ms - last_activity_unix_ms < window_ms {
        ActivityValue::Working
    } else {
        ActivityValue::IdleAtPrompt
    }
}

// ── The request: the owners' readings, carried verbatim ──

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskActivityRequest {
    pub schema: String,
    #[serde(default)]
    pub bound_ms: Option<i64>,
    pub tasks: Vec<TaskEvidence>,
}

/// One task's evidence: each field is an owner's own reading, or absent.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEvidence {
    /// The AIKit session the task is bound to.
    pub agent_session: String,
    /// The task's own ref (`aikit.encounter-task/v1` `request.central.task_ref`),
    /// the work its custody names.
    #[serde(default)]
    pub task_ref: Option<String>,
    /// AIKit binds this task to the session (`encounter-task-read` answered it).
    #[serde(default)]
    pub bound: bool,
    /// AIKit's encounter `status` reading for the session, verbatim.
    #[serde(default)]
    pub connection: Option<Value>,
    /// AIKit's refusal of that status read, in its own words.
    #[serde(default)]
    pub connection_refusal: Option<String>,
    /// When the status was read.
    #[serde(default)]
    pub connection_read_at_unix_ms: Option<i64>,
    /// The session's check-ins: its temporal events.
    #[serde(default)]
    pub check_ins: Option<CheckIns>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckIns {
    /// Where they were read (the temporal stream the caller read).
    pub source_ref: String,
    /// How many check-ins the read found for the session.
    pub count: u64,
    /// The newest one, when there is one.
    #[serde(default)]
    pub latest_unix_ms: Option<i64>,
    pub read_at_unix_ms: i64,
}

// ── The reading ──

/// The manual's five words (DLW §19.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskWord {
    Queued,
    Running,
    Stalled,
    Stopped,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Parked {
    Parked,
    NotParked,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationState {
    Open,
    Closed,
    Unknown,
}

/// One custody of the task's work, as Factory holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObligationCustody {
    pub custody_ref: String,
    pub state: CustodyState,
    pub position_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closure: Option<CustodyClosure>,
}

/// The obligation face the stall is read against, with the scope that ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Obligation {
    pub state: ObligationState,
    pub scope: String,
    pub custody: Vec<ObligationCustody>,
}

/// The launch posture: whether the session can take its task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Readiness {
    pub ready: bool,
    /// AIKit's state word, verbatim.
    pub owner_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskStateReading {
    pub agent_session: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<String>,
    /// How the task stands, in the manual's words; absent where no evidence decides.
    pub word: Option<TaskWord>,
    /// The evidence that decided it, in words.
    pub why: String,
    /// The track lights a retry.
    pub retry: bool,
    pub activity: ActivityValue,
    pub display: DisplayActivity,
    pub needs_input: NeedsInput,
    pub decided_by: Option<EvidenceRung>,
    pub session: Option<SessionPresence>,
    pub resumability: Option<Resumability>,
    pub readiness: Option<Readiness>,
    pub parked: Parked,
    pub obligation: Obligation,
    pub last_check_in_unix_ms: Option<i64>,
    pub adapter: &'static str,
    pub rungs: Vec<RungView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskActivityReading {
    pub schema: &'static str,
    pub read_at_unix_ms: i64,
    pub bound_ms: i64,
    /// `factory` (the check-in bound Factory keeps) or `request`.
    pub bound_from: &'static str,
    /// Whether Factory's custody was read, and from which project.
    pub custody_scope: String,
    pub tasks: Vec<TaskStateReading>,
}

// ── AIKit's adapter: its words onto the axes ──

/// AIKit's session axes and evidence, read from its encounter status.
/// AIKit's words: `Resident`, `TurnInFlight`, `InterruptRequested`
/// (`aikit-adapters/src/agent_session_host.rs:159-164`); `Opening`,
/// `Releasing`, `Released` (`aikit-cli/src/encounter_service.rs:860-886`);
/// `Disconnected` (`:2968`); `RecoveryRequired`, `CleanupUncertain` (`:2993`).
struct AikitAxes {
    session: Option<SessionPresence>,
    readiness: Option<Readiness>,
    evidence: Vec<ActivityEvidence>,
    declared: bool,
}

const AIKIT_STATUS_SOURCE: &str = "aikit:encounter-status";
/// AIKit's refusal for a session no host holds (`encounter_service.rs:1147`).
const AIKIT_NOT_RESIDENT: &str = "no resident native session";

fn aikit_axes(task: &TaskEvidence, now_unix_ms: i64) -> AikitAxes {
    let read_at = task.connection_read_at_unix_ms.unwrap_or(now_unix_ms);
    let Some(connection) = task.connection.as_ref() else {
        let absent = task
            .connection_refusal
            .as_deref()
            .is_some_and(|words| words.contains(AIKIT_NOT_RESIDENT));
        return AikitAxes {
            session: absent.then_some(SessionPresence::Absent),
            readiness: absent.then(|| Readiness {
                ready: false,
                owner_state: "Disconnected".into(),
                why: Some("AIKit holds no open session for it".into()),
            }),
            evidence: Vec::new(),
            declared: false,
        };
    };
    let state = connection["state"].as_str().unwrap_or("").to_owned();
    let fault = connection["error"]
        .as_str()
        .filter(|error| !error.trim().is_empty())
        .map(str::to_owned);
    let (session, ready, not_ready) = match state.as_str() {
        "Resident" | "TurnInFlight" | "InterruptRequested" => (
            Some(SessionPresence::Present),
            fault.is_none(),
            fault
                .as_ref()
                .map(|error| format!("AIKit reports a transport fault on the session: {error}")),
        ),
        "Opening" => (
            Some(SessionPresence::Present),
            false,
            Some("AIKit is still opening the session (Opening): its handshake has not finished, so the task waits".into()),
        ),
        "Releasing" | "Released" => (Some(SessionPresence::Exited), false, None),
        "Disconnected" => (Some(SessionPresence::Absent), false, None),
        "RecoveryRequired" | "CleanupUncertain" => (
            Some(SessionPresence::Detached),
            false,
            Some(format!(
                "AIKit needs the session's open recovered ({state}) before a task runs"
            )),
        ),
        _ => (None, false, None),
    };
    let mut evidence = Vec::new();
    if session == Some(SessionPresence::Present) && fault.is_none() {
        let activity = match state.as_str() {
            "TurnInFlight" | "InterruptRequested" => Some(ActivityValue::Working),
            "Resident" => Some(ActivityValue::IdleAtPrompt),
            _ => None,
        };
        if activity.is_some() {
            evidence.push(ActivityEvidence {
                rung: EvidenceRung::SelfReport,
                source_id: AIKIT_STATUS_SOURCE.into(),
                seq: 1,
                observed_at_unix_ms: read_at,
                activity,
                needs_input: None,
            });
        }
        let pending = connection["permissions"]
            .as_array()
            .map(|requests| requests.len() as u64);
        if let Some(count) = pending {
            evidence.push(ActivityEvidence {
                rung: EvidenceRung::NeedsInputChrome,
                source_id: format!("{AIKIT_STATUS_SOURCE}:permissions"),
                seq: 1,
                observed_at_unix_ms: read_at,
                activity: None,
                needs_input: Some(NeedsInput {
                    count,
                    reason: (count > 0).then(|| "permission request".into()),
                }),
            });
        }
    }
    AikitAxes {
        readiness: session.map(|_| Readiness {
            ready,
            owner_state: state.clone(),
            why: if ready { None } else { not_ready },
        }),
        session,
        evidence,
        declared: true,
    }
}

// ── The obligation face ──

fn obligation_of(task: &TaskEvidence, state: Option<&FactoryDevelopmentalState>) -> Obligation {
    let binding = if task.bound {
        "AIKit binds the task to the session"
    } else {
        "AIKit names no task bound to the session"
    };
    let Some(state) = state else {
        return Obligation {
            state: if task.bound {
                ObligationState::Open
            } else {
                ObligationState::Unknown
            },
            scope: format!("{binding}; no Factory state was named, so its custody was not read"),
            custody: Vec::new(),
        };
    };
    let Some(work) = task.task_ref.as_deref() else {
        return Obligation {
            state: if task.bound {
                ObligationState::Open
            } else {
                ObligationState::Unknown
            },
            scope: format!("{binding}; it names no task ref, so Factory custody cannot name it"),
            custody: Vec::new(),
        };
    };
    let mut custody: Vec<ObligationCustody> = state
        .work_custody
        .iter()
        .filter(|record| record.work_ref == work)
        .map(|record| ObligationCustody {
            custody_ref: record.custody_ref.clone(),
            state: record.state,
            position_ref: record.position_ref.clone(),
            closure: record.closure.clone(),
        })
        .collect();
    custody.sort_by(|left, right| left.custody_ref.cmp(&right.custody_ref));
    let scope = format!(
        "{binding}; Factory custody of work={work} in {} (every record, uncapped)",
        state.project_ref()
    );
    let obligation = if custody.iter().any(|record| !record.state.is_terminal()) {
        ObligationState::Open
    } else if !custody.is_empty() {
        ObligationState::Closed
    } else if task.bound {
        ObligationState::Open
    } else {
        ObligationState::Unknown
    };
    Obligation {
        state: obligation,
        scope,
        custody,
    }
}

// ── The reading ──

fn seconds(ms: i64) -> i64 {
    ms.max(0) / 1000
}

fn closure_words(custody: &[ObligationCustody]) -> String {
    let closed: Vec<String> = custody
        .iter()
        .map(|record| match &record.closure {
            Some(closure) => format!(
                "{} {}{}",
                record.state,
                closure.reason,
                closure
                    .target
                    .as_deref()
                    .map(|target| format!(" → {target}"))
                    .unwrap_or_default()
            ),
            None => format!("{} before Factory recorded closure reasons", record.state),
        })
        .collect();
    closed.join("; ")
}

/// How one task stands at `now`.
pub fn read_task(
    task: &TaskEvidence,
    state: Option<&FactoryDevelopmentalState>,
    bound_ms: i64,
    now_unix_ms: i64,
) -> TaskStateReading {
    let axes = aikit_axes(task, now_unix_ms);
    let mut evidence = axes.evidence;
    let last_check_in = task
        .check_ins
        .as_ref()
        .and_then(|check| check.latest_unix_ms);
    if let (Some(check), Some(latest)) = (task.check_ins.as_ref(), last_check_in) {
        evidence.push(ActivityEvidence {
            rung: EvidenceRung::WindowSampling,
            source_id: check.source_ref.clone(),
            seq: check.count.max(1),
            observed_at_unix_ms: check.read_at_unix_ms,
            activity: Some(sampled_activity(latest, now_unix_ms, bound_ms)),
            needs_input: None,
        });
    }
    let inventory = axes.declared.then_some(&AIKIT_ENCOUNTER_RUNG_INVENTORY);
    let arbitrated = arbitrate(inventory, &evidence, now_unix_ms);
    let obligation = obligation_of(task, state);
    let activity_known = arbitrated.activity != ActivityValue::Unknown;
    let stopped =
        arbitrated.activity == ActivityValue::IdleAtPrompt || arbitrated.needs_input.count > 0;
    let parked = if !activity_known {
        Parked::Indeterminate
    } else if !stopped {
        Parked::NotParked
    } else {
        match obligation.state {
            ObligationState::Open => Parked::Parked,
            ObligationState::Closed => Parked::NotParked,
            ObligationState::Unknown => Parked::Indeterminate,
        }
    };
    let age = last_check_in.map(|latest| seconds(now_unix_ms - latest));
    let bound_s = seconds(bound_ms);
    let (word, why) = if obligation.state == ObligationState::Closed {
        (
            Some(TaskWord::Done),
            format!("Closed: {}.", closure_words(&obligation.custody)),
        )
    } else if let Some(gone) = axes
        .session
        .filter(|session| matches!(session, SessionPresence::Exited | SessionPresence::Absent))
    {
        let owner = axes
            .readiness
            .as_ref()
            .map(|readiness| readiness.owner_state.clone())
            .unwrap_or_default();
        (
            Some(TaskWord::Stopped),
            format!(
                "AIKit reports the session {} ({owner}): it is not open, and its task is not closed.",
                serde_json::to_value(gone)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default()
            ),
        )
    } else if let Some(readiness) = axes.readiness.as_ref().filter(|readiness| !readiness.ready) {
        (
            Some(TaskWord::Queued),
            format!(
                "{}.",
                readiness.why.clone().unwrap_or_else(|| format!(
                    "AIKit reports the session {}: it is not ready",
                    readiness.owner_state
                ))
            ),
        )
    } else if parked == Parked::Parked {
        let why = if arbitrated.needs_input.count > 0 {
            format!(
                "Waits for you: {} {}{} pending in the session, with its task open.",
                arbitrated.needs_input.count,
                arbitrated
                    .needs_input
                    .reason
                    .as_deref()
                    .unwrap_or("request"),
                if arbitrated.needs_input.count == 1 {
                    ""
                } else {
                    "s"
                }
            )
        } else {
            format!(
                "No check-in within the {bound_s} s bound: the last was {} s ago, with its task open.",
                age.unwrap_or_default()
            )
        };
        (Some(TaskWord::Stalled), why)
    } else if arbitrated.activity == ActivityValue::Working {
        (
            Some(TaskWord::Running),
            format!(
                "Checked in {} s ago, within the {bound_s} s bound.",
                age.unwrap_or_default()
            ),
        )
    } else {
        let missing = if last_check_in.is_none() {
            "no check-in stands for the session".to_owned()
        } else if obligation.state == ObligationState::Unknown {
            format!("the session is idle, and {}", obligation.scope)
        } else {
            "no rung with usable evidence decides".to_owned()
        };
        (None, format!("Nothing decides how it stands: {missing}."))
    };
    TaskStateReading {
        agent_session: task.agent_session.clone(),
        task_ref: task.task_ref.clone(),
        retry: word == Some(TaskWord::Stalled),
        word,
        why,
        display: derive_display_activity(arbitrated.activity, &arbitrated.needs_input),
        activity: arbitrated.activity,
        needs_input: arbitrated.needs_input,
        decided_by: arbitrated.decided_by,
        resumability: (axes.session == Some(SessionPresence::Present))
            .then_some(Resumability::Live),
        session: axes.session,
        readiness: axes.readiness,
        parked,
        obligation,
        last_check_in_unix_ms: last_check_in,
        adapter: inventory
            .map(|inventory| inventory.adapter_id)
            .unwrap_or("undeclared"),
        rungs: arbitrated.rungs,
    }
}

fn refusal(code: &str, fact: impl Into<String>) -> Refusal {
    Refusal::new(
        code,
        fact,
        NOTHING_READ,
        format!("send a {FACTORY_TASK_ACTIVITY_REQUEST} document: {TASK_ACTIVITY_USAGE}"),
    )
}

/// Read every task in the request at `now`. Refuses a malformed request whole.
pub fn read_task_activity(
    request: &TaskActivityRequest,
    state: Option<&FactoryDevelopmentalState>,
    now_unix_ms: i64,
) -> Result<TaskActivityReading, Refusal> {
    if request.schema != FACTORY_TASK_ACTIVITY_REQUEST {
        return Err(refusal(
            "factory.task_activity.schema",
            format!(
                "the request is {}, not {FACTORY_TASK_ACTIVITY_REQUEST}",
                request.schema
            ),
        ));
    }
    if request.bound_ms.is_some_and(|bound| bound <= 0) {
        return Err(refusal(
            "factory.task_activity.bound",
            "bound_ms must be a positive number of milliseconds",
        ));
    }
    let mut sessions = std::collections::BTreeSet::new();
    for task in &request.tasks {
        if task.agent_session.trim().is_empty() {
            return Err(refusal(
                "factory.task_activity.session",
                "every task names the AIKit session it is bound to",
            ));
        }
        if !sessions.insert(task.agent_session.as_str()) {
            return Err(refusal(
                "factory.task_activity.session",
                format!(
                    "session {} is named twice; a session holds one bound task",
                    task.agent_session
                ),
            ));
        }
    }
    let bound_ms = request.bound_ms.unwrap_or(CHECK_IN_BOUND_MS);
    Ok(TaskActivityReading {
        schema: FACTORY_TASK_ACTIVITY,
        read_at_unix_ms: now_unix_ms,
        bound_ms,
        bound_from: if request.bound_ms.is_some() {
            "request"
        } else {
            "factory"
        },
        custody_scope: match state {
            Some(state) => format!("Factory custody read in {}", state.project_ref()),
            None => "no Factory state was named; custody was not read".into(),
        },
        tasks: request
            .tasks
            .iter()
            .map(|task| read_task(task, state, bound_ms, now_unix_ms))
            .collect(),
    })
}

/// Parse the request document, refusing by name.
pub fn parse_request(text: &str) -> Result<TaskActivityRequest, Refusal> {
    serde_json::from_str(text).map_err(|error| {
        refusal(
            "factory.task_activity.request",
            format!("the request is not a {FACTORY_TASK_ACTIVITY_REQUEST} document: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FactoryBuildState;
    use crate::core::run::{Project, ProjectRef};
    use crate::work_custody::{assign_in, update_in, AssignRequest, ClosureReason, UpdateRequest};
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;
    const S: &str = "agency/session-a";
    const P: &str = "central:position:project:O-I:builder";

    fn task() -> TaskEvidence {
        TaskEvidence {
            agent_session: S.into(),
            task_ref: Some("task:fixtures".into()),
            bound: true,
            connection: None,
            connection_refusal: None,
            connection_read_at_unix_ms: Some(NOW),
            check_ins: None,
        }
    }

    fn checked_in(ago_ms: i64) -> Option<CheckIns> {
        Some(CheckIns {
            source_ref: "aikit:temporal-events".into(),
            count: 4,
            latest_unix_ms: Some(NOW - ago_ms),
            read_at_unix_ms: NOW,
        })
    }

    fn status(state: &str, permissions: usize) -> Option<Value> {
        Some(json!({
            "agent_session": S, "state": state, "error": null,
            "permissions": (0..permissions).map(|i| json!({"native_request_id": format!("r{i}")})).collect::<Vec<_>>(),
        }))
    }

    fn read(task: &TaskEvidence) -> TaskStateReading {
        read_task(task, None, CHECK_IN_BOUND_MS, NOW)
    }

    #[test]
    fn a_stall_is_no_check_in_within_the_bound_and_lights_a_retry() {
        let mut quiet = task();
        quiet.connection = status("TurnInFlight", 0);
        quiet.check_ins = checked_in(CHECK_IN_BOUND_MS);
        let reading = read(&quiet);
        assert_eq!(reading.word, Some(TaskWord::Stalled));
        assert!(reading.retry);
        assert_eq!(reading.decided_by, Some(EvidenceRung::WindowSampling));
        assert_eq!(reading.parked, Parked::Parked);
        assert!(
            reading.why.contains("No check-in within the 90 s bound"),
            "{}",
            reading.why
        );
        // AIKit's TurnInFlight is reported, never consulted: a hung tool keeps it.
        let self_report = reading
            .rungs
            .iter()
            .find(|rung| rung.rung == EvidenceRung::SelfReport)
            .unwrap();
        assert_eq!(self_report.trust, RungTrust::IdentityOnly);

        let mut live = quiet.clone();
        live.check_ins = checked_in(CHECK_IN_BOUND_MS - 1);
        let reading = read(&live);
        assert_eq!(reading.word, Some(TaskWord::Running));
        assert!(!reading.retry);
        assert_eq!(reading.display, DisplayActivity::Working);
    }

    #[test]
    fn pending_permission_requests_stall_the_task_as_waiting_for_you() {
        let mut waiting = task();
        waiting.connection = status("TurnInFlight", 2);
        waiting.check_ins = checked_in(1_000);
        let reading = read(&waiting);
        assert_eq!(reading.word, Some(TaskWord::Stalled));
        assert_eq!(reading.display, DisplayActivity::NeedsInput);
        assert_eq!(reading.needs_input.count, 2);
        assert!(
            reading
                .why
                .starts_with("Waits for you: 2 permission requests"),
            "{}",
            reading.why
        );
    }

    #[test]
    fn nothing_is_drawn_where_no_evidence_stands() {
        let reading = read(&task());
        assert_eq!(reading.word, None);
        assert!(!reading.retry);
        assert_eq!(reading.parked, Parked::Indeterminate);
        assert_eq!(reading.activity, ActivityValue::Unknown);
        assert!(
            reading.why.contains("no check-in stands"),
            "{}",
            reading.why
        );
        // Idle with no obligation known cannot support a stall either.
        let mut unbound = task();
        unbound.bound = false;
        unbound.check_ins = checked_in(CHECK_IN_BOUND_MS * 3);
        let reading = read(&unbound);
        assert_eq!(reading.word, None);
        assert_eq!(reading.parked, Parked::Indeterminate);
    }

    #[test]
    fn a_session_not_ready_keeps_its_task_queued_and_says_why() {
        let mut opening = task();
        opening.connection = status("Opening", 0);
        opening.check_ins = checked_in(CHECK_IN_BOUND_MS * 2);
        let reading = read(&opening);
        assert_eq!(reading.word, Some(TaskWord::Queued));
        assert!(!reading.retry);
        assert!(!reading.readiness.as_ref().unwrap().ready);
        assert!(
            reading.why.contains("handshake has not finished"),
            "{}",
            reading.why
        );

        let mut faulted = task();
        faulted.connection =
            Some(json!({"state": "Resident", "error": "pipe closed", "permissions": []}));
        let reading = read(&faulted);
        assert_eq!(reading.word, Some(TaskWord::Queued));
        assert!(reading.why.contains("pipe closed"), "{}", reading.why);

        let mut recovering = task();
        recovering.connection = status("RecoveryRequired", 0);
        let reading = read(&recovering);
        assert_eq!(reading.word, Some(TaskWord::Queued));
        assert_eq!(reading.session, Some(SessionPresence::Detached));
    }

    #[test]
    fn a_closed_session_stops_its_open_task() {
        for (state, presence) in [
            ("Released", SessionPresence::Exited),
            ("Disconnected", SessionPresence::Absent),
        ] {
            let mut closed = task();
            closed.connection = status(state, 0);
            let reading = read(&closed);
            assert_eq!(reading.word, Some(TaskWord::Stopped), "{state}");
            assert_eq!(reading.session, Some(presence));
            assert!(reading.why.contains(state), "{}", reading.why);
        }
        let mut never_opened = task();
        never_opened.connection_refusal = Some(
            "This canonical encounter has no resident native session; explicit owner open is required".into(),
        );
        let reading = read(&never_opened);
        assert_eq!(reading.word, Some(TaskWord::Stopped));
        // Any other refusal decides nothing about the session.
        let mut refused = task();
        refused.connection_refusal = Some("Encounter is not attached".into());
        assert_eq!(read(&refused).session, None);
    }

    #[test]
    fn closed_custody_is_done_with_its_closure_reason() {
        let project: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();
        let mut state =
            FactoryDevelopmentalState::new(FactoryBuildState::empty(Project::new(project)), vec![])
                .unwrap();
        let custody = assign_in(
            &mut state,
            AssignRequest {
                position_ref: P.into(),
                work_ref: "task:fixtures".into(),
                reason: "assigned".into(),
                ..AssignRequest::default()
            },
            1,
        )
        .unwrap()
        .custody
        .custody_ref;
        let mut idle = task();
        idle.check_ins = checked_in(CHECK_IN_BOUND_MS * 2);
        let open = read_task(&idle, Some(&state), CHECK_IN_BOUND_MS, NOW);
        assert_eq!(open.word, Some(TaskWord::Stalled));
        assert_eq!(open.obligation.state, ObligationState::Open);
        assert!(open.obligation.scope.contains("work=task:fixtures"));
        update_in(
            &mut state,
            UpdateRequest {
                custody_ref: custody.clone(),
                state: Some(CustodyState::Completed),
                reason: "fixtures written".into(),
                closure_reason: Some(ClosureReason::NoFollowOn),
                ..UpdateRequest::default()
            },
            2,
        )
        .unwrap();
        let done = read_task(&idle, Some(&state), CHECK_IN_BOUND_MS, NOW);
        assert_eq!(done.word, Some(TaskWord::Done));
        assert!(!done.retry);
        assert_eq!(done.obligation.custody[0].custody_ref, custody);
        assert_eq!(done.why, "Closed: completed no-follow-on.");
    }

    #[test]
    fn the_ported_ladder_ranks_and_bounds_as_openrig_does() {
        let at = |rung, seq, observed: i64, activity| ActivityEvidence {
            rung,
            source_id: format!("{rung:?}"),
            seq,
            observed_at_unix_ms: observed,
            activity: Some(activity),
            needs_input: None,
        };
        let full: AdapterRungInventory = AdapterRungInventory {
            adapter_id: "full",
            runtime: "full",
            rungs: &[
                RungDeclaration {
                    rung: EvidenceRung::SelfReport,
                    lifecycle_coverage: LifecycleCoverage::Full,
                    initial_trust: RungTrust::Authoritative,
                },
                RungDeclaration {
                    rung: EvidenceRung::LifecycleHooks,
                    lifecycle_coverage: LifecycleCoverage::Full,
                    initial_trust: RungTrust::Authoritative,
                },
                RungDeclaration {
                    rung: EvidenceRung::WindowSampling,
                    lifecycle_coverage: LifecycleCoverage::Full,
                    initial_trust: RungTrust::Authoritative,
                },
            ],
        };
        // Self-report outranks hooks outranks sampling.
        let evidence = vec![
            at(
                EvidenceRung::WindowSampling,
                1,
                NOW,
                ActivityValue::IdleAtPrompt,
            ),
            at(
                EvidenceRung::LifecycleHooks,
                1,
                NOW,
                ActivityValue::IdleAtPrompt,
            ),
            at(EvidenceRung::SelfReport, 1, NOW, ActivityValue::Working),
        ];
        let decided = arbitrate(Some(&full), &evidence, NOW);
        assert_eq!(decided.decided_by, Some(EvidenceRung::SelfReport));
        assert_eq!(decided.activity, ActivityValue::Working);
        // Hook evidence older than its window falls through to sampling.
        let evidence = vec![
            at(
                EvidenceRung::WindowSampling,
                1,
                NOW,
                ActivityValue::IdleAtPrompt,
            ),
            at(
                EvidenceRung::LifecycleHooks,
                1,
                NOW - HOOK_AUTHORITY_WINDOW_MS - 1,
                ActivityValue::Working,
            ),
        ];
        let decided = arbitrate(Some(&full), &evidence, NOW);
        assert_eq!(decided.decided_by, Some(EvidenceRung::WindowSampling));
        // The newest evidence per rung decides; an undeclared seat has the sampling floor only.
        let evidence = vec![
            at(EvidenceRung::WindowSampling, 2, NOW, ActivityValue::Working),
            at(
                EvidenceRung::WindowSampling,
                1,
                NOW,
                ActivityValue::IdleAtPrompt,
            ),
            at(
                EvidenceRung::SelfReport,
                3,
                NOW,
                ActivityValue::IdleAtPrompt,
            ),
        ];
        let decided = arbitrate(None, &evidence, NOW);
        assert_eq!(decided.activity, ActivityValue::Working);
        assert_eq!(
            rung_trust(None, EvidenceRung::SelfReport),
            RungTrust::IdentityOnly
        );
        assert_eq!(
            rung_trust(Some(&TMUX_GENERIC_RUNG_INVENTORY), EvidenceRung::SelfReport),
            RungTrust::Absent
        );
        assert_eq!(sampled_activity(NOW + 5, NOW, 1), ActivityValue::Working);
    }

    #[test]
    fn a_request_is_checked_whole_before_anything_is_read() {
        let request = |body: Value| parse_request(&body.to_string());
        let refusal = request(json!({"schema": "x", "tasks": [], "extra": 1})).unwrap_err();
        assert_eq!(refusal.code, "factory.task_activity.request");
        let wrong = request(json!({"schema": "x", "tasks": []})).unwrap();
        assert_eq!(
            read_task_activity(&wrong, None, NOW).unwrap_err().code,
            "factory.task_activity.schema"
        );
        let twice = request(json!({"schema": FACTORY_TASK_ACTIVITY_REQUEST,
            "tasks": [{"agent_session": S}, {"agent_session": S}]}))
        .unwrap();
        assert_eq!(
            read_task_activity(&twice, None, NOW).unwrap_err().code,
            "factory.task_activity.session"
        );
        let named = request(json!({"schema": FACTORY_TASK_ACTIVITY_REQUEST, "bound_ms": 1000,
            "tasks": [{"agent_session": S, "bound": true,
                "check_ins": {"source_ref": "t", "count": 1, "latest_unix_ms": NOW - 2000, "read_at_unix_ms": NOW}}]}))
        .unwrap();
        let reading = read_task_activity(&named, None, NOW).unwrap();
        assert_eq!(reading.bound_from, "request");
        assert_eq!(reading.tasks[0].word, Some(TaskWord::Stalled));
        assert_eq!(reading.tasks[0].adapter, "undeclared");
    }
}
