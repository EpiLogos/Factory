"""One-shot inspected source integration, removed with its workflow after verification."""
from pathlib import Path
import subprocess
r=Path(__file__).resolve().parents[1]
expected={
 'factory/src/attempt_owner_dispatch.rs':'14a417e790fad079577911363bf073e88448f085',
 'factory/src/lib.rs':'9f20c968eadb084362d6654be548eaeebc08b221',
 'factory/src/attempt_cli.rs':'7b855a17c09d3a6975cb14843a63ceb9672dabcb',
}
for path,digest in expected.items():
 assert subprocess.check_output(['git','hash-object',path],cwd=r,text=True).strip()==digest, 'Inspected source changed: '+path
p=r/'factory/src/attempt_central.rs'; s=p.read_text()
s=s.replace('use std::time::{Duration, SystemTime, UNIX_EPOCH};','use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};')
s=s.replace('    pub recover: bool,\n','    pub recover: bool,\n    /// Total native preparation transport budget, never a remote worker lifetime.\n    #[serde(default)]\n    pub timeout_ms: Option<u64>,\n')
s=s.replace('fn call(endpoint: &CentralReceivingEndpoint, action: &str, input: &Value) -> Result<Value, String> {','''fn call(endpoint: &CentralReceivingEndpoint, action: &str, input: &Value, deadline: Instant, observed: &mut Vec<Value>) -> Result<Value, String> {
    let remaining = deadline.checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero()).ok_or("Central preparation transport budget exhausted")?;''')
s=s.replace('crate::native_process::output(&mut command, Duration::from_secs(30))\n        .map_err(|error| error.to_string())?', '''crate::native_process::output(&mut command, remaining)
        .map_err(|error| {
            observed.push(json!({"action":action,"request":input,"transportError":error.to_string()}));
            error.to_string()
        })?''')
s=s.replace('.map_err(|error| format!("Central ActionResult was unreadable: {error}"))?;','''.map_err(|error| {
            observed.push(json!({"action":action,"request":input,"unreadableResponse":true,
                "responseDigest":blake3::hash(&output.stdout).to_hex().to_string(),"responseBytes":output.stdout.len(),"transportError":error.to_string()}));
            format!("Central ActionResult was unreadable: {error}")
        })?;
    observed.push(json!({"action":action,"request":input,"response":response}));''')
s=s.replace('&serde_json::to_vec(&(phase, &receipt.payload))', '&serde_json::to_vec(&(&receipt.operation_ref, &receipt.source_revision, phase, &receipt.payload))')
s=s.replace('destinations: &[Value], anchors: bool) -> Result<Vec<Value>, String>', 'destinations: &[Value], anchors: bool, deadline: Instant, observed: &mut Vec<Value>) -> Result<Vec<Value>, String>')
s=s.replace('call(endpoint, "central.work.validate", &input)?','call(endpoint, "central.work.validate", &input, deadline, observed)?')
s=s.replace('previous: Option<&Value>) -> Result<Value, String>', 'previous: Option<&Value>, deadline: Instant, observed: &mut Vec<Value>) -> Result<Value, String>')
s=s.replace('call(endpoint, "central.work.policy", &scoped(endpoint, json!({})))?', 'call(endpoint, "central.work.policy", &scoped(endpoint, json!({})), deadline, observed)?')
s=s.replace('call(endpoint, "central.now.allocate", &input)?', 'call(endpoint, "central.now.allocate", &input, deadline, observed)?')
s=s.replace('validations(endpoint, policy, allocation, &destinations, false)?','validations(endpoint, policy, allocation, &destinations, false, deadline, observed)?')
s=s.replace('    let checkpoint = &receipt.payload["detail"]["checkpoint"];','''    let mut observed = Vec::new();
    let timeout = attempt.disposition.budget.wall_clock_timeout_ms.unwrap_or(30_000).min(30_000);
    if timeout == 0 { return Err("Central preflight transport budget exhausted".into()); }
    let deadline = Instant::now() + Duration::from_millis(timeout);
    let checkpoint = &receipt.payload["detail"]["checkpoint"];''')
s=s.replace('call(&endpoint, "central.work.policy", &scoped(&endpoint, json!({})))?', 'call(&endpoint, "central.work.policy", &scoped(&endpoint, json!({})), deadline, &mut observed)?')
s=s.replace('call(&endpoint, "central.now.read", &scoped(&endpoint, json!({"now_ref":allocation["now_ref"]})))?', 'call(&endpoint, "central.now.read", &scoped(&endpoint, json!({"now_ref":allocation["now_ref"]})), deadline, &mut observed)?')
s=s.replace('validations(&endpoint, old_policy, allocation, destinations, true)?', 'validations(&endpoint, old_policy, allocation, destinations, true, deadline, &mut observed)?')
s=s.replace('        || request.destinations.len() > 64','        || request.timeout_ms.is_some_and(|timeout| timeout == 0 || timeout > 30_000)\n        || request.destinations.len() > 64')
s=s.replace('    let intent = stamp(OwnerOperationReceipt {', '    let mut intent = stamp(OwnerOperationReceipt {')
old='''    if previous.is_none() {
        store.apply(action(&request, reading.revision, operation)).map_err(|error| error.to_string())?;
    }
    let prepared = prepare(&request, &attempt, &reading, previous.as_ref().and_then(|value| value.payload.pointer("/detail/checkpoint")));
    let (phase, detail) = match prepared {
        Ok(checkpoint) => (OwnerOperationPhase::Observed, json!({"checkpoint":checkpoint})),
        Err(error) => (OwnerOperationPhase::Uncertain, json!({"error":error,"instruction":"inspect the native NOW and explicitly recover; never start a worker from this result"})),
    };
    let settled = stamp(intent, phase, detail);
    let retention = retain(&mut store, &request, &settled);'''
new='''    let leg = reading.legs.get(&attempt.workflow_unit_ref).ok_or("attempt has no native workflow leg")?;
    let execution = attempt.execution_ref.as_deref().unwrap_or(&attempt.reserved_execution_ref);
    if leg.execution_ref != execution || !matches!(leg.status, crate::orchestration::LegStatus::Active | crate::orchestration::LegStatus::Detached) {
        return Err("preparation cannot revive a historical or terminal attempt".into());
    }
    if previous.is_none() {
        store.apply(action(&request, reading.revision, operation)).map_err(|error| error.to_string())?;
    } else {
        intent = previous.clone().expect("previous checked");
        // Explicit refresh invalidates the old proof before any new owner call.
        // A crash must not leave an earlier successful checkpoint usable.
        intent.payload["refreshBasisRevision"] = json!(reading.revision);
        intent = stamp(intent, OwnerOperationPhase::Dispatching, json!({"meaning":"explicit Central refresh before native effects"}));
        store.apply(action(&request, reading.revision, FactoryAttemptOperation::RecordObservation { attempt_ref:request.attempt_ref.clone(),receipt:intent.clone() })).map_err(|error|error.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms.unwrap_or(30_000));
    let mut observed = Vec::new();
    let prepared = prepare(&request, &attempt, &reading, previous.as_ref().and_then(|value| value.payload.pointer("/detail/checkpoint")), deadline, &mut observed);
    let (phase, detail) = match prepared {
        Ok(checkpoint) => (OwnerOperationPhase::Observed, json!({"checkpoint":checkpoint,"nativeResponses":observed})),
        Err(error) => {
            let phase = if observed.iter().any(|value| value.get("transportError").is_some()) { OwnerOperationPhase::Uncertain } else { OwnerOperationPhase::Failed };
            (phase, json!({"error":error,"nativeResponses":observed,"instruction":"inspect the retained native results and explicitly recover or correct the preparation; no worker was dispatched"}))
        }
    };
    let settled = stamp(intent, phase, detail);
    let retention = (|| {
        if phase == OwnerOperationPhase::Observed {
            // Keep per-owner source facts in the existing #222 tracking history.
            let checkpoint = &settled.payload["detail"]["checkpoint"];
            for (kind, subject, revision) in [
                ("now", checkpoint["allocation"]["now_ref"].clone(), checkpoint["allocation"]["revision"]["revision"].clone()),
                ("source-revision", checkpoint["allocation"]["source"]["ref"].clone(), checkpoint["allocation"]["revision"]["revision"].clone()),
                ("placement-policy", checkpoint["policy"]["scope_ref"].clone(), checkpoint["policy"]["revision"].clone()),
            ] {
                let fact = AttemptTrackingFact {
                    fact_ref: format!("factory-central-fact:{}", blake3::hash(serde_json::to_string(&(kind,&subject,&revision,&settled.receipt_ref)).map_err(|error|error.to_string())?.as_bytes()).to_hex()),
                    kind:kind.into(), owner_ref:"central".into(), subject_ref:subject.as_str().ok_or("missing native tracking subject")?.into(),
                    source_revision:revision.as_str().ok_or("missing native tracking revision")?.into(), evidence_refs:BTreeSet::from([settled.receipt_ref.clone()]),
                };
                retain_fact(&mut store, &request, fact)?;
            }
        }
        // The ready checkpoint is published last, never before its tracking.
        retain(&mut store, &request, &settled)
    })();'''
assert s.count(old)==1
s=s.replace(old,new)
helper='''fn retain_fact(store: &mut FileAttemptStore, request: &CentralAttemptRequest, fact: AttemptTrackingFact) -> Result<(), String> {
    for _ in 0..8 {
        let reading = store.reading().map_err(|error|error.to_string())?;
        if let Some(existing) = record(&reading, &request.attempt_ref)?.tracking.iter().find(|existing|existing.fact_ref==fact.fact_ref) {
            return if existing == &fact { Ok(()) } else { Err("Central tracking identity conflicts".into()) };
        }
        let operation = FactoryAttemptOperation::RecordTracking { attempt_ref:request.attempt_ref.clone(), fact:fact.clone() };
        match store.apply(action(request,reading.revision,operation)) {
            Ok(_) => return Ok(()),
            Err(error) => if store.reading().map_err(|error|error.to_string())?.revision==reading.revision { return Err(error.to_string()); },
        }
    }
    Err("Central tracking retention remained contended".into())
}
'''
s=s.replace('fn allocation_valid(',helper+'fn allocation_valid(')
p.write_text(s)
p=r/'factory/src/attempt_owner_dispatch.rs'; s=p.read_text()
s=s.replace('    let mut effective_packet = packet.clone();','    let dispatch_started = std::time::Instant::now();\n    let mut effective_packet = packet.clone();')
s=s.replace('    let intent = stamp(\n        OwnerOperationReceipt {','    let mut intent = stamp(\n        OwnerOperationReceipt {')
marker='    let invocation = NativeOwnerInvocation::AikitEncounter {'
insertion=r'''    if action == "send" {
        crate::attempt_runtime::validate_action_request(&action_request(
            &request, request.expected_revision, "admission",
            FactoryAttemptOperation::RecordObservation { attempt_ref:request.attempt_ref.clone(),receipt:intent.clone() },
        )).map_err(error)?;
        if let Some(preflight) = crate::attempt_central::preflight(attempt, cwd).map_err(error)? {
            if preflight["policy"]["data"]["enforcement"] != "native-actions" {
                return Err(error("the native policy requires worker interception/material enforcement not established by plain session delivery"));
            }
            intent.payload["placementPreflight"] = preflight.clone();
            intent = stamp(intent, OwnerOperationPhase::Dispatching, json!({"meaning":"Factory intent after fresh native Central readback, not worker enforcement"}));
            let body = effective_packet["turn"]["packet"]["text"].as_str().ok_or_else(||error("missing bounded task"))?;
            effective_packet["turn"]["packet"]["text"] = json!(format!("{body}\n\nNative Central placement preflight (not new authority or worker confinement):\n{}",serde_json::to_string(&preflight).map_err(error)?));
        }
    }
    let elapsed_ms = dispatch_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let timeout_ms = timeout_ms.saturating_sub(elapsed_ms);
    if timeout_ms == 0 { return Err(error("native owner transport budget exhausted during preparation")); }
'''
assert s.count(marker)==1
s=s.replace(marker,insertion+marker)
p.write_text(s)
p=r/'factory/src/lib.rs'; s=p.read_text().replace('pub mod attempt_cli;', 'pub mod attempt_central;\npub mod attempt_cli;');p.write_text(s)
p=r/'factory/src/attempt_cli.rs';s=p.read_text()
s=s.replace('    "attempt.init",','    "attempt.prepare",\n    "attempt.init",').replace('    "development.attempt.init",','    "development.attempt.prepare",\n    "development.attempt.init",')
s=s.replace('const CONTRACTS: &[&str] = &[','const CONTRACTS: &[&str] = &[\n    crate::attempt_central::CENTRAL_ACTION,\n    crate::attempt_central::CENTRAL_RECEIPT,')
s=s.replace('            Some("owner-action") => {','            Some("prepare") => crate::attempt_central::execute_cli(&args[1..], stdin),\n            Some("owner-action") => {')
s=s.replace('                let learning = crate::attempt_learning::execute_cli(&[], None)?;','                let learning = crate::attempt_learning::execute_cli(&[], None)?;\n                let preparation = crate::attempt_central::execute_cli(&[], None)?;')
s=s.replace(r'{receiving}\n\n{learning}', r'{receiving}\n\n{learning}\n\n{preparation}')
s=s.replace(r'Native task and Return readings:\n',r'Native task and Return readings:\n  factory attempt prepare <state> <request-json|-> [--json]\n')
p.write_text(s)
p=r/'factory/tests/attempt_owner_delivery.rs';s=p.read_text();assert 'mod central_cases;' not in s
p.write_text(s+'\n#[path = "support/attempt_central_cases.rs"]\nmod central_cases;\n')
print('Prepared inspected Factory integration; native tests must pass before commit')
