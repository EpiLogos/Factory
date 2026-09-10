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
```

`factory development attempt ...` projects the same operations. The existing
standalone `factory owner <invocation-json|->` remains a native owner adapter;
it does not itself attach the returned receipt to a Factory attempt.

Mutation requests carry the existing `FactoryActionCaller` and
`ProjectedFactoryActionAuthority`, exact Run/attempt identity and expected
revision. The concrete request structs and executable public-command examples
are in `factory/src/attempt_runtime.rs`, `attempt_owner_dispatch.rs`,
`attempt_receiving.rs`, `attempt_learning.rs` and the corresponding tests.
Native authority fields are the established Factory admission boundary, not a
new claim of credential isolation between hostile same-host processes.

## Native preparation and fresh dispatch

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

## Delivery and interruption

`factory.attempt-owner-action/v1` reserves a Factory-owned call intent in the
native store before invoking the actual AIKit addressed-session binary. The
selected Agent, session, Execution, source basis and current active attempt
must agree. The actual delegation and situated disposition are included in the
task packet; an acknowledgement is not task Return or verification.

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
as confinement of an already-running worker. This restriction marks an
unfinished adapter join, not completion of protected continuous work.

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

Ordinary Factory replay does not call Central. Explicit recovery reads an
already-known receiving receipt, or retries the exact original producer-key
submission after a lost acknowledgement. It never replaces the original
proposal with a newly generated Return. Later owner inclusion observations are
new evidence; the original receiving basis is not rewritten.

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
| Workcell | EpiLogos/Workcell#73, `f3a5be9fc751ee94b78aff11411e0cde65a46e4c` | `docs/CAW-MATERIAL-OPERATIONS.md`; standalone material and write-boundary adapters |
| Central | EpiLogos/Central#155, `e7e8479f1502732821bd3e7d3d5ceda38fd4279f` | `docs/CAW-NATIVE-CONSUMER-INTERFACES.md` and native receiving implementation |

A pinned contract is not verification of a locally installed executable's
identity. Installation/source parity remains later proof. No owner repository
was modified by this Factory continuation.

## Tests and checks

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


This continuation adds 37 tests: four bounded transport unit tests, ten public
owner-delivery tests, seven task/Return-reading tests, seven public Central
receiving tests and nine learning/ledger tests. They exercise the compiled
Factory binary and disposable filesystem Worlds. Owner protocol children are
explicit test doubles confined to tests, not commercial model evidence.

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
4. Finish continuously fed owner material-lifecycle/resource/usage observations
   and their consumption by the full attempt workflow. The current task view
   joins existing validated correlations but does not invent a live collector.
   AIKit catalogue-to-execution, gateway, enforcement and recurrence joins, and
   Workcell's ambiguous interrupted-provider-effect recovery remain their
   owners' code obligations.

These are repository implementation gaps, not LOCAL PROOF and not grounds to
wait for personal installation before writing the remaining Factory code.

## Separate LOCAL PROOF

Later acceptance uses exact installed binary/source cuts, real credentials and
model/harness replies, actual effective restrictions, interruption/restart and
cross-Day/late Return with real services, independent Agent verification, human
receiving/inclusion and a genuine second placement. The later two-Guardian
Factory #201 campaign remains open. No local adoption or installed-world
acceptance occurred in this continuation.
