//! Factory Return -> Central reviewed receiving. This calls the real `ctrl`
//! Action surface. It never edits, reviews, includes or recognises a document.
//! The only external write is the owner's idempotent producer-key submission.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::{FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptReading, FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION};
use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const CENTRAL_CONTRACT_REVISION: &str = "e7e8479f1502732821bd3e7d3d5ceda38fd4279f";
pub const RECEIVING_ACTION: &str = "factory.attempt-receiving-action/v1";
pub const RECEIVING_RECEIPT: &str = "factory.attempt-receiving-receipt/v1";
const CALL_CONTRACT: &str = "factory.attempt-receiving-call/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CentralReceivingEndpoint {
    pub binary: PathBuf,
    pub root: PathBuf,
    pub contract_revision: String,
    pub project: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceivingTarget {
    pub source_ref: String,
    pub document_id: String,
    pub source_revision: String,
    pub expected_authority_revision: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceivingRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    pub central: CentralReceivingEndpoint,
    pub target: ReceivingTarget,
    pub occurred_at_unix_seconds: Option<u64>,
    /// Explicitly observe a known receipt or retry the exact idempotent native
    /// submission after a lost response. Ordinary Action replay never calls out.
    #[serde(default)]
    pub recover: bool,
}
fn record<'a>(reading: &'a FactoryAttemptReading, reference: &str) -> Result<&'a FactoryAttemptRecord, String> {
    reading.attempts.iter().find(|record| record.attempt_ref == reference).ok_or_else(|| "attempt not retained in this Run".into())
}
fn action(request: &ReceivingRequest, revision: u64, suffix: &str, operation: FactoryAttemptOperation) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {contract:FACTORY_ATTEMPT_ACTION.into(),projection_ref:format!("{}:{}:{suffix}",request.projection_ref,request.request_ref),
        caller:request.caller.clone(),run_ref:request.run_ref.clone(),expected_revision:revision,authority:request.authority.clone(),operation}
}
fn retain(store: &mut FileAttemptStore, request: &ReceivingRequest, suffix: &str, operation: FactoryAttemptOperation) -> Result<(), String> {
    for _ in 0..8 {
        let reading=store.reading().map_err(|error|error.to_string())?;
        let attempt=record(&reading,&request.attempt_ref)?;
        if let FactoryAttemptOperation::RecordObservation{receipt,..}=&operation {
            if let Some(existing)=attempt.observations.iter().find(|existing|existing.receipt_ref==receipt.receipt_ref) {
                return if existing==receipt {Ok(())}else{Err("receiving receipt identity conflicts".into())};
            }
        }
        match store.apply(action(request,reading.revision,suffix,operation.clone())) {
            Ok(_)=>return Ok(()),
            Err(error)=>if store.reading().map_err(|error|error.to_string())?.revision==reading.revision{return Err(error.to_string());},
        }
    }
    Err("receiving retention remained contended; explicitly recover this request".into())
}
fn escape(text:&str)->String {
    text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;")
}
fn submission(request:&ReceivingRequest,attempt:&FactoryAttemptRecord)->Result<Value,String> {
    let returned=attempt.readable_return.as_ref().ok_or("a native readable Return is required before receiving")?;
    let identity=blake3::hash(format!("{}\n{}\n{}",request.run_ref,attempt.attempt_ref,returned.return_ref).as_bytes()).to_hex().to_string();
    let refs=|values:&BTreeSet<String>|values.iter().map(|value|escape(value)).collect::<Vec<_>>().join(", ");
    let html=format!("<p>{}</p><p>Factory Return: {}. Task: {}. Run: {}. Attempt: {}.</p><p>Artifacts: {}</p><p>Evidence: {}</p>",
        escape(&returned.summary),escape(&returned.return_ref),escape(&attempt.task_ref),escape(&request.run_ref.to_string()),escape(&attempt.attempt_ref),refs(&returned.artifact_refs),refs(&returned.evidence_refs));
    let mut input=json!({"producer_key":format!("factory-return:{identity}"),"source_ref":request.target.source_ref,
        "document_id":request.target.document_id,"expected_source_revision":request.target.source_revision,
        "task_ref":attempt.task_ref,"run_ref":request.run_ref.to_string(),"session_ref":attempt.disposition.body.agent_session_ref,
        "proposal":{"operation":"entry.add","entry_id":format!("factory-return:{identity}"),
            "contribution_id":format!("factory-return:{identity}:body"),"html":html}});
    if let Some(project)=&request.central.project {input["project"]=json!(project);}
    if let Some(revision)=&request.target.expected_authority_revision {input["expected_authority_revision"]=json!(revision);}
    if let Some(placement)=&attempt.disposition.placement {input["now_ref"]=json!(placement.now_ref);}
    if let Some(day)=attempt.tracking.iter().rev().find(|fact|fact.owner_ref=="central"&&fact.kind=="day") {input["day_ref"]=json!(day.subject_ref);}
    if let Some(time)=request.occurred_at_unix_seconds {input["occurred_at_unix_seconds"]=json!(time);}
    if serde_json::to_vec(&input).map_err(|error|error.to_string())?.len()>512*1024 {return Err("Return proposal exceeds Central's native receiving bound".into());}
    Ok(input)
}
fn call(endpoint:&CentralReceivingEndpoint,operation:&str,input:&Value)->Result<Value,String> {
    let mut command=Command::new(&endpoint.binary);
    // CENTRAL_NATIVE_TOKEN is inherited from the actual host credential channel;
    // never put it in the request, argv, Factory state or receipt.
    command.arg("--json").arg("--root").arg(&endpoint.root).arg("action").arg("run").arg(operation)
        .arg(serde_json::to_string(input).map_err(|error|error.to_string())?);
    let output=crate::native_process::output(&mut command,Duration::from_secs(30)).map_err(|error|error.to_string())?;
    let value:Value=serde_json::from_slice(&output.stdout).map_err(|error|format!("Central did not return a readable ActionResult: {error}"))?;
    if !output.status.success()||value["ok"]!=true||value["status"]!="success"||value["action"]!=operation {
        return Err(format!("Central did not confirm {operation}: {value}"));
    }
    Ok(value)
}
fn check_response(response:&Value,input:&Value,request:&ReceivingRequest,attempt:&FactoryAttemptRecord)->Result<(),String> {
    let data=&response["data"];let received=&data["record"];
    if data["schema"]!="central.receiving-reading/v1"||received["schema"]!="central.received-contribution/v1"
        ||data["return_ref"].as_str().is_none_or(|value|value.trim().is_empty())||data["revision"].as_str().is_none_or(|value|value.trim().is_empty())
        ||received["return_ref"]!=data["return_ref"]||received["source_ref"]!=input["source_ref"]||received["document_id"]!=input["document_id"]
        ||received["proposed_source_revision"]!=input["expected_source_revision"]||received["proposal"]!=input["proposal"]
        ||received["run_ref"]!=json!(request.run_ref.to_string())||received["task_ref"]!=json!(attempt.task_ref)
        ||received["session_ref"]!=json!(attempt.disposition.body.agent_session_ref)
        ||received["author"]["principal_ref"]!=json!(attempt.disposition.participant.agent_ref)
        ||received["now_ref"]!=input["now_ref"]||received["day_ref"]!=input["day_ref"]
        ||data["source_changed_by_arrival_or_review"]!=false {
        return Err("Central receiving result omitted or changed native source/producer/task/session identity; retain as unresolved".into());
    }
    if attempt.readable_return.as_ref().and_then(|returned|returned.receiving_ref.as_ref()).is_some_and(|reference|Some(reference.as_str())!=data["return_ref"].as_str()) {
        return Err("a different receiving identity is already attached to this Return".into());
    }
    Ok(())
}
fn stamp(mut receipt:OwnerOperationReceipt,phase:OwnerOperationPhase,detail:Value)->OwnerOperationReceipt {
    receipt.phase=phase;receipt.payload["detail"]=detail;
    receipt.receipt_ref=format!("factory-receiving-call:{}",blake3::hash(&serde_json::to_vec(&(&receipt.operation_ref,phase,&receipt.payload)).expect("JSON value" )).to_hex());receipt
}
fn result(store:&FileAttemptStore,request:&ReceivingRequest,observation:OwnerOperationReceipt,response:Option<Value>,failure:Option<String>,replayed:bool)->Value {
    let reading=store.reading();
    json!({"contract":RECEIVING_RECEIPT,"requestRef":request.request_ref,"runRef":request.run_ref,"attemptRef":request.attempt_ref,
        "replayed":replayed,"needsReconciliation":failure.is_some()||reading.is_err()||matches!(observation.phase,OwnerOperationPhase::Dispatching|OwnerOperationPhase::Uncertain),
        "transportObservation":observation,"centralResponse":response,"retentionError":failure.or_else(||reading.as_ref().err().map(|error|error.to_string())),
        "reading":reading.ok(),"humanRecognitionPerformed":false,"documentInclusionPerformed":false})
}

pub fn execute(path:&Path,request:ReceivingRequest)->Result<Value,String> {
    if request.contract!=RECEIVING_ACTION||request.request_ref.trim().is_empty()||request.projection_ref.trim().is_empty()
        ||!request.central.root.is_absolute()||request.central.contract_revision!=CENTRAL_CONTRACT_REVISION
        ||request.target.source_ref.trim().is_empty()||request.target.document_id.trim().is_empty()||request.target.source_revision.trim().is_empty()
        ||request.central.project.as_ref().is_some_and(|value|value.trim().is_empty()) {
        return Err("receiving requires exact Factory, Central PR #155 revision, absolute root and selected source identities".into());
    }
    let mut store=FileAttemptStore::open_run(path,request.run_ref.clone()).map_err(|error|error.to_string())?;
    let reading=store.reading().map_err(|error|error.to_string())?;let attempt=record(&reading,&request.attempt_ref)?;
    let mut identity=serde_json::to_value(&request).map_err(|error|error.to_string())?;
    for key in ["recover","expectedRevision","projectionRef"] {identity.as_object_mut().expect("request object").remove(key);}
    let digest=blake3::hash(&serde_json::to_vec(&identity).map_err(|error|error.to_string())?).to_hex().to_string();
    let operation_ref=format!("factory-attempt-receiving:{}",request.request_ref);
    let previous=attempt.observations.iter().rev().find(|receipt|receipt.owner_ref=="factory"&&receipt.contract==CALL_CONTRACT&&receipt.operation_ref==operation_ref).cloned();
    if previous.as_ref().is_some_and(|receipt|receipt.payload["requestDigest"].as_str()!=Some(digest.as_str())) {return Err("receiving Action identity reused with changed native request".into());}
    let input=if let Some(previous)=&previous {previous.payload["nativeRequest"].clone()}else{submission(&request,attempt)?};
    let intent=stamp(OwnerOperationReceipt{owner_ref:"factory".into(),contract:CALL_CONTRACT.into(),operation_ref,receipt_ref:String::new(),
        source_revision:format!("factory-state:{}",reading.revision),phase:OwnerOperationPhase::Dispatching,evidence_refs:BTreeSet::new(),partial_effect_refs:BTreeSet::new(),
        payload:json!({"requestDigest":digest,"nativeRequest":input,"centralContractRevision":CENTRAL_CONTRACT_REVISION,"attemptRevision":reading.revision})},
        OwnerOperationPhase::Dispatching,json!({"meaning":"receiving call intent; not worker execution, inclusion or Recognition"}));
    crate::attempt_runtime::validate_action_request(&action(&request,reading.revision,"admission",FactoryAttemptOperation::RecordObservation{attempt_ref:request.attempt_ref.clone(),receipt:intent.clone()})).map_err(|error|error.to_string())?;
    if let Some(previous)=&previous {
        if !request.recover {
            let response=previous.payload.pointer("/detail/centralResponse").filter(|value|!value.is_null()).cloned();
            return Ok(result(&store,&request,previous.clone(),response,None,true));
        }
    }else if request.recover {return Err("no retained receiving request to recover".into());}
    if reading.revision!=request.expected_revision {return Err("stale Factory revision before receiving call".into());}
    let known_receipt=attempt.readable_return.as_ref().and_then(|returned|returned.receiving_ref.as_ref()).cloned()
        .or_else(||previous.as_ref().and_then(|receipt|receipt.payload.pointer("/detail/centralResponse/data/return_ref")).and_then(Value::as_str).map(str::to_owned));
    let (operation,call_input)=if request.recover {
        if let Some(reference)=known_receipt {let mut read=json!({"return_ref":reference});if let Some(project)=&request.central.project{read["project"]=json!(project);}("central.receiving.read",read)}
        else {("central.receiving.submit",input.clone())}
    }else{("central.receiving.submit",input.clone())};
    store.apply(action(&request,request.expected_revision,"intent",FactoryAttemptOperation::RecordObservation{attempt_ref:request.attempt_ref.clone(),receipt:intent.clone()})).map_err(|error|error.to_string())?;
    let response=match call(&request.central,operation,&call_input) {
        Ok(response)=>response,
        Err(failure)=>{
            let uncertain=stamp(intent,OwnerOperationPhase::Uncertain,json!({"failure":failure,"recovery":"explicit recover uses the same native producer key, never a replacement Return"}));
            let retention=retain(&mut store,&request,"uncertain",FactoryAttemptOperation::RecordObservation{attempt_ref:request.attempt_ref.clone(),receipt:uncertain.clone()}).err();
            return Ok(result(&store,&request,uncertain,None,retention,false));
        }
    };
    let publication=(||->Result<(),String>{
        let current=store.reading().map_err(|error|error.to_string())?;let attempt=record(&current,&request.attempt_ref)?;
        check_response(&response,&input,&request,attempt)?;
        let data=&response["data"];let receipt_ref=data["return_ref"].as_str().ok_or("missing Central receiving ref")?.to_owned();
        let revision=data["revision"].as_str().ok_or("missing Central receipt revision")?.to_owned();
        let evidence=BTreeSet::from([receipt_ref.clone(),request.target.source_ref.clone()]);
        let owner=OwnerOperationReceipt{owner_ref:"central".into(),contract:"central.receiving-reading/v1".into(),operation_ref:format!("central.receiving:{receipt_ref}"),
            receipt_ref:format!("central-receiving-observation:{}",blake3::hash(&serde_json::to_vec(&response).map_err(|error|error.to_string())?).to_hex()),
            source_revision:revision.clone(),phase:OwnerOperationPhase::Observed,evidence_refs:evidence.clone(),partial_effect_refs:BTreeSet::new(),payload:response.clone()};
        retain(&mut store,&request,"owner-receipt",FactoryAttemptOperation::RecordObservation{attempt_ref:request.attempt_ref.clone(),receipt:owner})?;
        let current=store.reading().map_err(|error|error.to_string())?;
        let attached=record(&current,&request.attempt_ref)?.readable_return.as_ref().and_then(|returned|returned.receiving_ref.as_ref()).is_some();
        // The original receiving basis stays immutable. Subsequent review or
        // inclusion observations are new owner receipts, not rewritten history.
        if !attached {retain(&mut store,&request,"attach",FactoryAttemptOperation::AttachReceiving{attempt_ref:request.attempt_ref.clone(),receiving_ref:receipt_ref,source_revision:revision,evidence_refs:evidence})?;}
        Ok(())
    })();
    let phase=if publication.is_ok(){OwnerOperationPhase::Observed}else{OwnerOperationPhase::Uncertain};
    let mut failure=publication.err();
    let settled=stamp(intent,phase,json!({"centralResponse":response,"failure":failure,"meaning":"owner receiving evidence; no document inclusion or human Recognition"}));
    if let Err(error)=retain(&mut store,&request,"settled",FactoryAttemptOperation::RecordObservation{attempt_ref:request.attempt_ref.clone(),receipt:settled.clone()}) {failure=Some(error);}
    Ok(result(&store,&request,settled,Some(response),failure,false))
}

pub fn execute_cli(args:&[String],stdin:Option<&str>)->Result<String,String> {
    let args=args.iter().filter(|argument|argument.as_str()!="--json").collect::<Vec<_>>();
    if args.is_empty()||matches!(args[0].as_str(),"help"|"--help"|"-h") {
        return Ok(format!("factory attempt receiving <native-state> <request-json|-> [--json]\nAction: {RECEIVING_ACTION}\nCentral PR #155 source: {CENTRAL_CONTRACT_REVISION}\nThe host supplies CENTRAL_NATIVE_TOKEN, never JSON. Submit proposes a retained Return; recover explicitly observes/replays its exact native producer key. This does not review/include a human document."));
    }
    if args.len()>2{return Err("unexpected receiving command arguments".into());}
    let input=args.get(1).map(|value|value.as_str()).unwrap_or("-");
    let body=if input!="-"{std::fs::read_to_string(input).map_err(|error|error.to_string())?}else if let Some(body)=stdin{body.into()}else{let mut body=String::new();std::io::stdin().read_to_string(&mut body).map_err(|error|error.to_string())?;body};
    let request=serde_json::from_str(&body).map_err(|error|error.to_string())?;
    serde_json::to_string_pretty(&execute(Path::new(args[0]),request)?).map_err(|error|error.to_string())
}
