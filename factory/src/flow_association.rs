//! Factory's retained relation to an ordinary Central-owned Flow source.
//! Native reading is outside the Factory lock. It records an observed basis,
//! never human adoption, current content authority, execution or completion.
use crate::commission::{
    CommissionError, FactoryDevelopmentalMutation, FactoryDevelopmentalMutationRequest,
};
use crate::core::identity::{Ref, Revision};
use crate::core::run::RunRef;
use crate::developmental_read::FactoryDevelopmentalState;
use crate::journey::JourneyRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentralFlowLocation {
    pub schema: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub root: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryFlowObservation {
    pub location: CentralFlowLocation,
    pub document_id: String,
    pub document_revision: u64,
    pub source_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRunFlowAssociation {
    pub journey_ref: JourneyRef,
    pub run_ref: RunRef,
    pub workflow_source_ref: Ref,
    pub workflow_source_revision: String,
    pub workflow_source_digest: String,
    pub flow: FactoryFlowObservation,
    pub basis_refs: Vec<String>,
}

// Public JSON cannot construct this admission. The exact input is retained only
// in memory until the same Factory transaction checks its canonical basis.
pub(crate) struct NativeFlowAdmission {
    request: FactoryDevelopmentalMutationRequest,
}

type FlowAssociationFields<'a> = (&'a FactoryRunFlowAssociation, Revision, Revision, Revision);

fn fields(request: &FactoryDevelopmentalMutationRequest) -> Option<FlowAssociationFields<'_>> {
    match &request.mutation {
        FactoryDevelopmentalMutation::AssociateRunFlow {
            association,
            expected_provider_revision,
            expected_journey_revision,
            expected_run_revision,
        } => Some((
            association,
            *expected_provider_revision,
            *expected_journey_revision,
            *expected_run_revision,
        )),
        _ => None,
    }
}
fn invalid(message: &str) -> CommissionError {
    CommissionError::Invalid(message.into())
}
fn stable(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_whitespace)
}

pub(crate) fn validate_request(
    request: &FactoryDevelopmentalMutationRequest,
) -> Result<(), CommissionError> {
    let Some((association, _, _, _)) = fields(request) else {
        return Ok(());
    };
    crate::commission::validate_source_revision(
        &association.workflow_source_revision,
        "associate-run-flow.workflowSourceRevision",
    )?;
    let flow = &association.flow;
    if request.source.owner != "central"
        || request.source.reference != flow.location.reference
        || request.source.revision != flow.source_revision
        || flow.location.schema != "central.path-ref/v1"
        || !Path::new(&flow.location.root).is_absolute()
        || !flow.location.path.starts_with("Control/user/flows/")
        || !Path::new(&flow.location.path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || !stable(&flow.location.reference)
        || !stable(&flow.document_id)
        || !stable(&flow.source_revision)
        || association.workflow_source_digest.len() != 64
        || !association
            .workflow_source_digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || association.basis_refs.is_empty()
        || association.basis_refs.len() > 128
        || association.basis_refs.iter().any(|value| !stable(value))
        || association
            .basis_refs
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != association.basis_refs.len()
    {
        return Err(invalid("associate-run-flow exact native/source basis"));
    }
    Ok(())
}

fn validate_basis(
    native: &FactoryDevelopmentalState,
    request: &FactoryDevelopmentalMutationRequest,
    current: bool,
) -> Result<(), CommissionError> {
    let (association, provider_revision, journey_revision, run_revision) =
        fields(request).ok_or_else(|| invalid("associate-run-flow kind"))?;
    let journey = native
        .journeys
        .iter()
        .find(|journey| journey.journey_ref == association.journey_ref)
        .ok_or(CommissionError::InvalidStored)?;
    let view = crate::attempt_native_store::view_for(native, &association.run_ref)
        .map_err(|error| CommissionError::Conflict(error.to_string()))?;
    let source = &view.workflow_source.source;
    if !journey
        .runs
        .iter()
        .any(|link| link.run_ref == association.run_ref)
        || source.reference != association.workflow_source_ref
        || source.revision != association.workflow_source_revision
        || source.digest != association.workflow_source_digest
    {
        return Err(CommissionError::Conflict(
            "Run, Journey and exact workflow source are unrelated".into(),
        ));
    }
    if current
        && (native.build.revision() != provider_revision
            || journey.revision != journey_revision
            || view.run.revision() != run_revision
            || !crate::attempt_native_store::source_is_current(native, &view))
    {
        return Err(CommissionError::Conflict(
            "association Factory basis is stale".into(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare(
    native: &FactoryDevelopmentalState,
    request: &FactoryDevelopmentalMutationRequest,
) -> Result<Option<NativeFlowAdmission>, CommissionError> {
    if fields(request).is_none() {
        return Ok(None);
    }
    request.validate()?;
    if let Some(existing) = native.developmental_mutations.iter().find(|record| {
        record.request.mutation_ref == request.mutation_ref
            || record.request.occurrence_ref == request.occurrence_ref
    }) {
        return if existing.request == *request {
            Ok(None)
        } else {
            Err(CommissionError::ReplayConflict(
                request.mutation_ref.clone(),
            ))
        };
    }
    validate_basis(native, request, true)?;
    let (association, _, _, _) = fields(request).expect("checked operation");
    let endpoint = crate::attempt_native_receiving::configured_endpoint()
        .map_err(CommissionError::Conflict)?;
    if endpoint.root.to_str() != Some(association.flow.location.root.as_str()) {
        return Err(CommissionError::Conflict(
            "Flow location belongs to another configured Central root".into(),
        ));
    }
    let response = crate::attempt_receiving::call(
        &endpoint,
        "central.flow.read",
        &json!({"location":association.flow.location,"max_entries":1}),
    )
    .map_err(CommissionError::Conflict)?;
    check_reading(&response, &association.flow)?;
    Ok(Some(NativeFlowAdmission {
        request: request.clone(),
    }))
}

fn check_reading(
    response: &Value,
    expected: &FactoryFlowObservation,
) -> Result<(), CommissionError> {
    let data = &response["data"];
    if data["schema"] != "central.flow-reading/v1"
        || data["location"]
            != serde_json::to_value(&expected.location)
                .map_err(|error| invalid(&error.to_string()))?
        || data["document_id"] != expected.document_id
        || data["document_revision"].as_u64() != Some(expected.document_revision)
        || data["revision"] != expected.source_revision
        || data["format_version"] != 4
        || data["private_collections_included"] != false
    {
        return Err(CommissionError::Conflict(
            "Central Flow read refused or changed the exact document/source basis".into(),
        ));
    }
    Ok(())
}

pub(crate) fn apply(
    native: &mut FactoryDevelopmentalState,
    request: &FactoryDevelopmentalMutationRequest,
    admission: Option<&NativeFlowAdmission>,
) -> Result<(), CommissionError> {
    if !admission.is_some_and(|admission| admission.request == *request) {
        return Err(CommissionError::Conflict(
            "new Flow association requires an actual configured native Central reading".into(),
        ));
    }
    validate_basis(native, request, true)?;
    let (association, _, _, _) = fields(request).expect("checked operation");
    if native.developmental_mutations.iter().any(|record| matches!(&record.request.mutation, FactoryDevelopmentalMutation::AssociateRunFlow { association: previous, .. } if previous.run_ref == association.run_ref)) {
        return Err(CommissionError::Conflict("Run already has an explicit Flow association; replay its original mutation identity".into()));
    }
    native
        .journeys
        .iter_mut()
        .find(|journey| journey.journey_ref == association.journey_ref)
        .expect("checked Journey")
        .correlate_flow(association.flow.location.reference.clone())
        .map_err(|error| CommissionError::Conflict(error.to_string()))?;
    native
        .build
        .record_run_context_correlation(&association.run_ref)
        .map_err(|error| CommissionError::Conflict(error.to_string()))
}

pub(crate) fn validate_record(
    native: &FactoryDevelopmentalState,
    request: &FactoryDevelopmentalMutationRequest,
) -> Result<(), CommissionError> {
    validate_request(request)?;
    validate_basis(native, request, false)?;
    let (association, _, _, _) = fields(request).expect("association record");
    if !native.journeys.iter().any(|journey| {
        journey.journey_ref == association.journey_ref
            && journey
                .flow_refs
                .contains(&association.flow.location.reference)
    }) {
        return Err(CommissionError::InvalidStored);
    }
    // Retained history never rereads a current Flow or borrows later bytes.
    Ok(())
}

pub(crate) fn for_run(
    native: &FactoryDevelopmentalState,
    run_ref: &RunRef,
) -> Vec<FactoryRunFlowAssociation> {
    native
        .developmental_mutations
        .iter()
        .filter_map(|record| match &record.request.mutation {
            FactoryDevelopmentalMutation::AssociateRunFlow { association, .. }
                if &association.run_ref == run_ref =>
            {
                Some(association.as_ref().clone())
            }
            _ => None,
        })
        .collect()
}
