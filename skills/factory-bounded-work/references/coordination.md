# Factory bounded coordination law

Contract: `factory.bounded-coordination/v1`

This is the current Factory carrier for plural and interrupted bounded work.
Later workflow and pstack implementation consumes this carrier; it does not
create a second scheduler, agency ledger or execution ontology. The carrier is
part of the `factory-bounded-work` Skill and therefore inherits its
`METHOD:`-classified Skill identity. It is executable guidance and boundary
law, not a workflow-unit schema.

## Source basis and depth

The reduction is pinned to pstack revision
`93b00b89ef425a9c1bac0d0b317dfc49c930ac99`, with the source and edge accounts
under `docs/pstack-reduction/`. The SSSF execution optic is separately pinned
to `disler/super-simple-software-factory` revision
`de31374882e7a4e3e5b7bb9bd09e69dc2f779356` in
`docs/GUI-SSSF-SOURCE-FIDELITY.md` and
`factory-ui/source-integrations/sssf-visualizer.json`.

Factory keeps three distinct readings — **Semantic | Live | Trajectory**:

```text
Semantic    Factory meaning: Project, Journey, Run, frontier, Candidate,
            Claim, Evidence, Return and Recognition.
Live        current owner-provided Agency, Session, Harness, Surface,
            material and provider relations.
Trajectory  execution evidence: spans, tool calls, permissions, failures,
            retries, native trace refs and other target-native detail.
```

SSSF supplies an execution optic for `Trajectory`. It does not become the
Factory developmental ontology, Run owner, scheduler or source of Agency
meaning. A missing native trajectory field remains unavailable; it is never
filled from a fixture or inferred from process liveness.

## Admission and partition

1. Recover the commissioned concern, current subject revision, selected body
   and owner-provided bounds before delegating.
2. Use independent workers only when independent work or required judgement
   earns the extra coordination. **Fork != automatic parallelism.** A fork is
   an execution arrangement, not evidence that a second worker is needed.
3. Partition writable subjects before execution. Preserve **one writer per
   shared mutable subject** unless that subject's native owner supplies a
   verified concurrency contract.
4. A nested wrapper inherits the commission. Prompt language describing an
   agent does not recursively spawn an Agent, create an Agency or grant a
   tool.
5. Carry exact source, subject, body, authority, finite retry/compute/
   concurrency bounds, verification obligations and return address into each
   delegated brief. An omitted bound is unresolved, not unlimited.

## Coordination sequence

The coordinator retains the parent Run/Journey relation and remains
accountable for convergence.

```text
admit → partition → fork (only where earned) → execute
      → barrier → inspect returned artifacts → synthesize/return
```

- **Barrier:** a required branch must return its declared artifact and
  verification evidence before dependent work or synthesis can claim that
  obligation. A failed, missing or contradictory branch remains represented
  at the barrier.
- **Synthesis:** combine returned artifacts through Factory obligations and
  current subject revisions. Synthesis is a new attributable reading; it is
  not permission to overwrite a branch, erase dissent or turn a plausible
  summary into Closure.
- **Retry:** count retries against the original commission and subject. A
  retry reconciles partial prior effects first; it does not replenish a spent
  grant, reset a stop condition or silently widen the writable boundary.
- **Return:** report useful progress, exact evidence and unresolved
  conditions. Late or contradictory results remain evidence until admitted
  against the current subject; they do not retroactively become the winning
  branch.

## Interruption and recovery

On interruption, stop new dependent effects, request cancellation through the
selected harness protocol and observe the result. Account separately for:

1. pending logical work;
2. active tool/process termination;
3. accepted cancellation and whether quiescence was actually observed; and
4. retained artifacts, evidence and return refs.

A detached view is not cancellation. An accepted cancellation command is not
proof of quiescence. **Process liveness != execution ownership.** Ownership is
established by native execution identity, receipts and current subject state,
not by a PID or an apparently live process.

After restart, recover the native identity, receipts, subject revision and
spent/revoked authority before doing any effectful work. **Restart !=
replenished authority.** A restart may resume observation or reconciliation;
it cannot silently restore a consumed grant, revive revoked authority or count
the same partial effect as new work.

Late child output remains attributable evidence. Before incorporation, check
its original invocation, producing identity, source/subject revision and
current applicability. If the subject moved or the authority expired, retain
the output as late evidence and return the mismatch; do not apply it merely
because it arrived after the barrier.

## Ownership boundary

AIKit resolves Skill, SkillSet, Method classification, UsageOverlay, Profile
and ContextResolution. Actuation resolves Agency and authority. Workcell owns
material execution and resource facts. Factory owns the developmental
commission, Run/Journey attribution, obligations, barriers, returned evidence
and Recognition/owner-return relations. SSSF remains a source-pinned
trajectory optic. None of these passages may be replaced by a local parser of
ticket prose or a consumer-authored scheduler schema.

## Conformance obligations

A conforming implementation/test must demonstrate that:

- a single writer is retained for a shared subject;
- an unearned fork does not imply parallel work;
- a failed or missing branch blocks dependent synthesis while remaining
  visible;
- cancellation is distinguished from detach and process exit;
- retry and restart preserve cumulative bounds and spent authority;
- late output is retained and checked against current subject state; and
- Semantic, Live and Trajectory readings remain separate, including honest
  absence when the source cannot provide a field.
