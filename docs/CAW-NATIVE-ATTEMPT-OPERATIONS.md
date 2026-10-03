# Native attempts, owner delivery and Return intake

Status: implemented partial repository operations in Factory #221 -> #222,
under Factory #195 and O-I #220. This is not whole-feature acceptance. Factory
#201 remains the later real self-hosting campaign. No personal installation,
private Control mutation, commercial model execution or installed-world proof
is represented by this document.

## The native boundary

The existing RunMap coordinator remains authoritative. `FileAttemptStore`
uses the canonical developmental provider, not a copied Run or a second
scheduler. It retains source/workflow identity and basis, attempt history,
writer ownership, consumed retry grants, cancellation and unresolved outcomes.
A fresh process can reopen that same state. Exact Action replay is different
from submitting a new operation against a fresh revision.

The public binary routes through `attempt_cli.rs` and retains the existing
Development Field/base CLI for other operations. `factory capabilities --json`
and `factory attempt help` disclose the commands and contract versions.

```text
factory attempt prepare <native-state> <request-json|-> --json
factory attempt init <native-state> <seed-json|-> --json
factory attempt attach <native-state> <run-ref> <admitted-source-ref> --json
factory attempt read <native-state> [run-ref] --json
factory attempt action <native-state> <request-json|-> --json
factory attempt owner-action <native-state> <request-json|-> --json
factory attempt task <native-state> <run-ref> <task-ref> [--json] [--limit 1..100] [--cursor JSON]
factory attempt return <native-state> <run-ref> <attempt-ref> [--json]
factory attempt receiving <native-state> <request-json|-> --json
factory attempt learn <native-state> <request-json|-> --json
factory attempt material <native-state> <request-json|-> --json
```

`factory development attempt ...` projects the same operations. The existing
standalone `factory owner <invocation-json|->` remains a native owner adapter;
it does not itself attach the returned receipt to a Factory attempt.

Mutation requests carry the existing `FactoryActionCaller` and
`ProjectedFactoryActionAuthority`, exact Run/attempt identity and expected
revision. The concrete request structs and executable public-command examples
are in `factory/src/attempt_runtime.rs`, `attempt_owner_dispatch.rs`,
`attempt_receiving.rs`, `attempt_learning.rs`, `attempt_material.rs` and the
corresponding tests. Native authority fields are the established Factory
admission boundary, not a new claim of credential isolation between hostile
same-host processes.

An explicit native model choice can supply AIKit's
`aikit.explicit-model-selection/v1` receipt through the existing Execution
Intelligence selection field. `EXPLICIT_PIN` is a deliberate pin, not an invented
ranking result. Factory verifies the owner basis and route digests using
`aikit.sorted-json/v1`, the thin Pi composition's exact target basis, and the
attempt's matching Agent, Agency, WorldBinding, source ref/revision/digest,
AgentSession and SessionSpace. A valid receipt from another conversation is not
the body for this attempt. The ordinary ranked selection path remains available.
Neither receipt is dispatch, an authority grant, a loaded-Skill claim or a model
result; current native task/authority checks still run before the actual send.

## Native preparation and fresh dispatch

`factory attempt prepare` (also `factory development attempt prepare`) calls
Central's actual policy, idempotent NOW allocation and destination-validation
Actions for the existing attempt. `CentralAttemptRequest` in
`factory/src/attempt_central.rs` carries the existing caller/authority/Run/attempt
identity, expected Factory revision, pinned Central endpoint, absolute working
directory and selected destinations. `timeoutMs` is an optional total transport
budget of 1..30000 ms, not a new remote-worker lifetime.

The working directory is an invocation location, not an implicit write grant.
Factory checks that it is a real canonical directory within a native Central
writable destination and outside protected ground, then retains its device and
inode. A repository root can therefore be the cwd while its `.git` and other
protected descendants remain unwritable. Only the explicitly selected destinations
go through Central's write validation; the Workcell worker boundary remains
separate. Dispatch rechecks the cwd identity against fresh policy. Missing,
redirected or replaced directories refuse before worker transport. Older ready
checkpoints without this cwd anchor require explicit preparation recovery; they
are not silently upgraded or reused.

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
session path. The task-bound path described below consumes the native worker
boundary. An endpoint revision pin is not installed-binary authentication.

Read-only attempts retain the native allocated NOW through their actual Central
tracking fact. The existing receiving adapter carries that NOW with the verified
Return, even when no initial placement grant was supplied. Native arrival does
not include, edit or recognise the target document.

## Delivery and interruption

`factory.attempt-owner-action/v1` reserves a Factory-owned call intent in the
native store before invoking the actual AIKit addressed-session binary. The
selected Agent, session, Execution, source basis and current active attempt
must agree. The actual delegation and situated disposition are included in the
task packet; an acknowledgement is not task Return or verification.

The configured AIKit binary is the unified `aikit` executable. Factory invokes
`<binary> -C <cwd> session-space encounter --request-json <request>` for
addressed delivery and `<binary> -C <cwd> session-space encounter-task-read`
for protected task admission. An explicit binary override changes only the
executable path; it does not select a retired standalone companion or weaken
the owner contract. The supplied executable must support this native route.

The additive optional `transport` field on `aikit-encounter` declares a local
or SSH route to that same native owner. Both protected Task inspection and the
subsequent Encounter call use its exact binary, cwd and route. Omitting the field
preserves existing v1 request bytes and local invocation behavior. A declared
route uses absolute native paths and retains its actual `workcell_ref`; SSH
additionally requires an absolute `ssh_binary` and explicit `target`. The SSH
client uses batch mode, a bounded connection timeout and separately quoted
arguments. `environment` accepts only `PATH` and `WORKCELL_HOME` for native
helper discovery; credentials and control authority cannot be placed in it.
Factory does not copy a Mac executable or bearer into the remote process.

For a Task prepared over an existing Workcell Run, Factory reads native
`run show`, inspects its actual world receipt, then reads the Run again on the
same route. It compares the prepared scope revision, demand and boundary with
the Task, and the native world's exact Factory Run, current WorkflowSource
ref/revision/digest, unit, Task, Agent, Agency, binding, AgentSession, SessionSpace
and Workcell. A different undertaking, changed source or material revision,
unavailable owner or replaced material refuses before the send intent. These
sequential readings are retained previews; AIKit's locked Task admission and
Workcell's scope validation still run before the provider effect. Inspection
does not create a lease or certify completion. Native refusal diagnostics are
retained with bounded stdout/stderr rather than converted into generic absence.

Once Factory retains a send intent, delivery recovery must use its original
binary, cwd, contract, declared route and Execution identity. It reads the same
native delivery; a changed route cannot silently resend work. A timed-out SSH
client leaves the remote effect uncertain until native readback resolves it.

Transport has a finite deadline and a four-MiB limit on each output stream.
Timeout, invalid identity, unreadable output and lost response remain uncertain.
Stopping a client is not evidence that a remote worker stopped. No fixture
worker exists in the production adapter.

An exact replay of a retained owner request never sends the task again. A new,
explicit `delivery` request observes the original delivery and reconciles its
pending send intent. Local receipt-retention retries never invoke the external
operation again. Identical owner receipts are idempotent; conflicting terminal
observations require explicit re-resolution. The current call is settled last
so interrupted multi-receipt publication does not manufacture resolved state.
An actual owner response is returned even when post-effect local retention
fails; unavailable current readback is null rather than an old snapshot.

The pinned plain AIKit session path does not establish effective placement
protection. Factory therefore refuses protected or write-effect dispatch on
this path rather than treating declared coverage or a sandboxed control client
as confinement of an already-running worker.

The task-bound AIKit contract at
`8804866fb49ec5072aaedcfcf032c7a078fa585f` supplies the separate protected route
through `attempt_task_dispatch.rs`. Factory reads the actual prepared native
task and compares its exact Agency/source, NOW, policy, cwd, writable/protected
paths and Workcell inspection to this attempt. The addressed send carries
`expected_task`; AIKit checks it again at its own execution boundary. This
supports the joined operation without weakening the plain-session refusal.
Actual OS coverage, launched process and provider result still need their native
receipts; a configured boundary alone is not whole-operation acceptance.

## Attempt-scoped material lifecycle

`factory attempt material` connects the existing Workcell World adapter to the
same persisted attempt. It reuses `FactoryAttemptOwnerRequest`, with contract
`factory.attempt-material-action/v1`, and returns
`factory.attempt-material-receipt/v1`. The existing `WorkcellWorld` invocation
selects `inspect`, `observe`, `expose`, `collect`, `recover` or `release`.
`factory development attempt material` is the identical operation.

A new call must identify the exact Run/attempt/Execution, current Factory
revision, bound Workcell and original or recorded successor material World.
The caller supplies the pinned native binary, explicit service endpoint and
absolute material-receipt path. `WORKCELL_CONTROL_TOKEN` stays in the host
environment; credential-bearing invocation JSON is refused before persistence.
The bounded, private transport copy freezes the validated receipt bytes so a
changed input path cannot redirect a call or let Workcell rewrite the original
receipt. That temporary copy is not another material store.

Factory persists its intent before transport, then validates and retains the
actual owner response. Inspect/recovery preserve demand and caller subjects;
other results must identify the addressed World and native result shape.
Successful release is distinct from Workcell's preserved, suspended or
snapshotted dispositions. Unknown dispositions remain uncertain. Material
observations are visible in existing attempt/task readback; they are not
converted into Agent liveness, task-specific usage or a completed Return.

Recovery retains predecessor and replacement World identities without rewriting
the historical disposition. A superseded or released binding cannot start new
work. Recover/release also check other attempts sharing that Workcell/World in
the same canonical provider. The existing native transaction excludes competing
material effects and new starts, retries or dispatch while a consequential
material call is unresolved. This is provider-local coordination, not a global
inventory of every external user of a service.

Release requires the addressed attempt's explicit quiescent state and no
unresolved owner effects or active peer material users. This gate does not
itself cancel an Agent or prove independent worker quiescence. Recovery is a
material operation, not permission to continue an old Agent on a replacement
body; further execution needs its proper re-resolution and dispatch admission.

Exact replay never invokes Workcell again, including replay of a material read
made before the Execution was subsequently bound. A new successful read can
settle older uncertain transports for that same read operation and endpoint;
a slow older read cannot settle newer uncertainty. The original responses stay
in history. An ordinary healthy inspection cannot settle an interrupted
recover/release by pretending its effects never occurred. Such consequential
uncertainty remains blocked for explicit owner reconciliation. Post-effect
retention failure returns actual owner output and an error, with null readback
when current state is unavailable, rather than a fabricated rollback or replay.

## Task, source and telemetry reading

`factory.attempt-task-reading/v1` pages a task's native attempts at a pinned
provider revision. A stale or foreign cursor is refused. Current versus
historical standing comes from the coordinator's actual Execution identity,
not lexicographic attempt names. Readback carries the exact source revision,
digest and current-source status alongside Agent/context/praxis/body/NOW facts.

`factory.attempt-return-reading/v1` exposes the retained readable Return,
artifacts, evidence and receiving/archive/learning references. Human-readable
and JSON projections use the same native facts and do not mutate state.

Usage values come only from existing owner-validated execution correlations
matching the Run, Execution and workflow unit. A `model-usage` tracking
reference is not a measurement. Unobserved model or material usage remains
`not-observed`, never zero cost or an inferred aggregate. A receiving reference
is not human Recognition; an archive reference is not lifecycle proof.

## Reviewed receiving through Central

`factory.attempt-receiving-action/v1` submits an escaped, bounded contribution
proposal derived from the actual retained Return. It invokes the published
`central.receiving.submit` operation, retaining exact task/Run/session/NOW/Day
correlations and the selected destination source/document/revision. The native
producer key is stable for that Run/attempt/Return.

Central credentials come from the host's `CENTRAL_NATIVE_TOKEN`, not JSON,
argv or Factory state. Factory validates the returned native producer, opaque
receiving identity, source, proposal and correlations before attachment. A
foreign-producer response remains unresolved evidence, not an accepted link.

Ordinary exact replay does not call Central and exposes prior public JSON only
as unverified retained history requiring reconciliation. A canonical native
intent claim binds the original root/project, opaque producer key, endpoint and
exact input inside the existing Factory transaction. Only the insertion winner
submits. Equal aliases, retries and coordinator replacements perform guarded
`central.receiving.read` with the original producer key and original request;
a missing or unavailable receipt remains unresolved and never permits resend.
The original Return Action supplies proposal bytes before later correlations.

Optional `lookupEndpoint` selects a replacement native client for lookup only.
It is excluded from original request identity and may change only the binary,
never the original root/project/contract or input. Factory holds one bounded
regular client and checks its actual metadata and full SHA256 before and after
its read-only descriptor and guarded lookup. It checks the same held client
again before admitting the reply. First submission is qualified after its durable
intent, so an unavailable client leaves a recoverable unknown intent.

New native intents also retain the original Central root's canonical path and
held directory device/inode. The original locator and canonical path must still
name that held directory before and after calls. Commands use the original
qualified canonical root. A retargeted original locator is refused rather than
being treated as an unclaimed producer; independently declared distinct roots
remain distinct owners. Host-configured question/closure admission may use a
physically identical canonical root spelling: the current root must match the
retained original canonical path and device/inode. Only the client binary may
change. The original raw locator is held and rechecked, and the actual client
argv retains its qualified original canonical root. A retargeted original alias
is refused even when the host still names the old canonical root. Initial
question claims retain the absolute host root locator instead of discarding its
alias ancestry. Explicit public `lookupEndpoint` recovery still requires its
original raw root text, project and contract; this host bridge does not broaden
that request. Missing legacy physical identity stays unknown and is
never silently backfilled from the current filesystem or host environment.

Native by-reference decision/closure admission uses the original intent and held
physical owner, even when a currently configured client replaces the old client.
The opaque admission holds these objects through the existing Factory transaction
and rechecks them without calling Central while Factory is locked. The actual
native request digest still must match; matching visible proposal fields is
insufficient. Existing exact Finished replay retains its historical admitted
receipt and does not assert a new current native observation.

These are finite filesystem observations, not atomic fd-exec, a filesystem
sandbox or a lock on another product's source/grant/Project namespace. Directory
identity checks do not certify current native authentication or disclosure; those
remain required from the actual Central owner. A changed or unavailable client
or root with a prior reply retains that actual response and qualification cause.
No cleanup deletes client/root material and no qualification failure permits
resubmission. Missing lookup capability refuses without fallback. Legacy intents with no endpoint require the
original caller request digest; Factory never infers or backfills an endpoint.
Question retries retain their original endpoint/input and use the explicit
current host binary only for compatible same-owner lookup. Known-ref live human
and closure reads retain their existing contract. Later native observations are
new evidence; original receiving basis and owner receipt bytes are not rewritten.

Reserved native CALL/QUESTION observations require a private native admission;
public JSON cannot manufacture delivery proof. Lookup must affirm authenticated
producer selection and exact original request verification, and Factory checks
the actual response digest, schema, status, action, reference and correlations.
Post-call validation and publication failures retain the actual response and
separate causes as uncertain; no caller JSON or transport exit creates success.

This adapter never invokes document mutation, review or inclusion. Pending and
needs-review contributions remain proposals. Inclusion and human Recognition
remain separate owner operations.

## Existing development/learning intake

`factory.attempt-learning-action/v1` appends a `DevelopmentObservation` to the
existing `ProjectDevelopmentLedger` and correlates it back to the selected
attempt. The ordinary `factory development observations <ledger-root>
<run-ref> --json` command reads the same entry. Its native count field remains
`observationCount`.

Evidence must already be retained on this exact attempt. Insufficient-evidence
observations can record absence; fitness/discrepancy statements require actual
evidence references. Context, praxis, Agent, model, session, source and Return
subjects are derived from the existing record. An `OwnerReturnProposal` must
preserve required Recognition. Intake does not alter a Method, promote a
finding, train a model or recognise the Return.

A durable Factory intent joins the two existing stores. Interrupted intake can
be explicitly recovered without duplicating the observation. Ordinary replay
checks that the actual ledger still contains it; a missing ledger remains
unresolved rather than being inferred present from its old receipt.

The existing file ledger now locks and atomically publishes observation
appends. A stale writer cannot erase another process's observations; duplicate
identity with changed content is refused. This is append-safe observation
persistence, not a claim that all other ledger fields gained general merge/CAS
semantics.

## Exact owner dependencies

All three cuts below were inspected as open, unmerged PRs on 10 September 2026.
They are explicit adapter dependencies, not accepted-main claims.

| Owner | PR and exact source revision | Contract used |
| --- | --- | --- |
| AIKit | EpiLogos/ai-kit#278, `3d23d1eefbb999b0a5058ed0b4ca98dd2575b632` | `docs/implementation/CAW-NATIVE-DELIVERY.md`; native addressed send/delivery |
| Workcell | EpiLogos/Workcell#73, `f3a5be9fc751ee94b78aff11411e0cde65a46e4c` | `docs/CAW-MATERIAL-OPERATIONS.md`; attempt-scoped material lifecycle and standalone write-boundary adapter |
| Central | EpiLogos/Central#155, `e7e8479f1502732821bd3e7d3d5ceda38fd4279f` | `docs/CAW-NATIVE-CONSUMER-INTERFACES.md` and native receiving implementation |

A pinned contract is not verification of a locally installed executable's
identity. Installation/source parity remains later proof. No owner repository
was modified by this Factory continuation.

## Tests and checks

The material continuation adds 24 portable public-binary regression cases and
one separately executed source-built Workcell integration case. Regressions
cover exact replay across later execution binding, wrong-World input/output,
input-path swaps, bounded transport, retention failure, old/new observation
ordering, retained versus released material, shared users, native transactional
exclusion and preservation of original receipt/source identity.

The `Factory native material integration` workflow builds the pinned Workcell
CLI and control-service, requires exactly one discovered native test, and runs
it with `--include-ignored`. Missing native binaries or zero discovered cases
cannot pass. This exercises the actual Factory caller through Workcell's public
CLI/control service: read operations, managed-service host death, restart,
recovery to a new World and PID, replay without new effects, old-receipt refusal
and release of the replacement. Original receipt, source and pending-Return
bytes are checked unchanged. The hosted workload is a disposable TCP process,
not an Agent, independent verifier or commercial-model execution. The native
case is ignored only by standalone tests lacking the external owner binaries;
its dedicated CI lane executes it. Exact heads, logs and binary digests belong
to the retained run artifacts and PR return.

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

The earlier dispatch/Return tranche added 37 tests: four bounded transport unit
tests, ten public owner-delivery tests, seven task/Return-reading tests, seven
public Central receiving tests and nine learning/ledger tests. They exercise
the compiled Factory binary and disposable filesystem Worlds. Owner protocol
children are explicit test doubles confined to tests, not commercial model evidence.

Coverage includes persisted restart/readback, process death after send,
original-intent recovery, no-resend replay, repeated observations, writer/CAS
conflicts, foreign attribution, protection refusal, source-basis reading,
revision-bound pagination, telemetry absence, reviewed contribution proposal,
lost receiving acknowledgement, immutable arrival basis, concurrent existing
ledger writers and interrupted/missing-ledger learning recovery. Existing
fork/barrier, partial/unknown effects, consumed retry, historical late Return,
source protection and telemetry regressions are retained.

The Commission fixture check still compares the complete native state. Its
legacy fixture is normalised only for the new serde-defaulted empty
`attemptStates`; actual attempts must be empty, not discarded. This ordinary
repository check does not execute or gate on the real #201 campaign.

The temporary source-patching/auto-commit workflow and its script are removed
from the final diff. Normal formatting, Clippy, all-target tests and existing
conformance/Commission checks remain required. Exact executed heads and CI
results belong in PR #223, not an assertion that every later head is green.

## Remaining WEB CODE

1. Native Central preparation and fresh dispatch revalidation are implemented.
   Complete the broader Candidate worktree and lifecycle arrangement;
   map only representable protection into Workcell and attach it to the actual
   AIKit worker through #277's mandatory enforcement guard. Do not sandbox the
   control client and call the remote worker confined. Source/NOW paths and
   declared/effective coverage are not interchangeable.
2. Complete owner-backed, attempt-specific cancellation and observed quiescence,
   producer-independent verifier dispatch/result admission, and effective
   remote budget/stop-condition enforcement. Recorded verification obligations
   and supplied receipts are not an independently executing verifier.
3. Finish Central source-horizon, Day/NOW obligation, archive lifecycle and
   cross-Day re-resolution joins. Receiving submit/read is implemented here;
   archive-reference attachment alone is not an archive operation.
4. Attempt-scoped material inspect/observe/expose/collect/recover/release and
   their durable readback are implemented. Continuous observation scheduling,
   resource/usage collection and the complete attempt workflow's response to
   those updates remain separate work. The task view joins existing validated
   correlations; it does not invent a live collector. AIKit catalogue-to-execution,
   gateway, enforcement and recurrence joins, and Workcell's ambiguous
   interrupted-provider-effect recovery remain their owners' code obligations.

These are repository implementation gaps, not LOCAL PROOF and not grounds to
wait for personal installation before writing the remaining Factory code.

## Separate LOCAL PROOF

Later acceptance uses exact installed binary/source cuts, real credentials and
model/harness replies, actual effective restrictions, interruption/restart and
cross-Day/late Return with real services, independent Agent verification, human
receiving/inclusion and a genuine second placement. The later two-Guardian
Factory #201 campaign remains open. No local adoption or installed-world
acceptance occurred in this continuation.


## Native original-owner regression qualification

The `native_lifecycle_receiving` Source defines 19 explicitly ignored process
cases on required Linux/macOS. Qualification must run `--include-ignored` and
require exactly 19 successes with independently qualified current and previous
Ctrl binaries; a zero-test or ordinary ignored-only run is insufficient. Nine
existing decision/closure cases and six original-claim cases remain. Four further
cases use the actual filesystem/Factory/Central/OS: descriptor followed by regular
client replacement; the original root locator changed between two genuine roots;
actual abrupt coordinator death after durable intent and before native Ctrl exec;
and by-reference read of a genuine receipt with matching visible fields but a
different full original request digest. The death case explicitly does not prove
the earlier pre-spawn boundary, and refuses incomplete owned-group retirement.
All 19 definitions in this proposal are UNRUN; they are not installed acceptance,
model worker activity, physical human response or whole commissioned completion.

The required native receiving census for this host-bridge candidate is 21 on
macOS/Linux (19 preserved cases plus two actual original-alias controls). The
new controls exercise genuine old/current client images, original alias
retarget refusal, authenticated controlled human review/Resolve, final native
receiving/Finished, and unchanged owner bytes. Native crash-fixture cleanup
checks the exact owned Child before any signal and treats observed exit or wait
authority loss as non-passing uncertainty. All definitions are Source-only until
actual qualified execution; no Original Run, model worker or personal H credit
is supplied by these isolated cases.


### Bounded native capture at receiving and local-tool boundaries

A native capture failure retains the actual owned I/O cause, optional observed
exit status, captured stdout/stderr prefixes and their hashes, EOF/truncation,
deadline, wait-custody, stop and reap observations. Captured byte length is not
the total native output length. PID, birth identity, remote effect cancellation
and descendant quiescence are not inferred from these fields. A failed prefix
is never parsed as an ActionResult, even when it contains complete-looking JSON.
Complete nonzero Output keeps its actual status and native ActionResult.

The existing reserved CALL/QUESTION observation retains these private facts as
Uncertain through the existing internal admission path. Native by-ref Finished
and Resolve reads preserve the typed cause before the canonical provider lock;
failed reads do not mint an admission or change the provider. Canonical producer
claims, their original endpoint/input, guarded lookup and no-resubmit semantics
are unchanged. When a later observation publication also fails, both actual
causes survive: the original invocation is an explicit opaque accessor and the
native publisher remains discoverable through Error::source. No synthetic
publication uncertainty is created to transport capture evidence.

A local Actuation tool can submit capture facts in the existing tool-result
metadata under its actual granted stream. These facts are durable only when the
real Gateway acknowledges that event. If a later Gateway call fails, the opaque
error retains the original local capture, the actual Gateway error and only the
prior acknowledged receipts; it does not invent a result/evidence/Return or
retry. Complete nonzero local execution still produces its existing Failed
receipt. General Debug/Display omit private prefixes and receipt payloads;
explicit authorized evidence reads and JSON retention preserve their bytes.

The source-only consumer qualification census is retained with the proposal.
Its native integration parents require hash-pinned real Ctrl and, for Gateway
cases, hash-pinned real Actuation binaries in isolated roots. The dedicated
ignored child test is selected by the two Gateway parents exactly (three actual
child executions including the complete-nonzero control); a blanket selection
must not run that child without its required parent fixture. All definitions
are UNRUN until qualification. The additional post-document Central
`central.receiving.inclusion_incomplete` dual-failure case requires paired actual
Central owner qualification; it is not implemented or discharged by a Factory
JSON fixture, ordinary refusal, or a consumer-created ActionResult. Windows
PeekNamedPipe absolute-bound qualification remains open.

### Failed native owner dispatch is a failed live API operation

An actual failed `factory.attempt-owner-action/v1` invocation returns a typed
`AttemptOwnerError` from the live API. The CLI exits nonzero while its existing
`native_result` depth exposes the same public uncertain `OwnerOperationReceipt`
JSON. A failed transport is no longer a successful CLI delivery. Successful
owner-action JSON/text and the serialized uncertain receipt fields are
unchanged. The receipt is an observation of uncertainty, not worker delivery
or an assertion that no remote effect occurred.

The live error privately retains the original `NativeOwnerError`, its actual
bounded capture and the existing receipt. A distinct later native publication
failure remains discoverable through `Error::source`; explicit opaque accessors
retain the original invocation cause and separate retention/readback causes.
General `Debug`/`Display` and public receipt JSON do not export private prefixes
or argv. A typed `PublicationUncertain` does not trigger another same-store
settlement, compensation or resend.

Exact historical replay remains a read-only observation of the retained receipt.
It does not invoke the owner again, reconstruct a live typed I/O error from
serialized strings, or turn an uncertain receipt into current success. The
original native delivery lookup remains a separate owner operation. Private
capture/cause retention through the live API and CLI lasts only while that
process holds the error: these changes add no durable private destination,
carrier, registry or restart reconstruction of captured bytes. That partial-byte
and restart-evidence lifetime remains an explicit limitation.

The required Linux/macOS owner-dispatch group selects these six definitions
individually:

1. `attempt_owner_dispatch::native_failure_tests::actual_timeout_incomplete_pipes_retain_original_private_capture`
2. `attempt_owner_dispatch::native_failure_tests::actual_missing_executable_is_typed_failure_with_single_intent_and_no_resend`
3. `attempt_owner_dispatch::native_failure_tests::actual_capture_and_post_publish_uncertainty_remain_distinct_without_compensation`
4. `attempt_owner_dispatch::native_failure_tests::actual_source_receipt_projection_and_historical_replay_keep_public_schema`
5. `attempt_owner_dispatch::publication_tests::actual_fallback_publication_retains_primary_refusal_and_secondary_native_cause`
6. `attempt_owner_dispatch::publication_tests::actual_owner_retention_keeps_apply_cause_when_followup_native_read_fails`

The four new opt-in definitions use actual OS pipes/process failure, real
filesystem publication adversity and the actual Factory source/store/CLI
projection; they do not supply a successful AIKit response. The fifth is the
existing real publication case. The sixth runs an actual pre-byte stage privacy
fault and a distinct subsequent no-follow source-read failure. It preserves the
original held owner inode/bytes, restores only its owned replaced leaf, and
requires both actual typed IO causes without retry or receipt publication.
The same body also exercises actual owned-file corruption through the native
store JSON decoder and restores the original bytes. Actual native Io/Json
causes remain privately owned while the previous public InvalidOperation text
stays unchanged. All six are UNRUN in this Source candidate.
The inherited portable owner-delivery protocol doubles remain separate coverage.
No default green result or ignored-only run substitutes for these native bodies.

The existing native evidence workflow adds `owner_dispatch` as its seventh
required component outcome. Its compiled-list census requires exactly these
six names; each `--include-ignored --exact --test-threads=1` execution must
retain a complete log with exactly one named passed body and zero failures or
ignored bodies. The same bounded census rejects missing, extra, duplicate,
zero-test and incomplete captures. The controlled fixture uses the compiled
product's `ProjectCentral/now/tmp`, measured Python path/hash and observed
physical directory ancestry. Its filesystem/tool metadata is not a NOW, Agency,
ACL, model or process-quiescence grant. Failed or uncertain cleanup retains the
owned fixture and actual failure rather than signalling guessed processes.

The previous owner/source pins, groups and oracles remain required. The paired
FCI06 hosted wiring is Source-implemented and UNRUN: it builds the pinned ordinary
Ctrl and opt-in Central child and selects the existing c80 paired parent on
Linux/macOS. The qualification record retains actual `paired_build` and `paired`
outcomes. The separate 12c9 missing-arm Source successor is not composed or
selected by that wiring. `full_native_composite_qualified` remains false. These
isolated qualifications
cannot establish provider inference, an Original Run attempt, native independent
verification, Receiving/Recognition or whole commissioned completion.

### Current native cancellation composition (Source, execution pending)

The reviewed prior 29 receiving parents and one internal child remain intact.
The existing native evidence job explicitly adds the three real current-request
cancellation parents and one separately selected ignored library guard. It
admits the same private ProjectCentral/now/tmp evidence root before receiving
and shares that observation through the job environment; this does not grant
NOW, Agency, model, personal authority or Original Run acceptance.

The library guard receives only the actual first cancellation parent's
non-moved, pre-termination owner snapshot. A separate finite selector validates
the completed parent log, actual manifest and measured held snapshot bytes.
Missing, foreign, substituted, ambiguous or changed material refuses; no
synthetic-state fallback is used. Its existing native provider wrapper and
engine restore remain the semantic owner. The guard does not terminate a model
worker or publish a second Return.

Current Source/lock/archive, compiler JSON, native image bytes and pre/post
observations accompany the existing Linux/macOS gate. Compiler originals may
have normal Cargo hardlinks; retained copies are private single-link files.
Ordinary fixture bodies have bounded private copies, while links, FIFOs,
sockets and devices retain only typed metadata. Original fixtures are not
recursively deleted or directly passed to the artifact uploader. No inner
retirement witness, universal ACL, concurrent-writer exclusion or installed
baseline is inferred from this retention.

The complete named Source census is 64 parent definitions: the original 63
remain selected, with the existing ninth preparation followup now included.
That followup uses an explicit controlled provider turn and actual native
document creation, NOW allocation and successful Receiving without document
inclusion; it does not grant
model execution or document inclusion. Preparation selects all nine exact
bodies, including the ignored followup, through the same complete-run census.
Portable log filenames use the group and SHA256 of the exact case solely as
evidence geometry. Exclusive sidecars retain each exact case-to-path mapping
before dispatch; completed libtest bodies still prove the selected case. The
existing cancellation snapshot reader uses that same mapping. All actual
evidence paths, including Flow/FCI06 captures and numbered fixture copies,
require a finite ordinary/portable path checkpoint before artifact upload;
refusal retains the host failure and never renames a semantic identity or
certifies missing cases. The one internal capture
child still executes through its three existing parents, and is not another
parent. Existing Flow and sensing targets, normal format/Clippy/workspace/doc
gates, paired FCI06 and its uncomposed missing-arm dependency keep their prior
meaning. All new syntax, compiler, platform and native executions are UNRUN
until the admitted hosted replay. Root's separately retained installed Factory
image supplies the genuine Original old baseline in its own later replay;
this workflow neither downloads nor manufactures that image. Job cancellation,
host death, finite capacity refusal and post-last-observation filesystem change
remain explicit limits.
