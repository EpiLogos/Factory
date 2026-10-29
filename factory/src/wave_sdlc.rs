//! The planning dial and the wave SDLC: how much planning a unit of a Run
//! gets, and how the Run's units build, merge and are reviewed in waves.
//! Ported from OpenRig at c8fca9d5 (O-I #741): `docs/reference/planning-dial.md`,
//! `docs/reference/wave-sdlc.md`, `docs/reference/sdlc-conventions.md` (EC-1,
//! EC-2) and the read-time derivations of
//! `packages/daemon/src/domain/execution-view.ts` (the ladder, `next_up`, the
//! care dial). Factory keeps one plan per Run (`factory.wave-plan/v1`) in the
//! project's developmental state document, written only through its advisory
//! lock and atomic publisher. There is no second store, queue row or wave map.
//!
//! The planning dial (`planning-dial.md`). Each unit's plan stands on a rung,
//! set with its reason ("frames each slice's planning-dial rung with its
//! reason", `product-management-pass.md`):
//! - P0, mini-requirements and pointers;
//! - P1, an authored spec with a failing-test-first proof contract and a
//!   declared territory (the default OpenRig recommends; Factory never supplies
//!   it on a plan nobody set, as the execution view floors an absent
//!   `approved-spec-dial` to INDETERMINATE);
//! - P2, plus a research round, run before the spec freezes;
//! - P3, plus an adversarial pass by a non-author, its amendments landing as
//!   proof-contract items;
//! - P4, plus a blind design: similar proceeds, significantly different stops
//!   for a design session with the owner.
//!
//! A plan locks only once its rung's rounds have returned, and a lock is an
//! additive stamp (`wave-sdlc.md`: "approval appends metadata and never
//! rewrites authored bytes"): a locked plan takes no new rung and no further
//! round. Rounds run before the lock, never during the build.
//!
//! The wave SDLC (`wave-sdlc.md`). Waves are composition only, never status
//! labels (EC-2: "status derives at read time"). A wave's units build in
//! parallel in disjoint file territories ("Territory overlap = serialize"); a
//! candidate is admitted only for a locked plan, inside its territory, after
//! every earlier wave sealed, and after its serialized predecessor merged.
//! Whether a candidate merged is read from the integration repository at read
//! time (the ladder's `folded`, `gitAncestor(candidate, main)`), never
//! recorded; an unreadable repository reads as unknown, never as merged or
//! unmerged (the INDETERMINATE floor). An unmerged candidate cut before the
//! tip moved is a rebase trigger. Review fires once the wave's build
//! completes: two legs, structure and claims, by two reviewers, neither a
//! writer of the wave, each at the integration tip. Findings name their cause,
//! context-gap or judgment-gap, and drift is a finding. A wave seals when both
//! legs' latest verdicts are clear at one revision that holds every unit's
//! latest candidate; a fix round's merges are re-earned by both legs at the
//! round's final revision. A sealed wave takes no further candidate, and the
//! next wave opens.
//!
//! Who acts is never claimed in a request. A body occupying a World Position
//! acts as that Position (`OI_POSITION_REF`, `OI_OCCUPANT_GENERATION`, which
//! Actuation must call current); the person acts from the desk as a
//! desktop-human caller presenting the grant the host resolved, as a prompt
//! pad's write does.

use crate::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use crate::build::FACTORY_NATIVE_OWNER;
use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::project_development_store::transact_developmental_state;
use crate::work_custody::{state_refusal, CustodyActor, Refusal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FACTORY_WAVE_PLAN: &str = "factory.wave-plan/v1";
pub const FACTORY_WAVE_REQUEST: &str = "factory.wave-request/v1";
pub const FACTORY_WAVE_RECEIPT: &str = "factory.wave-receipt/v1";
pub const FACTORY_WAVE_READING: &str = "factory.wave-reading/v1";
/// The capability the person's caller presents to plan and compose.
pub const PLAN_CAPABILITY_REF: &str = "capability/factory/plan-waves";

pub const ACT_USAGE: &str = "factory development wave act [<state>] <request-file|-> [--json]   (a factory.wave-request/v1 document: act dial|round|lock|compose|candidate|review)";
pub const READ_USAGE: &str = "factory development wave read [<state>] --run <run-ref> [--json]";

/// OpenRig's planning rungs (`planning-dial.md` "The rungs").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PlanningRung {
    P0,
    P1,
    P2,
    P3,
    P4,
}

impl PlanningRung {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
        }
    }

    /// What the rung asks of the plan, in a person's words.
    pub fn asks(self) -> &'static str {
        match self {
            Self::P0 => "Mini-requirements and pointers: the builder works out the how.",
            Self::P1 => "An authored spec: intent, mini-requirements, a failing-test-first proof contract and a declared territory.",
            Self::P2 => "The spec, plus a research round that returns before the spec freezes.",
            Self::P3 => "The spec and its research, plus an adversarial pass by someone who did not write the plan.",
            Self::P4 => "The spec, research and adversarial pass, plus a blind design compared with the priors.",
        }
    }

    /// The rounds the rung calls for, in the order they run.
    fn calls_for(self) -> &'static [RoundKind] {
        match self {
            Self::P0 | Self::P1 => &[],
            Self::P2 => &[RoundKind::Research],
            Self::P3 => &[RoundKind::Research, RoundKind::Adversarial],
            Self::P4 => &[
                RoundKind::Research,
                RoundKind::Adversarial,
                RoundKind::BlindDesign,
            ],
        }
    }

    /// P1 and above declare a territory before the plan locks.
    fn needs_territory(self) -> bool {
        self >= Self::P1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoundKind {
    Research,
    Adversarial,
    BlindDesign,
    /// The owner's design session after a blind design that differs.
    DesignSession,
}

impl RoundKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Adversarial => "adversarial",
            Self::BlindDesign => "blind-design",
            Self::DesignSession => "design-session",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlindVerdict {
    Similar,
    Different,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActorKind {
    Position,
    Person,
}

/// Who acted: a Position (with its occupant generation) or the person (the
/// grant the host resolved, and the caller that presented it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaveActor {
    pub kind: ActorKind,
    pub actor_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningRound {
    pub round: RoundKind,
    pub evidence_ref: String,
    pub by: WaveActor,
    pub at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<BlindVerdict>,
    /// Proof-contract items an adversarial pass added ("amendments land with
    /// proof-contract teeth — a new or changed checkbox").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub amendments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanLock {
    pub rung: PlanningRung,
    pub by: WaveActor,
    pub at_unix_ms: i64,
}

/// One unit's plan: its rung, why, its territory, its rounds and its lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitPlan {
    pub workflow_unit_ref: WorkflowUnitRef,
    /// The unit's key in its workflow (`inspect-source`), as Factory compiled
    /// it, so a person reads the unit by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub rung: PlanningRung,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub territory: Vec<String>,
    pub set_by: WaveActor,
    pub set_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<PlanningRound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<PlanLock>,
}

/// Where a Run's candidates integrate: a repository and the ref they merge onto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integration {
    pub repo: PathBuf,
    pub onto: String,
}

/// A wave's composition, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wave {
    pub wave: String,
    pub units: Vec<WorkflowUnitRef>,
    /// The order of units whose territories overlap: each builds after the
    /// one before it merged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serialized: Vec<WorkflowUnitRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_model: Option<String>,
    pub composed_by: WaveActor,
    pub composed_at_unix_ms: i64,
}

/// A frozen candidate: the commit a builder froze for a unit, and the files
/// it changes against the integration ref, as Factory read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub workflow_unit_ref: WorkflowUnitRef,
    pub revision: String,
    pub changed: Vec<String>,
    pub by: WaveActor,
    pub at_unix_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewLegKind {
    /// Does the structure hold.
    Structure,
    /// Does each claim survive contact with the source.
    Claims,
}

impl ReviewLegKind {
    pub const ALL: [ReviewLegKind; 2] = [Self::Structure, Self::Claims];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Claims => "claims",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Structure => Self::Claims,
            Self::Claims => Self::Structure,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Clear,
    Findings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingCause {
    /// The spec or its routes lacked what the moment needed: a planning finding.
    ContextGap,
    /// The context was there and the call was wrong: a builder finding.
    JudgmentGap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub cause: FindingCause,
    pub text: String,
    /// The built thing no longer matches what the brief asked for.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewLeg {
    pub wave: String,
    pub leg: ReviewLegKind,
    /// The integration tip the leg reviewed, as Factory read it.
    pub revision: String,
    pub verdict: ReviewVerdict,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    pub by: WaveActor,
    pub at_unix_ms: i64,
}

/// `factory.wave-plan/v1`: one Run's plan of record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryWavePlan {
    pub schema: String,
    pub run_ref: RunRef,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<UnitPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waves: Vec<Wave>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<ReviewLeg>,
}

impl UnitPlan {
    /// The unit by its key where Factory knows it, else by its ref.
    fn named(&self) -> String {
        self.key
            .clone()
            .unwrap_or_else(|| self.workflow_unit_ref.to_string())
    }
}

impl FactoryWavePlan {
    fn new(run_ref: RunRef) -> Self {
        Self {
            schema: FACTORY_WAVE_PLAN.into(),
            run_ref,
            revision: 0,
            integration: None,
            units: Vec::new(),
            waves: Vec::new(),
            candidates: Vec::new(),
            reviews: Vec::new(),
        }
    }

    fn unit(&self, unit: &WorkflowUnitRef) -> Option<&UnitPlan> {
        self.units
            .iter()
            .find(|plan| &plan.workflow_unit_ref == unit)
    }

    fn wave_of(&self, unit: &WorkflowUnitRef) -> Option<(usize, &Wave)> {
        self.waves
            .iter()
            .enumerate()
            .find(|(_, wave)| wave.units.contains(unit))
    }

    fn latest_candidate(&self, unit: &WorkflowUnitRef) -> Option<&Candidate> {
        self.candidates
            .iter()
            .rev()
            .find(|candidate| &candidate.workflow_unit_ref == unit)
    }

    fn latest_leg(&self, wave: &str, leg: ReviewLegKind) -> Option<&ReviewLeg> {
        self.reviews
            .iter()
            .rev()
            .find(|review| review.wave == wave && review.leg == leg)
    }

    /// Everyone who froze a candidate for a unit of the wave.
    fn writers(&self, wave: &Wave) -> BTreeSet<String> {
        self.candidates
            .iter()
            .filter(|candidate| wave.units.contains(&candidate.workflow_unit_ref))
            .map(|candidate| candidate.by.actor_ref.clone())
            .collect()
    }
}

/// Structural invariants, checked whenever the developmental state is read
/// or written. Referential checks happen when an act is admitted.
pub fn validate_wave_plans(plans: &[FactoryWavePlan]) -> Result<(), String> {
    let mut runs = BTreeSet::new();
    for plan in plans {
        if plan.schema != FACTORY_WAVE_PLAN {
            return Err(format!(
                "wave plan schema {} is not {FACTORY_WAVE_PLAN}",
                plan.schema
            ));
        }
        if !runs.insert(plan.run_ref.to_string()) {
            return Err(format!("two wave plans for {}", plan.run_ref));
        }
        let mut units = BTreeSet::new();
        for unit in &plan.units {
            if !units.insert(unit.workflow_unit_ref.to_string()) {
                return Err(format!("two plans for {}", unit.workflow_unit_ref));
            }
            if unit.reason.trim().is_empty() {
                return Err(format!(
                    "the plan of {} has no reason",
                    unit.workflow_unit_ref
                ));
            }
        }
        let mut waves = BTreeSet::new();
        let mut placed = BTreeSet::new();
        for wave in &plan.waves {
            if !waves.insert(wave.wave.clone()) {
                return Err(format!("two waves named {}", wave.wave));
            }
            for unit in &wave.units {
                if !placed.insert(unit.to_string()) {
                    return Err(format!("{unit} stands in two waves"));
                }
            }
        }
    }
    Ok(())
}

/// What a request asks, one act per document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "act", rename_all = "kebab-case", deny_unknown_fields)]
pub enum WaveAct {
    Dial {
        run_ref: RunRef,
        workflow_unit_ref: WorkflowUnitRef,
        rung: PlanningRung,
        reason: String,
        #[serde(default)]
        territory: Option<Vec<String>>,
    },
    Round {
        run_ref: RunRef,
        workflow_unit_ref: WorkflowUnitRef,
        round: RoundKind,
        evidence_ref: String,
        #[serde(default)]
        verdict: Option<BlindVerdict>,
        #[serde(default)]
        amendments: Vec<String>,
    },
    Lock {
        run_ref: RunRef,
        workflow_unit_ref: WorkflowUnitRef,
    },
    Compose {
        run_ref: RunRef,
        wave: String,
        units: Vec<WorkflowUnitRef>,
        #[serde(default)]
        serialized: Vec<WorkflowUnitRef>,
        #[serde(default)]
        review_model: Option<String>,
        #[serde(default)]
        integration: Option<Integration>,
    },
    Candidate {
        run_ref: RunRef,
        workflow_unit_ref: WorkflowUnitRef,
        revision: String,
    },
    Review {
        run_ref: RunRef,
        wave: String,
        leg: ReviewLegKind,
        verdict: ReviewVerdict,
        #[serde(default)]
        findings: Vec<Finding>,
    },
}

impl WaveAct {
    fn name(&self) -> &'static str {
        match self {
            Self::Dial { .. } => "dial",
            Self::Round { .. } => "round",
            Self::Lock { .. } => "lock",
            Self::Compose { .. } => "compose",
            Self::Candidate { .. } => "candidate",
            Self::Review { .. } => "review",
        }
    }

    fn run_ref(&self) -> &RunRef {
        match self {
            Self::Dial { run_ref, .. }
            | Self::Round { run_ref, .. }
            | Self::Lock { run_ref, .. }
            | Self::Compose { run_ref, .. }
            | Self::Candidate { run_ref, .. }
            | Self::Review { run_ref, .. } => run_ref,
        }
    }
}

/// A parsed request: the act, and the person's caller and grant if it names one.
#[derive(Debug, Clone)]
pub struct WaveRequest {
    pub act: WaveAct,
    pub caller: Option<FactoryActionCaller>,
    pub authority: Option<ProjectedFactoryActionAuthority>,
}

fn request_refusal(fact: impl Into<String>) -> Refusal {
    Refusal::new(
        "factory.wave.request",
        fact,
        "The request was not admitted; nothing was read or persisted.",
        format!("send one factory.wave-request/v1 document: {ACT_USAGE}"),
    )
}

pub fn parse_request(text: &str) -> Result<WaveRequest, Refusal> {
    let mut value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| request_refusal(format!("the request is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| request_refusal("the request is not a JSON object"))?;
    match object.remove("schema") {
        Some(serde_json::Value::String(schema)) if schema == FACTORY_WAVE_REQUEST => {}
        other => {
            return Err(request_refusal(format!(
                "the request's schema is {}, not {FACTORY_WAVE_REQUEST}",
                other
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "absent".into())
            )))
        }
    }
    let caller = object
        .remove("caller")
        .map(serde_json::from_value::<FactoryActionCaller>)
        .transpose()
        .map_err(|error| request_refusal(format!("the caller is not a Factory caller: {error}")))?;
    let authority = object
        .remove("authority")
        .map(serde_json::from_value::<ProjectedFactoryActionAuthority>)
        .transpose()
        .map_err(|error| {
            request_refusal(format!("the authority is not a Factory authority: {error}"))
        })?;
    let act = serde_json::from_value::<WaveAct>(value)
        .map_err(|error| request_refusal(format!("the act is not admitted: {error}")))?;
    Ok(WaveRequest {
        act,
        caller,
        authority,
    })
}

/// Who is acting: the occupancy stamped into this process, or the person's
/// caller and grant in the request. Never both, never neither.
pub fn resolve_actor(
    request: &WaveRequest,
    occupancy: Option<CustodyActor>,
) -> Result<WaveActor, Refusal> {
    match (occupancy, &request.caller) {
        (Some(_), Some(_)) => Err(Refusal::unchanged(
            "factory.wave.two_actors",
            "the request names a desktop caller while this process is a body occupying a Position",
            "send the person's request from the desk, or the Position's from its body, not both",
        )),
        (Some(actor), None) => {
            let generation = actor
                .generation_ref
                .clone()
                .filter(|generation| !generation.trim().is_empty())
                .ok_or_else(|| {
                    Refusal::unchanged(
                        "factory.wave.generation_required",
                        format!(
                            "this body names {} but carries no occupant generation",
                            actor.position_ref
                        ),
                        "inhabit so OI_OCCUPANT_GENERATION is set, then retry",
                    )
                })?;
            Ok(WaveActor {
                kind: ActorKind::Position,
                actor_ref: actor.position_ref,
                generation_ref: Some(generation),
                caller_ref: None,
            })
        }
        (None, Some(caller)) => person_actor(caller, request.authority.as_ref()),
        (None, None) => Err(Refusal::unchanged(
            "factory.wave.actor_required",
            "the act names no actor: this process occupies no Position and the request carries no desktop caller",
            "act from a body occupying a Position (aikit inhabit), or from the desk as the person",
        )),
    }
}

fn person_actor(
    caller: &FactoryActionCaller,
    authority: Option<&ProjectedFactoryActionAuthority>,
) -> Result<WaveActor, Refusal> {
    let refuse = |fact: String| {
        Refusal::unchanged(
            "factory.wave.not_authorised",
            fact,
            format!("present a desktop-human caller and the granted {PLAN_CAPABILITY_REF}, as the host resolves it"),
        )
    };
    if caller.projection_kind != FactoryActionProjectionKind::DesktopHuman {
        return Err(refuse(format!(
            "the caller {} is not a desktop-human",
            caller.caller_ref
        )));
    }
    if caller.caller_ref.trim().is_empty()
        || caller.lineage.last().map(String::as_str) != Some(caller.caller_ref.as_str())
    {
        return Err(refuse("the caller's lineage must end at the caller".into()));
    }
    let authority = authority.ok_or_else(|| refuse("the request names no authority".into()))?;
    if authority.native_owner != FACTORY_NATIVE_OWNER
        || authority.capability_ref.as_deref() != Some(PLAN_CAPABILITY_REF)
        || !authority.capability_granted
        || !authority.action_authorised
        || authority.authority_ref.trim().is_empty()
    {
        return Err(refuse(format!(
            "the authority must name {FACTORY_NATIVE_OWNER} as its owner, grant {PLAN_CAPABILITY_REF}, authorise the act and name its grant"
        )));
    }
    Ok(WaveActor {
        kind: ActorKind::Person,
        actor_ref: authority.authority_ref.clone(),
        generation_ref: None,
        caller_ref: Some(caller.caller_ref.clone()),
    })
}

/// The facts a plan reads from its integration repository. Production reads
/// git; tests may answer from a table.
pub trait IntegrationFacts {
    /// The full commit the integration ref points at.
    fn tip(&self) -> Result<String, String>;
    /// The full commit a revision names.
    fn resolve(&self, revision: &str) -> Result<String, String>;
    /// Whether `ancestor` is contained in `descendant`.
    fn contains(&self, ancestor: &str, descendant: &str) -> Result<bool, String>;
    /// The files `revision` changes since it left the integration ref.
    fn changed(&self, revision: &str) -> Result<Vec<String>, String>;
}

/// `git -C <repo>` over the plan's integration.
pub struct GitIntegration<'a>(pub &'a Integration);

impl GitIntegration<'_> {
    fn git(&self, args: &[&str]) -> Result<std::process::Output, String> {
        Command::new("git")
            .arg("-C")
            .arg(&self.0.repo)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| format!("git could not run in {}: {error}", self.0.repo.display()))
    }

    fn line(&self, args: &[&str]) -> Result<String, String> {
        let output = self.git(args)?;
        if !output.status.success() {
            return Err(format!(
                "git {} in {} failed: {}",
                args.join(" "),
                self.0.repo.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

impl IntegrationFacts for GitIntegration<'_> {
    fn tip(&self) -> Result<String, String> {
        self.resolve(&self.0.onto)
    }

    fn resolve(&self, revision: &str) -> Result<String, String> {
        if revision.starts_with('-') || revision.trim().is_empty() {
            return Err(format!("{revision:?} is not a revision"));
        }
        self.line(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{revision}^{{commit}}"),
        ])
    }

    fn contains(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
        let output = self.git(&["merge-base", "--is-ancestor", ancestor, descendant])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(format!(
                "git merge-base --is-ancestor {ancestor} {descendant} in {} failed: {}",
                self.0.repo.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }

    fn changed(&self, revision: &str) -> Result<Vec<String>, String> {
        let base = self.line(&["merge-base", &self.0.onto, revision])?;
        Ok(self
            .line(&["diff", "--name-only", "--no-renames", &base, revision])?
            .lines()
            .map(str::to_owned)
            .filter(|line| !line.is_empty())
            .collect())
    }
}

/// A territory entry, without a trailing slash.
fn territory_entry(entry: &str) -> &str {
    entry.trim_end_matches('/')
}

fn path_within(path: &str, entry: &str) -> bool {
    let entry = territory_entry(entry);
    path == entry
        || path
            .strip_prefix(entry)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Whether two declared territories share any path.
fn territories_overlap(left: &[String], right: &[String]) -> bool {
    left.iter().any(|a| {
        right
            .iter()
            .any(|b| path_within(territory_entry(a), b) || path_within(territory_entry(b), a))
    })
}

fn validate_territory(territory: &[String]) -> Result<Vec<String>, Refusal> {
    let mut entries = Vec::new();
    for entry in territory {
        let trimmed = entry.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('/')
            || trimmed.split('/').any(|part| part == ".." || part == ".")
            || trimmed.contains(['\n', '\r'])
        {
            return Err(Refusal::unchanged(
                "factory.plan.invalid_territory",
                format!("{entry:?} is not a path inside the repository"),
                "declare the territory as repository-relative paths or directories, such as app/ or kernel/b.rs",
            ));
        }
        if !entries.iter().any(|known: &String| known == trimmed) {
            entries.push(trimmed.to_owned());
        }
    }
    Ok(entries)
}

/// The rounds a plan still needs before it locks, in the order they run.
fn needs(plan: &UnitPlan) -> Vec<RoundKind> {
    let has = |kind: RoundKind| plan.rounds.iter().any(|round| round.round == kind);
    let mut missing: Vec<RoundKind> = plan
        .rung
        .calls_for()
        .iter()
        .copied()
        .filter(|kind| !has(*kind))
        .collect();
    // A blind design that differs stops for the owner's design session after it.
    if let Some(index) = plan.rounds.iter().rposition(|round| {
        round.round == RoundKind::BlindDesign && round.verdict == Some(BlindVerdict::Different)
    }) {
        if !plan.rounds[index..]
            .iter()
            .any(|round| round.round == RoundKind::DesignSession)
        {
            missing.push(RoundKind::DesignSession);
        }
    }
    missing
}

fn words(kinds: &[RoundKind]) -> String {
    kinds
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn short(revision: &str) -> &str {
    revision.get(..9).unwrap_or(revision)
}

/// A unit by its key where Factory knows it, else by its ref.
fn name_of(plan: &FactoryWavePlan, unit: &WorkflowUnitRef) -> String {
    plan.unit(unit)
        .map(UnitPlan::named)
        .unwrap_or_else(|| unit.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveReceipt {
    pub schema: String,
    pub act: String,
    /// `applied` or `unchanged` (the same act again).
    pub result: String,
    pub run_ref: RunRef,
    pub plan_revision: u64,
}

/// Admit one act against the state, through the developmental lock.
pub fn act(path: &Path, request: WaveRequest, actor: WaveActor) -> Result<WaveReceipt, Refusal> {
    let now = now_unix_ms();
    transact_developmental_state(path, |state| Ok(act_in(state, request, actor, now)))
        .map_err(|error| state_refusal(path, error))?
}

/// The act, in memory, over git's facts for the plan's integration.
pub fn act_in(
    state: &mut FactoryDevelopmentalState,
    request: WaveRequest,
    actor: WaveActor,
    now: i64,
) -> Result<WaveReceipt, Refusal> {
    let run_ref = request.act.run_ref().clone();
    if state.build.run(&run_ref).is_none() {
        return Err(Refusal::unchanged(
            "factory.wave.run_not_found",
            format!("{run_ref} is not a Run of this project"),
            "name a Run this developmental state holds (factory development project <state> <project-ref>)",
        ));
    }
    let name = request.act.name();
    // Nothing touches the state until the act is admitted: a refusal returns
    // before the plan is put back, so the transaction publishes nothing.
    let index = state
        .wave_plans
        .iter()
        .position(|plan| plan.run_ref == run_ref);
    let before = index
        .map(|index| state.wave_plans[index].clone())
        .unwrap_or_else(|| FactoryWavePlan::new(run_ref.clone()));
    let mut keys: Vec<(WorkflowUnitRef, String)> = Vec::new();
    let mut units_known = |unit: &WorkflowUnitRef| {
        state
            .workflow_unit_reading(unit, Some(&run_ref))
            .map(|reading| keys.push((unit.clone(), reading.key)))
            .map_err(|_| {
                Refusal::unchanged(
                    "factory.wave.unit_not_found",
                    format!("{unit} is not one of this project's workflow units"),
                    format!(
                        "name a unit Factory lists (factory development workflow-units <state> {run_ref})"
                    ),
                )
            })
    };
    match &request.act {
        WaveAct::Dial {
            workflow_unit_ref, ..
        }
        | WaveAct::Round {
            workflow_unit_ref, ..
        }
        | WaveAct::Lock {
            workflow_unit_ref, ..
        }
        | WaveAct::Candidate {
            workflow_unit_ref, ..
        } => units_known(workflow_unit_ref)?,
        WaveAct::Compose { units, .. } => {
            for unit in units {
                units_known(unit)?;
            }
        }
        WaveAct::Review { .. } => {}
    }
    let mut plan = before.clone();
    let applied = {
        let integration = plan.integration.clone();
        let facts = integration.as_ref().map(GitIntegration);
        apply(
            &mut plan,
            request.act,
            actor,
            now,
            facts.as_ref().map(|facts| facts as &dyn IntegrationFacts),
        )?
    };
    for unit in plan.units.iter_mut().filter(|unit| unit.key.is_none()) {
        if let Some((_, key)) = keys
            .iter()
            .find(|(known, _)| known == &unit.workflow_unit_ref)
        {
            unit.key = Some(key.clone());
        }
    }
    if applied && plan != before {
        plan.revision += 1;
        match index {
            Some(index) => state.wave_plans[index] = plan.clone(),
            None => state.wave_plans.push(plan.clone()),
        }
    }
    let revision = if applied {
        plan.revision
    } else {
        before.revision
    };
    Ok(WaveReceipt {
        schema: FACTORY_WAVE_RECEIPT.into(),
        act: name.into(),
        result: if applied { "applied" } else { "unchanged" }.into(),
        run_ref,
        plan_revision: revision,
    })
}

fn no_plan(unit: &WorkflowUnitRef) -> Refusal {
    Refusal::unchanged(
        "factory.plan.not_set",
        format!("{unit} has no plan: no rung was set for it"),
        format!("set its planning dial first: act dial with workflow_unit_ref {unit}, a rung and its reason"),
    )
}

fn locked_refusal(unit: &UnitPlan) -> Refusal {
    let lock = unit.lock.as_ref().expect("a locked plan");
    Refusal::unchanged(
        "factory.plan.locked",
        format!(
            "the plan of {} locked at {} by {}: a locked plan takes no new rung and no further round",
            unit.named(),
            lock.rung.as_str(),
            lock.by.actor_ref
        ),
        "plan the change as new work in a later wave; a lock is never rewritten",
    )
}

/// Apply one act to the plan. `Ok(false)` is the same act again.
fn apply(
    plan: &mut FactoryWavePlan,
    act: WaveAct,
    actor: WaveActor,
    now: i64,
    facts: Option<&dyn IntegrationFacts>,
) -> Result<bool, Refusal> {
    match act {
        WaveAct::Dial {
            workflow_unit_ref,
            rung,
            reason,
            territory,
            ..
        } => {
            if reason.trim().is_empty() {
                return Err(Refusal::unchanged(
                    "factory.plan.reason_required",
                    format!("a rung for {workflow_unit_ref} needs its reason"),
                    "say why the piece earns this rung: an unresolved external unknown earns P2, a failure its author cannot see earns P3",
                ));
            }
            let territory = territory
                .map(|entries| validate_territory(&entries))
                .transpose()?;
            let existing = plan
                .units
                .iter_mut()
                .find(|unit| unit.workflow_unit_ref == workflow_unit_ref);
            match existing {
                Some(unit) if unit.lock.is_some() => Err(locked_refusal(unit)),
                Some(unit) => {
                    let territory = territory.unwrap_or_else(|| unit.territory.clone());
                    if unit.rung == rung && unit.reason == reason && unit.territory == territory {
                        return Ok(false);
                    }
                    unit.rung = rung;
                    unit.reason = reason;
                    unit.territory = territory;
                    unit.set_by = actor;
                    unit.set_at_unix_ms = now;
                    Ok(true)
                }
                None => {
                    plan.units.push(UnitPlan {
                        workflow_unit_ref,
                        key: None,
                        rung,
                        reason,
                        territory: territory.unwrap_or_default(),
                        set_by: actor,
                        set_at_unix_ms: now,
                        rounds: Vec::new(),
                        lock: None,
                    });
                    Ok(true)
                }
            }
        }
        WaveAct::Round {
            workflow_unit_ref,
            round,
            evidence_ref,
            verdict,
            amendments,
            ..
        } => {
            let unit = plan
                .units
                .iter_mut()
                .find(|unit| unit.workflow_unit_ref == workflow_unit_ref)
                .ok_or_else(|| no_plan(&workflow_unit_ref))?;
            if unit.lock.is_some() {
                return Err(locked_refusal(unit));
            }
            if evidence_ref.trim().is_empty() {
                return Err(Refusal::unchanged(
                    "factory.plan.evidence_required",
                    format!("the {} round names no evidence", round.as_str()),
                    "name what the round returned: its citations, its verdict artifact or its design",
                ));
            }
            let called = unit.rung.calls_for().contains(&round)
                || (round == RoundKind::DesignSession
                    && needs(unit).contains(&RoundKind::DesignSession));
            if !called {
                return Err(Refusal::unchanged(
                    "factory.plan.round_not_called",
                    format!(
                        "{} at {} calls for {}; it does not call for a {} round",
                        unit.named(),
                        unit.rung.as_str(),
                        if unit.rung.calls_for().is_empty() {
                            "no round".to_owned()
                        } else {
                            words(unit.rung.calls_for())
                        },
                        round.as_str()
                    ),
                    "dial the plan up to the rung that calls for it, with its reason, or record the round its rung calls for",
                ));
            }
            match (round, verdict) {
                (RoundKind::BlindDesign, None) => {
                    return Err(Refusal::unchanged(
                        "factory.plan.verdict_required",
                        "a blind design returns its comparison with the priors",
                        "record its verdict: similar to proceed, different to stop for a design session with the owner",
                    ))
                }
                (RoundKind::BlindDesign, Some(_)) => {}
                (_, Some(_)) => {
                    return Err(Refusal::unchanged(
                        "factory.plan.verdict_not_admitted",
                        format!("a {} round carries no blind-design verdict", round.as_str()),
                        "record the verdict on the blind-design round only",
                    ))
                }
                _ => {}
            }
            if round == RoundKind::Adversarial && actor.actor_ref == unit.set_by.actor_ref {
                return Err(Refusal::unchanged(
                    "factory.plan.self_review",
                    format!(
                        "{} set this plan's rung, so its adversarial pass is not theirs to run",
                        actor.actor_ref
                    ),
                    "ask someone who did not write the plan, and who holds the product's context, to attack it",
                ));
            }
            let amendments: Vec<String> = amendments
                .into_iter()
                .map(|item| item.trim().to_owned())
                .filter(|item| !item.is_empty())
                .collect();
            if round != RoundKind::Adversarial && !amendments.is_empty() {
                return Err(Refusal::unchanged(
                    "factory.plan.amendments_not_admitted",
                    format!("a {} round adds no proof-contract items", round.as_str()),
                    "record amendments on the adversarial pass that found them",
                ));
            }
            if unit.rounds.iter().any(|known| {
                known.round == round
                    && known.evidence_ref == evidence_ref
                    && known.verdict == verdict
            }) {
                return Ok(false);
            }
            unit.rounds.push(PlanningRound {
                round,
                evidence_ref,
                by: actor,
                at_unix_ms: now,
                verdict,
                amendments,
            });
            Ok(true)
        }
        WaveAct::Lock {
            workflow_unit_ref, ..
        } => {
            let unit = plan
                .units
                .iter_mut()
                .find(|unit| unit.workflow_unit_ref == workflow_unit_ref)
                .ok_or_else(|| no_plan(&workflow_unit_ref))?;
            if let Some(lock) = &unit.lock {
                return if lock.rung == unit.rung {
                    Ok(false)
                } else {
                    Err(locked_refusal(unit))
                };
            }
            let missing = needs(unit);
            if !missing.is_empty() {
                return Err(Refusal::unchanged(
                    "factory.plan.rounds_missing",
                    format!(
                        "the plan of {} stands at {} and still needs: {}",
                        unit.named(),
                        unit.rung.as_str(),
                        words(&missing)
                    ),
                    format!(
                        "run the {} round before the spec freezes, record it, then lock",
                        missing[0].as_str()
                    ),
                ));
            }
            if unit.rung.needs_territory() && unit.territory.is_empty() {
                return Err(Refusal::unchanged(
                    "factory.plan.territory_missing",
                    format!(
                        "{} at {} declares no territory",
                        unit.named(),
                        unit.rung.as_str()
                    ),
                    "declare the files the unit may change (act dial with its territory), then lock",
                ));
            }
            unit.lock = Some(PlanLock {
                rung: unit.rung,
                by: actor,
                at_unix_ms: now,
            });
            Ok(true)
        }
        WaveAct::Compose {
            wave,
            units,
            serialized,
            review_model,
            integration,
            ..
        } => compose(
            plan,
            ComposeAct {
                wave,
                units,
                serialized,
                review_model,
                integration,
            },
            actor,
            now,
        ),
        WaveAct::Candidate {
            workflow_unit_ref,
            revision,
            ..
        } => candidate(plan, workflow_unit_ref, &revision, actor, now, facts),
        WaveAct::Review {
            wave,
            leg,
            verdict,
            findings,
            ..
        } => review(plan, &wave, leg, verdict, findings, actor, now, facts),
    }
}

struct ComposeAct {
    wave: String,
    units: Vec<WorkflowUnitRef>,
    serialized: Vec<WorkflowUnitRef>,
    review_model: Option<String>,
    integration: Option<Integration>,
}

fn compose(
    plan: &mut FactoryWavePlan,
    act: ComposeAct,
    actor: WaveActor,
    now: i64,
) -> Result<bool, Refusal> {
    let id = act.wave.trim().to_owned();
    if id.is_empty() || id.contains(char::is_whitespace) || act.units.is_empty() {
        return Err(Refusal::unchanged(
            "factory.wave.invalid_composition",
            "a wave needs a name without spaces and at least one unit",
            "compose the wave with its name and its units",
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some(twice) = act.units.iter().find(|unit| !seen.insert(unit.to_string())) {
        return Err(Refusal::unchanged(
            "factory.wave.invalid_composition",
            format!("{twice} is named twice in wave {id}"),
            "name each unit once",
        ));
    }
    if let Some(stray) = act.serialized.iter().find(|unit| !act.units.contains(unit)) {
        return Err(Refusal::unchanged(
            "factory.wave.invalid_composition",
            format!("{stray} is serialized in wave {id} but is not one of its units"),
            "serialize only units of the wave",
        ));
    }
    match (&plan.integration, act.integration) {
        (None, None) => {
            return Err(Refusal::unchanged(
                "factory.wave.integration_required",
                "this Run's plan names no integration: where its candidates merge",
                "compose the first wave with integration {repo, onto}: the repository and the ref the integrator merges onto",
            ))
        }
        (None, Some(integration)) => {
            if !integration.repo.is_absolute() || integration.onto.trim().is_empty() {
                return Err(Refusal::unchanged(
                    "factory.wave.integration_required",
                    "the integration needs an absolute repository path and the ref it merges onto",
                    "name the repository by its absolute path and the ref, such as main",
                ));
            }
            plan.integration = Some(integration);
        }
        (Some(known), Some(integration)) if *known != integration => {
            return Err(Refusal::unchanged(
                "factory.wave.integration_fixed",
                format!(
                    "this Run integrates onto {} in {}",
                    known.onto,
                    known.repo.display()
                ),
                "compose without integration, or with the one the plan already names",
            ))
        }
        _ => {}
    }
    for unit in &act.units {
        let unit_plan = plan.unit(unit).ok_or_else(|| {
            Refusal::unchanged(
                "factory.wave.unplanned_unit",
                format!("{unit} has no plan, so it cannot be composed into a wave"),
                "set its planning dial, with its reason and territory, first",
            )
        })?;
        if unit_plan.territory.is_empty() {
            return Err(Refusal::unchanged(
                "factory.wave.territory_missing",
                format!(
                    "{} declares no territory: a unit built in parallel needs its own files",
                    name_of(plan, unit)
                ),
                "declare its territory with act dial, then compose",
            ));
        }
        if let Some((_, other)) = plan.wave_of(unit).filter(|(_, other)| other.wave != id) {
            return Err(Refusal::unchanged(
                "factory.wave.unit_in_two_waves",
                format!("{} already stands in wave {}", name_of(plan, unit), other.wave),
                "a unit stands in one wave; recompose that wave without it while none of its units has a candidate",
            ));
        }
    }
    for (left_index, left) in act.units.iter().enumerate() {
        for right in act.units.iter().skip(left_index + 1) {
            let (Some(a), Some(b)) = (plan.unit(left), plan.unit(right)) else {
                continue;
            };
            if territories_overlap(&a.territory, &b.territory)
                && !(act.serialized.contains(left) && act.serialized.contains(right))
            {
                return Err(Refusal::unchanged(
                    "factory.wave.territory_overlap",
                    format!(
                        "{} and {} share files: {} and {}",
                        name_of(plan, left),
                        name_of(plan, right),
                        a.territory.join(", "),
                        b.territory.join(", ")
                    ),
                    "split their territories, put one in a later wave, or serialize both in this wave",
                ));
            }
        }
    }
    let composed = Wave {
        wave: id.clone(),
        units: act.units,
        serialized: act.serialized,
        review_model: act
            .review_model
            .map(|model| model.trim().to_owned())
            .filter(|model| !model.is_empty()),
        composed_by: actor,
        composed_at_unix_ms: now,
    };
    if let Some(existing) = plan.waves.iter_mut().find(|wave| wave.wave == id) {
        if existing.units == composed.units
            && existing.serialized == composed.serialized
            && existing.review_model == composed.review_model
        {
            return Ok(false);
        }
        if plan
            .candidates
            .iter()
            .any(|candidate| existing.units.contains(&candidate.workflow_unit_ref))
        {
            return Err(Refusal::unchanged(
                "factory.wave.composition_frozen",
                format!("wave {id} has begun building, so its composition stands"),
                "a conflict found mid-build stops and is ruled: compose the new work into a later wave",
            ));
        }
        *existing = composed;
    } else {
        plan.waves.push(composed);
    }
    Ok(true)
}

fn integration_facts<'a>(
    plan: &FactoryWavePlan,
    facts: Option<&'a dyn IntegrationFacts>,
) -> Result<&'a dyn IntegrationFacts, Refusal> {
    facts.ok_or_else(|| {
        Refusal::unchanged(
            "factory.wave.integration_required",
            format!(
                "the plan of {} names no integration repository",
                plan.run_ref
            ),
            "compose the first wave with its integration {repo, onto}",
        )
    })
}

fn unreadable(fact: String) -> Refusal {
    Refusal::unchanged(
        "factory.wave.integration_unreadable",
        fact,
        "make the integration repository readable to Factory, then retry",
    )
}

fn candidate(
    plan: &mut FactoryWavePlan,
    unit: WorkflowUnitRef,
    revision: &str,
    actor: WaveActor,
    now: i64,
    facts: Option<&dyn IntegrationFacts>,
) -> Result<bool, Refusal> {
    let unit_name = name_of(plan, &unit);
    let (position, wave) = plan
        .wave_of(&unit)
        .map(|(index, wave)| (index, wave.clone()))
        .ok_or_else(|| {
            Refusal::unchanged(
                "factory.wave.unit_not_in_wave",
                format!("{unit_name} stands in no wave"),
                "compose it into a wave before freezing a candidate",
            )
        })?;
    let facts = integration_facts(plan, facts)?;
    let standing = derive(plan, facts);
    if standing.waves[position].state == WaveState::Sealed {
        return Err(Refusal::unchanged(
            "factory.wave.sealed",
            format!("wave {} sealed: both legs cleared it", wave.wave),
            "plan the change as new work in a later wave",
        ));
    }
    if let Some(open) = standing.waves[..position]
        .iter()
        .find(|earlier| earlier.state != WaveState::Sealed)
    {
        return Err(Refusal::unchanged(
            "factory.wave.earlier_wave_open",
            format!(
                "wave {} has not sealed ({}), so wave {} has not opened",
                open.wave, open.gate, wave.wave
            ),
            format!("seal wave {} first: {}", open.wave, open.next),
        ));
    }
    let unit_plan = plan.unit(&unit).ok_or_else(|| no_plan(&unit))?.clone();
    if unit_plan.lock.is_none() {
        let missing = needs(&unit_plan);
        return Err(Refusal::unchanged(
            "factory.wave.plan_not_locked",
            format!(
                "the plan of {unit_name} at {} is not locked{}",
                unit_plan.rung.as_str(),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(" and still needs: {}", words(&missing))
                }
            ),
            "lock the plan before building: dispatch follows the lock",
        ));
    }
    if let Some(at) = wave.serialized.iter().position(|item| item == &unit) {
        if let Some(before) = at.checked_sub(1).map(|index| &wave.serialized[index]) {
            let before_name = name_of(plan, before);
            let merged = plan
                .latest_candidate(before)
                .map(|candidate| {
                    facts
                        .tip()
                        .and_then(|tip| facts.contains(&candidate.revision, &tip))
                })
                .transpose()
                .map_err(unreadable)?;
            if merged != Some(true) {
                return Err(Refusal::unchanged(
                    "factory.wave.serialized_after",
                    format!("{unit_name} is serialized after {before_name}, whose candidate has not merged"),
                    format!("merge {before_name}'s candidate, then cut {unit_name}'s from the new tip"),
                ));
            }
        }
    }
    let resolved = facts.resolve(revision).map_err(|error| {
        Refusal::unchanged(
            "factory.wave.unknown_revision",
            format!("{revision} is not a commit in the integration repository: {error}"),
            "freeze the candidate as a commit the integration repository holds, then name it",
        )
    })?;
    if plan
        .latest_candidate(&unit)
        .is_some_and(|known| known.revision == resolved)
    {
        return Ok(false);
    }
    let changed = facts.changed(&resolved).map_err(unreadable)?;
    let outside: Vec<&String> = changed
        .iter()
        .filter(|path| {
            !unit_plan
                .territory
                .iter()
                .any(|entry| path_within(path, entry))
        })
        .collect();
    if !outside.is_empty() {
        return Err(Refusal::unchanged(
            "factory.wave.outside_territory",
            format!(
                "{} changes {} outside {unit_name}'s territory ({})",
                short(&resolved),
                outside
                    .iter()
                    .map(|path| path.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                unit_plan.territory.join(", ")
            ),
            "keep the candidate inside the declared territory; a conflict found mid-build stops and is ruled",
        ));
    }
    plan.candidates.push(Candidate {
        workflow_unit_ref: unit,
        revision: resolved,
        changed,
        by: actor,
        at_unix_ms: now,
    });
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn review(
    plan: &mut FactoryWavePlan,
    wave_id: &str,
    leg: ReviewLegKind,
    verdict: ReviewVerdict,
    findings: Vec<Finding>,
    actor: WaveActor,
    now: i64,
    facts: Option<&dyn IntegrationFacts>,
) -> Result<bool, Refusal> {
    let position = plan
        .waves
        .iter()
        .position(|wave| wave.wave == wave_id)
        .ok_or_else(|| {
            Refusal::unchanged(
                "factory.wave.not_found",
                format!("this Run's plan has no wave {wave_id}"),
                "name a wave the plan composed (factory development wave read <state> --run <run-ref>)",
            )
        })?;
    let wave = plan.waves[position].clone();
    match (verdict, findings.is_empty()) {
        (ReviewVerdict::Clear, false) | (ReviewVerdict::Findings, true) => {
            return Err(Refusal::unchanged(
                "factory.wave.verdict_findings",
                "a clear verdict carries no findings, and a findings verdict carries at least one",
                "send clear with no findings, or findings with each one's cause",
            ))
        }
        _ => {}
    }
    if findings
        .iter()
        .any(|finding| finding.text.trim().is_empty())
    {
        return Err(Refusal::unchanged(
            "factory.wave.verdict_findings",
            "a finding says what it found",
            "give each finding its words and its cause",
        ));
    }
    if plan.writers(&wave).contains(&actor.actor_ref) {
        return Err(Refusal::unchanged(
            "factory.wave.writer_reviews",
            format!(
                "{} froze a candidate in wave {wave_id}, so it does not review it",
                actor.actor_ref
            ),
            "ask a reviewer who wrote none of the wave; without one the wave gate is theatre",
        ));
    }
    let facts = integration_facts(plan, facts)?;
    let standing = derive(plan, facts);
    let current = &standing.waves[position];
    match current.state {
        WaveState::Sealed => {
            return Err(Refusal::unchanged(
                "factory.wave.sealed",
                format!("wave {wave_id} sealed already"),
                "review the next wave when its build completes",
            ))
        }
        WaveState::InReview | WaveState::FixRound => {}
        _ => {
            return Err(Refusal::unchanged(
                "factory.wave.build_incomplete",
                format!(
                    "wave {wave_id} stands {}: {}",
                    current.state.as_str(),
                    current.gate
                ),
                format!(
                    "review fires when the wave's build completes: {}",
                    current.next
                ),
            ))
        }
    }
    let tip = facts.tip().map_err(unreadable)?;
    if plan
        .latest_leg(wave_id, leg.other())
        .is_some_and(|other| other.revision == tip && other.by.actor_ref == actor.actor_ref)
    {
        return Err(Refusal::unchanged(
            "factory.wave.one_reviewer_two_legs",
            format!(
                "{} reviewed the {} leg at {}: the two legs are two reviewers' vantages",
                actor.actor_ref,
                leg.other().as_str(),
                short(&tip)
            ),
            format!(
                "ask another reviewer, who wrote none of the wave, for the {} leg",
                leg.as_str()
            ),
        ));
    }
    if plan.latest_leg(wave_id, leg).is_some_and(|known| {
        known.revision == tip
            && known.verdict == verdict
            && known.findings == findings
            && known.by.actor_ref == actor.actor_ref
    }) {
        return Ok(false);
    }
    plan.reviews.push(ReviewLeg {
        wave: wave_id.to_owned(),
        leg,
        revision: tip,
        verdict,
        findings,
        by: actor,
        at_unix_ms: now,
    });
    Ok(true)
}

/// How a rung of a unit's ladder stands, read at read time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RungValue {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderRung {
    pub value: RungValue,
    pub basis: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

impl LadderRung {
    fn yes(basis: String) -> Self {
        Self {
            value: RungValue::Yes,
            basis,
            revision: None,
        }
    }
    fn no(basis: String) -> Self {
        Self {
            value: RungValue::No,
            basis,
            revision: None,
        }
    }
    fn unknown(basis: String) -> Self {
        Self {
            value: RungValue::Unknown,
            basis,
            revision: None,
        }
    }
}

/// Done-ness is the ladder, never one "done" (execution-view.ts "THE LADDER
/// IS THE SCHEMA"): the plan locked, a candidate built, merged, the wave reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitLadder {
    pub locked: LadderRung,
    pub built: LadderRung,
    pub merged: LadderRung,
    pub reviewed: LadderRung,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialReading {
    pub rung: PlanningRung,
    pub asks: String,
    pub reason: String,
    pub territory: Vec<String>,
    pub set_by: WaveActor,
    /// The rounds the rung still needs before the plan locks.
    pub needs: Vec<RoundKind>,
    pub rounds: Vec<PlanningRound>,
    pub lock: Option<PlanLock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitReading {
    pub workflow_unit_ref: WorkflowUnitRef,
    pub key: Option<String>,
    pub wave: Option<String>,
    pub dial: DialReading,
    pub ladder: UnitLadder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaveState {
    /// An earlier wave has not sealed.
    Waiting,
    /// A unit's plan is not locked.
    Planning,
    /// A unit has no candidate, or a candidate has not merged.
    Building,
    /// Built and merged; the legs have not both cleared one revision.
    InReview,
    /// A leg's latest verdict found something: fixes merge, then both re-earn.
    FixRound,
    /// Both legs clear at one revision that holds every candidate.
    Sealed,
    /// The integration repository could not be read.
    Unread,
}

impl WaveState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Planning => "planning",
            Self::Building => "building",
            Self::InReview => "in-review",
            Self::FixRound => "fix-round",
            Self::Sealed => "sealed",
            Self::Unread => "unread",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingTally {
    pub context_gap: usize,
    pub judgment_gap: usize,
    pub drift: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveStanding {
    pub wave: String,
    pub ordinal: usize,
    pub units: Vec<WorkflowUnitRef>,
    pub serialized: Vec<WorkflowUnitRef>,
    pub review_model: Option<String>,
    pub state: WaveState,
    /// What holds the wave where it stands, in words.
    pub gate: String,
    /// The next lawful step.
    pub next: String,
    pub writers: Vec<String>,
    /// Each leg's latest review.
    pub legs: Vec<ReviewLeg>,
    /// Review rounds: the distinct revisions the wave was reviewed at.
    pub rounds: usize,
    /// Every finding of the wave's reviews, by cause: the calibration the
    /// dispatch shape tunes on.
    pub findings: FindingTally,
    pub sealed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationReading {
    pub repo: PathBuf,
    pub onto: String,
    pub tip: Option<String>,
    pub basis: String,
}

/// `factory.wave-reading/v1`: the Run's plan with every status derived now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveReading {
    pub schema: String,
    pub run_ref: RunRef,
    pub plan_revision: u64,
    pub integration: Option<IntegrationReading>,
    pub units: Vec<UnitReading>,
    pub waves: Vec<WaveStanding>,
}

/// Read a Run's plan from the state, deriving its standing from git now.
pub fn read(state: &FactoryDevelopmentalState, run_ref: &RunRef) -> Result<WaveReading, Refusal> {
    if state.build.run(run_ref).is_none() {
        return Err(Refusal::new(
            "factory.wave.run_not_found",
            format!("{run_ref} is not a Run of this project"),
            crate::work_custody::NOTHING_READ,
            "name a Run this developmental state holds",
        ));
    }
    let plan = state
        .wave_plans
        .iter()
        .find(|plan| &plan.run_ref == run_ref)
        .cloned()
        .unwrap_or_else(|| FactoryWavePlan::new(run_ref.clone()));
    let integration = plan.integration.clone();
    Ok(match integration.as_ref() {
        Some(integration) => derive(&plan, &GitIntegration(integration)),
        None => derive(&plan, &NoIntegration),
    })
}

struct NoIntegration;

impl IntegrationFacts for NoIntegration {
    fn tip(&self) -> Result<String, String> {
        Err("this Run's plan names no integration repository".into())
    }
    fn resolve(&self, _: &str) -> Result<String, String> {
        self.tip()
    }
    fn contains(&self, _: &str, _: &str) -> Result<bool, String> {
        self.tip().map(|_| false)
    }
    fn changed(&self, _: &str) -> Result<Vec<String>, String> {
        self.tip().map(|_| Vec::new())
    }
}

/// The plan's standing, every field derived from the record and git's facts.
pub fn derive(plan: &FactoryWavePlan, facts: &dyn IntegrationFacts) -> WaveReading {
    let tip = facts.tip();
    let onto = plan
        .integration
        .as_ref()
        .map(|integration| integration.onto.clone())
        .unwrap_or_default();
    let merged_of = |candidate: &Candidate| -> LadderRung {
        match &tip {
            Err(error) => LadderRung::unknown(error.clone()),
            Ok(tip) => match facts.contains(&candidate.revision, tip) {
                Ok(true) => LadderRung::yes(format!(
                    "{} is contained in {onto} at {}",
                    short(&candidate.revision),
                    short(tip)
                )),
                Ok(false) => LadderRung::no(format!(
                    "{} is not in {onto} at {}",
                    short(&candidate.revision),
                    short(tip)
                )),
                Err(error) => LadderRung::unknown(error),
            },
        }
    };

    let mut waves: Vec<WaveStanding> = Vec::new();
    for (ordinal, wave) in plan.waves.iter().enumerate() {
        let mut findings = FindingTally::default();
        let mut revisions = BTreeSet::new();
        for review in plan
            .reviews
            .iter()
            .filter(|review| review.wave == wave.wave)
        {
            revisions.insert(review.revision.clone());
            for finding in &review.findings {
                match finding.cause {
                    FindingCause::ContextGap => findings.context_gap += 1,
                    FindingCause::JudgmentGap => findings.judgment_gap += 1,
                }
                if finding.drift {
                    findings.drift += 1;
                }
            }
        }
        let legs: Vec<ReviewLeg> = ReviewLegKind::ALL
            .iter()
            .filter_map(|leg| plan.latest_leg(&wave.wave, *leg).cloned())
            .collect();
        let (state, gate, next, sealed_at) = wave_state(plan, wave, &waves, &tip, facts, &onto);
        waves.push(WaveStanding {
            wave: wave.wave.clone(),
            ordinal,
            units: wave.units.clone(),
            serialized: wave.serialized.clone(),
            review_model: wave.review_model.clone(),
            state,
            gate,
            next,
            writers: plan.writers(wave).into_iter().collect(),
            legs,
            rounds: revisions.len(),
            findings,
            sealed_at,
        });
    }

    let units = plan
        .units
        .iter()
        .map(|unit| {
            let wave = plan.wave_of(&unit.workflow_unit_ref);
            let missing = needs(unit);
            let locked = match &unit.lock {
                Some(lock) => LadderRung::yes(format!(
                    "locked at {} by {}",
                    lock.rung.as_str(),
                    lock.by.actor_ref
                )),
                None if missing.is_empty() => LadderRung::no(format!(
                    "not locked; {} has the rounds it calls for",
                    unit.rung.as_str()
                )),
                None => LadderRung::no(format!(
                    "not locked; {} still needs {}",
                    unit.rung.as_str(),
                    words(&missing)
                )),
            };
            let latest = plan.latest_candidate(&unit.workflow_unit_ref);
            let built = match latest {
                Some(candidate) => LadderRung {
                    revision: Some(candidate.revision.clone()),
                    ..LadderRung::yes(format!(
                        "candidate {} frozen by {}",
                        short(&candidate.revision),
                        candidate.by.actor_ref
                    ))
                },
                None => LadderRung::no("no candidate frozen".into()),
            };
            let merged = match latest {
                Some(candidate) => merged_of(candidate),
                None => LadderRung::no("no candidate to merge".into()),
            };
            let reviewed = match wave {
                None => LadderRung::no("in no wave".into()),
                Some((index, _)) => {
                    let standing = &waves[index];
                    match (standing.state, &standing.sealed_at) {
                        (WaveState::Sealed, Some(revision)) => LadderRung {
                            revision: Some(revision.clone()),
                            ..LadderRung::yes(format!(
                                "wave {} sealed: both legs clear at {}",
                                standing.wave,
                                short(revision)
                            ))
                        },
                        (WaveState::Unread, _) => LadderRung::unknown(standing.gate.clone()),
                        _ => LadderRung::no(format!(
                            "wave {} stands {}: {}",
                            standing.wave,
                            standing.state.as_str(),
                            standing.gate
                        )),
                    }
                }
            };
            UnitReading {
                workflow_unit_ref: unit.workflow_unit_ref.clone(),
                key: unit.key.clone(),
                wave: wave.map(|(_, wave)| wave.wave.clone()),
                dial: DialReading {
                    rung: unit.rung,
                    asks: unit.rung.asks().into(),
                    reason: unit.reason.clone(),
                    territory: unit.territory.clone(),
                    set_by: unit.set_by.clone(),
                    needs: if unit.lock.is_some() {
                        Vec::new()
                    } else {
                        missing
                    },
                    rounds: unit.rounds.clone(),
                    lock: unit.lock.clone(),
                },
                ladder: UnitLadder {
                    locked,
                    built,
                    merged,
                    reviewed,
                },
            }
        })
        .collect();

    WaveReading {
        schema: FACTORY_WAVE_READING.into(),
        run_ref: plan.run_ref.clone(),
        plan_revision: plan.revision,
        integration: plan
            .integration
            .as_ref()
            .map(|integration| IntegrationReading {
                repo: integration.repo.clone(),
                onto: integration.onto.clone(),
                tip: tip.as_ref().ok().cloned(),
                basis: match &tip {
                    Ok(tip) => format!("{} at {}", integration.onto, short(tip)),
                    Err(error) => error.clone(),
                },
            }),
        units,
        waves,
    }
}

type Standing = (WaveState, String, String, Option<String>);

fn wave_state(
    plan: &FactoryWavePlan,
    wave: &Wave,
    earlier: &[WaveStanding],
    tip: &Result<String, String>,
    facts: &dyn IntegrationFacts,
    onto: &str,
) -> Standing {
    if let Some(open) = earlier
        .iter()
        .find(|standing| standing.state != WaveState::Sealed)
    {
        return (
            WaveState::Waiting,
            format!("wave {} has not sealed", open.wave),
            format!("seal wave {}: {}", open.wave, open.next),
            None,
        );
    }
    // Seal first: a sealed wave stays sealed whatever was planned after it.
    let tip = match tip {
        Ok(tip) => tip,
        Err(error) => {
            return (
                WaveState::Unread,
                format!("Factory cannot read the integration: {error}"),
                "make the integration repository readable to Factory".into(),
                None,
            )
        }
    };
    let latest: Vec<Option<&Candidate>> = wave
        .units
        .iter()
        .map(|unit| plan.latest_candidate(unit))
        .collect();
    let structure = plan.latest_leg(&wave.wave, ReviewLegKind::Structure);
    let claims = plan.latest_leg(&wave.wave, ReviewLegKind::Claims);
    if let (Some(structure), Some(claims)) = (structure, claims) {
        if structure.verdict == ReviewVerdict::Clear
            && claims.verdict == ReviewVerdict::Clear
            && structure.revision == claims.revision
        {
            let contained = latest.iter().all(|candidate| {
                candidate.is_some_and(|candidate| {
                    facts.contains(&candidate.revision, &structure.revision) == Ok(true)
                })
            });
            if contained {
                return (
                    WaveState::Sealed,
                    format!("both legs clear at {}", short(&structure.revision)),
                    "the next wave opens".into(),
                    Some(structure.revision.clone()),
                );
            }
        }
    }
    for unit in &wave.units {
        let unit_name = name_of(plan, unit);
        match plan.unit(unit) {
            None => {
                return (
                    WaveState::Planning,
                    format!("{unit_name} has no plan"),
                    format!("set {unit_name}'s planning dial, with its reason and territory"),
                    None,
                )
            }
            Some(unit_plan) if unit_plan.lock.is_none() => {
                let missing = needs(unit_plan);
                return (
                    WaveState::Planning,
                    format!(
                        "{unit_name}'s plan at {} is not locked",
                        unit_plan.rung.as_str()
                    ),
                    if missing.is_empty() {
                        format!("lock {unit_name}'s plan")
                    } else {
                        format!(
                            "record {unit_name}'s {} round, then lock its plan",
                            missing[0].as_str()
                        )
                    },
                    None,
                );
            }
            Some(_) => {}
        }
    }
    if let Some(index) = latest.iter().position(Option::is_none) {
        let unit = &wave.units[index];
        let unit_name = name_of(plan, unit);
        let after = wave
            .serialized
            .iter()
            .position(|item| item == unit)
            .and_then(|at| at.checked_sub(1))
            .map(|before| format!(", after {} merges", name_of(plan, &wave.serialized[before])))
            .unwrap_or_default();
        return (
            WaveState::Building,
            format!("{unit_name} has no candidate"),
            format!("freeze {unit_name}'s candidate inside its territory{after}"),
            None,
        );
    }
    for (unit, candidate) in wave.units.iter().zip(&latest) {
        let unit_name = name_of(plan, unit);
        let candidate = candidate.expect("every unit has a candidate");
        match facts.contains(&candidate.revision, tip) {
            Err(error) => {
                return (
                    WaveState::Unread,
                    format!(
                        "Factory cannot read whether {} merged: {error}",
                        short(&candidate.revision)
                    ),
                    "make the integration repository readable to Factory".into(),
                    None,
                )
            }
            Ok(false) => {
                let next = match facts.contains(tip, &candidate.revision) {
                    Ok(true) => format!("merge {unit_name}'s candidate {} into {onto}", short(&candidate.revision)),
                    Ok(false) => format!(
                        "rebase {unit_name}'s candidate {} onto {onto} at {}, freeze it again, then merge it",
                        short(&candidate.revision),
                        short(tip)
                    ),
                    Err(error) => format!("merge {unit_name}'s candidate once Factory can read the repository ({error})"),
                };
                return (
                    WaveState::Building,
                    format!(
                        "{unit_name}'s candidate {} is not merged into {onto}",
                        short(&candidate.revision)
                    ),
                    next,
                    None,
                );
            }
            Ok(true) => {}
        }
    }
    let found: Vec<&ReviewLeg> = [structure, claims]
        .into_iter()
        .flatten()
        .filter(|leg| leg.verdict == ReviewVerdict::Findings)
        .collect();
    if !found.is_empty() {
        let leg = found[0];
        return (
            WaveState::FixRound,
            format!(
                "the {} leg found {} at {}",
                leg.leg.as_str(),
                match leg.findings.len() {
                    1 => "1 finding".to_owned(),
                    n => format!("{n} findings"),
                },
                short(&leg.revision)
            ),
            format!(
                "merge the forward fixes, then both legs review again at the round's final revision ({onto} at {})",
                short(tip)
            ),
            None,
        );
    }
    let unearned: Vec<&str> = [
        (ReviewLegKind::Structure, structure),
        (ReviewLegKind::Claims, claims),
    ]
    .into_iter()
    .filter(|(_, leg)| leg.is_none_or(|leg| &leg.revision != tip))
    .map(|(kind, _)| kind.as_str())
    .collect();
    (
        WaveState::InReview,
        if unearned.is_empty() {
            "both legs reviewed, not at one revision that holds every candidate".to_owned()
        } else {
            format!(
                "the {} leg has not cleared {onto} at {}",
                unearned.join(" and "),
                short(tip)
            )
        },
        format!(
            "review the {} leg at {}: a reviewer who wrote none of the wave",
            unearned.first().copied().unwrap_or("structure"),
            short(tip)
        ),
        None,
    )
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn territories_overlap_by_path_never_by_prefix_of_a_name() {
        let app = vec!["app/".to_owned()];
        assert!(territories_overlap(&app, &["app/a.rs".to_owned()]));
        assert!(territories_overlap(&["app/a.rs".to_owned()], &app));
        assert!(!territories_overlap(&app, &["apps/a.rs".to_owned()]));
        assert!(!territories_overlap(&app, &["kernel".to_owned()]));
        assert!(path_within("app/a.rs", "app"));
        assert!(!path_within("application.rs", "app"));
    }

    fn actor(name: &str) -> WaveActor {
        WaveActor {
            kind: ActorKind::Position,
            actor_ref: name.into(),
            generation_ref: Some("g".into()),
            caller_ref: None,
        }
    }

    fn unit(rung: PlanningRung) -> UnitPlan {
        UnitPlan {
            workflow_unit_ref: "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap(),
            key: None,
            rung,
            reason: "why".into(),
            territory: vec!["app/".into()],
            set_by: actor("planner"),
            set_at_unix_ms: 0,
            rounds: Vec::new(),
            lock: None,
        }
    }

    fn round(kind: RoundKind, verdict: Option<BlindVerdict>) -> PlanningRound {
        PlanningRound {
            round: kind,
            evidence_ref: "e".into(),
            by: actor("critic"),
            at_unix_ms: 0,
            verdict,
            amendments: Vec::new(),
        }
    }

    #[test]
    fn each_rung_needs_its_rounds_and_a_differing_blind_design_needs_the_owner() {
        assert!(needs(&unit(PlanningRung::P0)).is_empty());
        assert!(needs(&unit(PlanningRung::P1)).is_empty());
        assert_eq!(needs(&unit(PlanningRung::P2)), vec![RoundKind::Research]);
        assert_eq!(
            needs(&unit(PlanningRung::P4)),
            vec![
                RoundKind::Research,
                RoundKind::Adversarial,
                RoundKind::BlindDesign
            ]
        );
        let mut p4 = unit(PlanningRung::P4);
        p4.rounds = vec![
            round(RoundKind::Research, None),
            round(RoundKind::Adversarial, None),
            round(RoundKind::BlindDesign, Some(BlindVerdict::Similar)),
        ];
        assert!(needs(&p4).is_empty());
        p4.rounds
            .push(round(RoundKind::BlindDesign, Some(BlindVerdict::Different)));
        assert_eq!(needs(&p4), vec![RoundKind::DesignSession]);
        p4.rounds.push(round(RoundKind::DesignSession, None));
        assert!(needs(&p4).is_empty());
    }

    /// Git's facts as a table: the tip, and which commits each commit holds.
    struct Table {
        tip: String,
        holds: BTreeMap<String, Vec<String>>,
    }

    impl IntegrationFacts for Table {
        fn tip(&self) -> Result<String, String> {
            Ok(self.tip.clone())
        }
        fn resolve(&self, revision: &str) -> Result<String, String> {
            Ok(revision.into())
        }
        fn contains(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
            Ok(ancestor == descendant
                || self
                    .holds
                    .get(descendant)
                    .is_some_and(|held| held.iter().any(|item| item == ancestor)))
        }
        fn changed(&self, _: &str) -> Result<Vec<String>, String> {
            Ok(vec!["app/a.rs".into()])
        }
    }

    #[test]
    fn a_wave_seals_only_where_both_clear_legs_hold_every_latest_candidate() {
        let mut locked = unit(PlanningRung::P0);
        locked.lock = Some(PlanLock {
            rung: PlanningRung::P0,
            by: actor("planner"),
            at_unix_ms: 0,
        });
        let unit_ref = locked.workflow_unit_ref.clone();
        let mut plan = FactoryWavePlan::new("run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap());
        plan.integration = Some(Integration {
            repo: "/repo".into(),
            onto: "main".into(),
        });
        plan.units.push(locked);
        plan.waves.push(Wave {
            wave: "w1".into(),
            units: vec![unit_ref.clone()],
            serialized: Vec::new(),
            review_model: None,
            composed_by: actor("planner"),
            composed_at_unix_ms: 0,
        });
        let candidate = |revision: &str| Candidate {
            workflow_unit_ref: unit_ref.clone(),
            revision: revision.into(),
            changed: vec!["app/a.rs".into()],
            by: actor("builder"),
            at_unix_ms: 0,
        };
        let leg = |kind: ReviewLegKind, revision: &str, by: &str| ReviewLeg {
            wave: "w1".into(),
            leg: kind,
            revision: revision.into(),
            verdict: ReviewVerdict::Clear,
            findings: Vec::new(),
            by: actor(by),
            at_unix_ms: 0,
        };
        plan.candidates.push(candidate("c1"));
        plan.reviews.push(leg(ReviewLegKind::Structure, "m1", "r1"));
        plan.reviews.push(leg(ReviewLegKind::Claims, "m1", "r2"));
        let table = Table {
            tip: "m1".into(),
            holds: BTreeMap::from([("m1".into(), vec!["c1".into()])]),
        };
        assert_eq!(derive(&plan, &table).waves[0].state, WaveState::Sealed);
        // Two clear legs at two revisions do not seal.
        plan.reviews[1].revision = "m0".into();
        assert_eq!(derive(&plan, &table).waves[0].state, WaveState::InReview);
        plan.reviews[1].revision = "m1".into();
        // A later candidate the cleared revision does not hold unseals it.
        plan.candidates.push(candidate("c2"));
        let reading = derive(&plan, &table);
        assert_eq!(reading.waves[0].state, WaveState::Building);
        assert!(reading.waves[0].next.contains("rebase"), "{reading:#?}");
    }
}
