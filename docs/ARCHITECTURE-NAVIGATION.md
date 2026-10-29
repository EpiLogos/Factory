---
role: architecture
standing: agent-inference
scope: Factory native operations and composed O:I consumer boundaries
updated: 2026-09-30
---
# Factory architecture navigation

This is an implementation-facing navigation companion for O:I #65/#220 and the
existing documentation programme. It recovers native owners and successors;
it does not adopt a new design or claim the whole running experience complete.
Inspected native checkout revision: `18682ee4841bcf329265b2bea2f4509a6e8cff1b`. Active repair source may advance
that cut; identify the file revision before relying on its returned result.

## Governing source and successors


- [QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX](canon/QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md)
- [QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC](canon/QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md)
- [RUN-CLOSURE-VERIFICATION-ALIGNMENT](canon/RUN-CLOSURE-VERIFICATION-ALIGNMENT.md)
- [BOUNDED-AGENCY-FOUNDATION](program/BOUNDED-AGENCY-FOUNDATION.md)

Directory names are routes, not authority. Target design, amendment, historical
baseline, current implementation and observed result keep their own standing.

| Concern | Public operation / entry | Native source | Boundary and lifecycle |
| --- | --- | --- | --- |
| Commission / situated attempt | Native bounded Commission / attempt operations | `factory/src/commission.rs`; `attempt_runtime.rs` | Participant, actual execution body, effects, material protection, budget, obligations and return address. |
| Commission composition | `factory development commission-compose [draft\|-]` | `factory/src/commission.rs` (`compose_commission`) | A draft becomes the exact `factory.commission-request/v1` Factory would admit, with the Project/Journey/Run it derives; nothing is written until `commission` admits it. |
| Attempt scope | `factory attempt scope <state> [run-ref] [--attempt <ref>]` | `factory/src/attempt_scope.rs`; `attempt_runtime.rs` (`PlacementProtection`) | What each attempt may write (NOW path, writable and protected paths, coverage, write boundary), its permitted effects and source basis, read from its record; dispatch enforces the paths. Reach, read places and a write mode are named as not held. |
| Prompt pads | `factory development pads / pads-write / pads-run / pads-record / pads-due` | `factory/src/prompt_pads.rs` | A pad may carry `routine_ref`, the AIKit Routine that carries its schedule, verbatim and absent until a person links one; the Routine is AIKit's. |
| Custody | `factory development custody assign/update/list` | `factory/src/work_custody.rs` | Current Actuation generation and atomic successor handoff; responsibility is not Run ancestry. A close carries one of OpenRig's closure reasons (`--closure-reason`, with `--closure-target` where the reason names one); `completed` and `released` refuse a close without one by name, and a hand-off closes as `handed_off_to` its receiver. |
| Task state | `factory development task-activity [<state>] --request -` | `factory/src/task_activity.rs` | A read: the caller carries the owners' readings (AIKit's encounter status, the session's check-ins, the task binding) and Factory answers how each task stands in the manual's words, with the rung that decided and the stall OpenRig calls parked. Factory runs nothing and keeps no oracle; with a named state it reads its custody of each task's work and the closure a closed one carries. |
| Attempt / material record | Native attempt and Workcell material operations | `factory/src/attempt_native_store.rs`; `attempt_material.rs` | Attempt/coordinator metadata shares developmental transaction lock; Workcell owns material Run records. |
| Verification / receiving | Native review and receiving | `factory/src/attempt_review.rs`; `attempt_receiving.rs` | Evidence sourceRevision may name an owner journal; candidate applicability also requires attempt identity and subject basis. Receipt is not human Recognition. |

## Diagram and consumer relation

The maintained suite companion is
`source:project:O-I:docs/architecture/execution-return.md`; its editable diagram is
`source:project:O-I:docs/architecture/execution-return.mmd`. The O:I architecture entry
contains six question-specific companions, full-size rendered SVGs, an indexed
basis for every arrow, exact source hashes and independent navigation evidence.
Resolve that source in the current O:I checkout before substituting a cached or
historical copy. Its solid arrows are inspected relations, not installed
acceptance; proposed joins remain explicitly proposed.

Existing capability records link this companion through the optional
`extensions.documentation` protocol. These links change discoverability, not
capability IDs, coordinate placements, implementation status or source authority.

## Verification and open joins

Read each native test and its actual runner conditions, then the corresponding
dated Return. A test definition is not an executed result; a process/receipt is
not human Recognition. The suite's architecture verification records real
Mermaid rendering, source/link checks and the fresh-agent navigation task.
The four repair lanes continue to own their code, installed replay and open
architectural decisions. Preserve a missing join as missing until that proof
or decision is returned.
