"""Apply the last inspected Factory-only joins; removed by the verification job."""
from pathlib import Path
import subprocess
r=Path(__file__).resolve().parents[1]
for path,expected in {
 'factory/src/attempt_central.rs':'27c8c18207db9813569f3e9a3a1cb5650e2f729b',
 'factory/src/attempt_receiving.rs':'e0cc0875226e63cf490b80e1f31a98c9acbe5d4a',
}.items():
 assert subprocess.check_output(['git','hash-object',path],cwd=r,text=True).strip()==expected, 'Source changed: '+path

def once(s,a,b):
 assert s.count(a)==1, 'Inspected replacement changed: '+a[:100]
 return s.replace(a,b,1)
p=r/'factory/src/attempt_central.rs';s=p.read_text()
needle='''    let leg = reading
        .legs
        .get(&attempt.workflow_unit_ref)
        .ok_or("attempt has no native workflow leg")?;'''
s=once(s,needle,'''    if let Some(latest) = attempt.observations.iter().rev().find(|receipt| receipt.contract == CALL) {
        if latest.operation_ref != intent.operation_ref
            && (previous.is_some() || matches!(latest.phase, OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain))
        {
            return Err("another Central preparation is current or unresolved; recover that exact request before replacing it".into());
        }
    }
'''+needle)
p.write_text(s)
p=r/'factory/src/attempt_receiving.rs';s=p.read_text()
s=once(s,'''    if let Some(placement) = &attempt.disposition.placement {
        input["now_ref"] = json!(placement.now_ref);
    }''','''    // Read-only attempts can acquire their native NOW after initial admission.
    // Preserve that actual owner correlation without inventing a placement grant.
    let now_ref = attempt.disposition.placement.as_ref().map(|placement| placement.now_ref.as_str())
        .or_else(|| attempt.tracking.iter().rev().find(|fact| fact.owner_ref == "central" && fact.kind == "now").map(|fact| fact.subject_ref.as_str()));
    if let Some(now_ref) = now_ref {
        input["now_ref"] = json!(now_ref);
    }''')
p.write_text(s)
p=r/'factory/tests/support/attempt_central_cases.rs';s=p.read_text();assert 'mod followup;' not in s
p.write_text(s+'\n#[path = "attempt_central_followup.rs"]\nmod followup;\n')
p=r/'factory/tests/support/attempt_central_followup.rs';s=p.read_text()
s=once(s,'"expectedSourceRevision":document["revision"]["revision"]','"sourceRevision":document["revision"]["revision"]')
s=once(s,'''    fs::write(relations_path, relations.to_string()).unwrap();''','''    fs::write(world.dir.path().join("Control/user/time.json"), json!({
        "schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London",
        "day_boundary_minutes":0,"automatic_day_rollover":true}).to_string()).unwrap();
    relations["relations"].as_array_mut().unwrap().push(json!({
        "ref":"central:source:control:root:Control/user/time.json","path":"Control/user/time.json",
        "roles":["civil-time-policy"],"provenance":"human-adopted","standing":"architecture-contract",
        "treatment":"projectcentral-user","recognition":"disposable-native-test-not-personal-adoption","recorded_at_unix_seconds":1}));
    fs::write(relations_path, relations.to_string()).unwrap();''')
p.write_text(s)
p=r/'.github/workflows/factory-native-evidence.yml';s=p.read_text()
s=once(s,"'^central_cases::native_central_.*: test$'", "'^central_cases::.*native_central_.*: test$'")
s=once(s,'native-evidence/native-central-cases.txt)" = 6','native-evidence/native-central-cases.txt)" = 7')
s=once(s,'native_central_ -- --nocapture','native_central_ -- --include-ignored --nocapture')
p.write_text(s)
p=r/'docs/CAW-NATIVE-ATTEMPT-OPERATIONS.md';s=p.read_text()
s=once(s,'factory attempt init <native-state>', 'factory attempt prepare <native-state> <request-json|-> --json\nfactory attempt init <native-state>')
section='''## Native preparation and fresh dispatch

`factory attempt prepare` (also `factory development attempt prepare`) calls
Central's actual policy, idempotent NOW allocation and destination-validation
Actions for the existing attempt. `CentralAttemptRequest` in
`factory/src/attempt_central.rs` carries the existing caller/authority/Run/attempt
identity, expected Factory revision, pinned Central endpoint, absolute working
 directory and selected destinations. `timeoutMs` is an optional total transport
budget of 1..30000 ms, not a new remote-worker lifetime.

Factory derives the allocation's task, purpose, selected Agent/Agency and source
relationships from the existing attempt. It retains the actual returned NOW,
source revision, policy revision and destination anchors; it never reconstructs
a NOW path or calls a preparation successful because supplied labels look right.
The preparation intent and all returned results stay in the existing attempt
store. NOW/source/policy tracking facts are committed before the ready checkpoint.
A refused destination retains its actual allocation and rejection so a corrected
request can reuse the same task NOW. No ordinary source file is written by the
preparation operation.

Exact replay is read-only. Explicit `recover:true` with the fresh Factory revision
reissues the owner's idempotent allocation and native reads, marking the previous
checkpoint pending **before** calling out. Process death cannot leave an older
ready checkpoint usable. An unresolved request must be recovered by its own
identity; an old request cannot refresh over a newer preparation. Concurrent
requests against one opening revision have one native transaction winner.

The existing owner dispatcher itself performs fresh policy, NOW source/lifecycle
and exact destination-anchor readback before a new send. Wrong cwd, source or
policy drift, a replaced destination or an expired preparation stops transport.
The successful preflight accompanies the existing bounded task packet and
transport receipt. Delivery recovery and historical replay never resend work.
This is not Git Candidate provisioning or an independently enforced worker
sandbox: stronger interception/material requirements still refuse on the plain
session path. An endpoint revision pin is not installed-binary authentication.

Read-only attempts retain the native allocated NOW through their actual Central
tracking fact. The existing receiving adapter carries that NOW with the verified
Return, even when no initial placement grant was supplied. Native arrival does
not include, edit or recognise the target document.

'''.replace('absolute working\n directory','absolute working\ndirectory')
s=once(s,'## Delivery and interruption',section+'## Delivery and interruption')
s=once(s,'## Tests and checks\n','''## Tests and checks

Preparation adds seventeen cases to the existing public owner-delivery suite:
identity/current-source gates, total transport budget, allocation-loss recovery,
refresh/process-death recovery, exact replay, successor ordering, concurrency,
expiry, wrong cwd, changed NOW/policy/destination and required enforcement refusal.
The normal Factory suite uses explicit Central protocol doubles for the portable
cases. The additional native evidence lane source-builds Central at the exact
pinned revision and requires seven discovered native cases, no fallback double.
It explicitly executes the real native Return/receiving case with
`--include-ignored`; that one case has an external-binary prerequisite and is
ignored only by the standalone Factory suite. Missing/zero-case native execution
cannot pass the required joined lane.

The joined Return case creates a disposable native Flow document under explicit
test-only policy/time/credential sources, prepares and dispatches a Factory
attempt, admits a controlled provider result and verification, and submits the
retained Return through the actual Central receiving operation. It checks actual
producer/task/NOW attribution and unchanged target bytes. Provider/verification
inputs remain controlled evidence, not independent Agent or commercial model
acceptance. Exact executed heads and logs remain PR evidence.

''')
s=once(s,"1. Consume Central's now-published fresh policy/NOW allocation/write validation\n   and lifecycle/expiry facts in the complete Factory attempt arrangement;", "1. Native Central preparation and fresh dispatch revalidation are implemented.\n   Complete the broader Candidate worktree and lifecycle arrangement;")
p.write_text(s)
print('Final Factory preparation ordering, native receiving correlation and documentation staged')
