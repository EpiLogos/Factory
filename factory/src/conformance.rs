//! Factory-owned deterministic provider state for native contract conformance.
//!
//! This is not execution history. It is a validated owner state whose stable
//! locators let consumers exercise the real Factory binary without rebuilding
//! private provider state or inventing correlations.

use crate::build::{AgencyRecord, ExecutionRecord, FactoryBuildState};
use crate::core::identity::{Ref, Revision};
use crate::core::run::{Project, ProjectRef, Run, RunRef, WorkflowUnitRef};
use crate::developmental_read::{
    FactoryAgencyCarrier, FactoryAgencyCarrierMechanism, FactoryDevelopmentalFileProvider,
    FactoryDevelopmentalState, FactoryExecutionCorrelation, FactoryObservationStanding,
    FactoryOwnerTelemetryLink, FactoryRevisionedOwnerRef, FactoryTelemetryAvailability,
    FactoryTelemetryOwner, FactoryTemporalCorrelation,
};
use crate::journey::{Journey, JourneyCommission, JourneyRef};
use crate::routine_continuation::{
    FactoryRoutineContinuationRequest, AIKIT_ROUTINE_INVOCATION_REVISION,
    AIKIT_ROUTINE_INVOCATION_SCHEMA_SHA256,
};
use crate::workflow::{compile_workflow, WorkflowSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::Path;

pub const FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST: &str =
    "factory.developmental-conformance-manifest/v1";

const PROJECT_REF: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const JOURNEY_REF: &str = "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD";
const RUN_REF: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
const TELEMETRY_REF: &str = "telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA2";
const INVOCATION_REF: &str = "routine-invocation:2026-09-09:daily-research:1";
const WORKFLOW_SOURCE: &str =
    include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const CONTINUATION_REQUEST: &str =
    include_str!("../../contracts/factory/fixtures/routine-continuation-request.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryDevelopmentalConformanceManifest {
    pub contract: String,
    pub provider_state: String,
    pub project_ref: ProjectRef,
    pub journey_ref: JourneyRef,
    pub run_ref: RunRef,
    pub routine_run_ref: RunRef,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub telemetry_ref: Ref,
    pub invocation_ref: String,
    pub aikit_owner_revision: String,
    pub aikit_schema_sha256: String,
}

/// Write a complete, deterministic, validated native provider state.
pub fn create_developmental_conformance_state(
    path: &Path,
) -> Result<FactoryDevelopmentalConformanceManifest, Box<dyn Error + Send + Sync>> {
    let project_ref: ProjectRef = PROJECT_REF.parse()?;
    let journey_ref: JourneyRef = JOURNEY_REF.parse()?;
    let run_ref: RunRef = RUN_REF.parse()?;
    let project = Project::new(project_ref.clone());
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Exercise the Factory developmental conformance surface",
        "factory",
    )?;
    let mut build = FactoryBuildState::new(project, run)?;

    let source: WorkflowSource = serde_json::from_str(WORKFLOW_SOURCE)?;
    let workflow = compile_workflow(source.clone())?;
    let workflow_unit_ref = workflow
        .unit("inspect-source")
        .ok_or("embedded workflow lacks inspect-source")?
        .reference
        .clone();
    let authority = build
        .run_mutation_authority(&run_ref)
        .ok_or("conformance Run lacks mutation authority")?;
    build.apply_run_topology_command(
        &run_ref,
        &authority,
        workflow.topology_command(Revision::INITIAL),
    )?;
    build.insert_agency(AgencyRecord {
        run_ref: run_ref.clone(),
        agency_ref: "agency:factory/conformance".into(),
        agent_ref: "agent:factory-conformance".into(),
        label: "Factory conformance Agency".into(),
        position: None,
        root_scope_ref: None,
        metagency_grant_refs: vec![],
        actuation_ref: Some("actuation:factory-conformance".into()),
        return_ref: None,
        return_state: None,
    })?;
    build.insert_execution(ExecutionRecord {
        run_ref: run_ref.clone(),
        execution_ref: "execution:factory-conformance".into(),
        status: "contract-fixture".into(),
        agency_ref: Some("agency:factory/conformance".into()),
        agent_ref: Some("agent:factory-conformance".into()),
        harness_ref: Some("harness:factory-native-cli".into()),
        harness_composition_ref: None,
        agent_session_ref: None,
        session_space_ref: None,
        surface_refs: vec![],
        workcell_binding_refs: vec![],
        native_trajectory_ref: None,
    })?;

    let mut journey = Journey::new(
        journey_ref.clone(),
        project_ref.clone(),
        JourneyCommission {
            purpose: "Exercise the public Factory developmental contracts".into(),
            commission_ref: Some("commission:factory-conformance".into()),
            why_refs: vec!["issue:EpiLogos/agent-system-design#200".into()],
        },
        "Factory native conformance",
        "2026-09-09T09:00:00Z",
    )?;
    journey.add_run(
        run_ref.clone(),
        vec!["contract:factory.developmental-conformance-manifest/v1".into()],
        vec![],
    )?;

    let actuation_source = FactoryRevisionedOwnerRef {
        owner: FactoryTelemetryOwner::Actuation,
        reference: "actuation:factory-conformance".into(),
        revision: "contract-fixture-v1".into(),
        standing: FactoryObservationStanding::Derived,
        model_usage: None,
        resource_usage: None,
        contract_schema_digest: None,
    };
    let absent = |owner, reason: &str| FactoryOwnerTelemetryLink {
        owner,
        availability: FactoryTelemetryAvailability::Unavailable,
        observations: vec![],
        reason: Some(reason.into()),
    };
    let correlation = FactoryExecutionCorrelation {
        correlation_ref: "execution-correlation:01ARZ3NDEKTSV4RRFFQ69G5FA1".parse()?,
        telemetry_ref: TELEMETRY_REF.parse()?,
        run_ref: run_ref.clone(),
        workflow_unit_ref: workflow_unit_ref.clone(),
        execution_ref: "execution:factory-conformance".into(),
        agency_ref: "agency:factory/conformance".into(),
        carrier: FactoryAgencyCarrier {
            mechanism: FactoryAgencyCarrierMechanism::NativeHarness,
            carrier_ref: "harness:factory-native-cli".into(),
            determination_ref: "determination:factory-conformance".into(),
            source: actuation_source,
        },
        handoff: None,
        temporal: FactoryTemporalCorrelation {
            started: None,
            updated: None,
            completed: None,
            activity_refs: vec![],
            flow: None,
            change_horizon: None,
            source_changes: vec![],
            day_refs: vec![],
        },
        model_usage: absent(
            FactoryTelemetryOwner::Actuation,
            "no Actuation model-usage observation is supplied by this contract fixture",
        ),
        material_usage: absent(
            FactoryTelemetryOwner::Workcell,
            "no Workcell resource-usage observation is supplied by this contract fixture",
        ),
    };
    let mut state = FactoryDevelopmentalState::new(build, vec![journey])?
        .with_workflow_sources(vec![source])?
        .with_execution_correlations(vec![correlation])?;
    let request: FactoryRoutineContinuationRequest = serde_json::from_str(CONTINUATION_REQUEST)?;
    let admitted_at = DateTime::parse_from_rfc3339("2026-09-09T09:30:02Z")?.with_timezone(&Utc);
    let admission = state.admit_routine_continuation_at(request, admitted_at)?;
    FactoryDevelopmentalFileProvider::create_new(path, state)?;

    Ok(FactoryDevelopmentalConformanceManifest {
        contract: FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST.into(),
        provider_state: path.to_string_lossy().into_owned(),
        project_ref,
        journey_ref,
        run_ref,
        routine_run_ref: admission.continuation.run_ref,
        workflow_unit_ref,
        telemetry_ref: TELEMETRY_REF.parse()?,
        invocation_ref: INVOCATION_REF.into(),
        aikit_owner_revision: AIKIT_ROUTINE_INVOCATION_REVISION.into(),
        aikit_schema_sha256: AIKIT_ROUTINE_INVOCATION_SCHEMA_SHA256.into(),
    })
}
