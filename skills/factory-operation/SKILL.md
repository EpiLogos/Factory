---
name: factory-operation
description: Operate Software Factory Projects, Runs, RunMaps, build state, project-development condition, evidence, Candidates and native Actions through public Factory contracts.
---

# Factory operation

Use this Skill when an authorised human or Agent needs to orient in Factory developmental state, inspect a Run, gather evidence, request a native Factory Action, or explain why a Run is blocked or not promotable.

## Contract metadata

- Semantic ref: `factory:operator`
- Native owner: `EpiLogos/Factory` / Software Factory
- Executable root: `factory/`
- Build read contract: `factory.build-view/v1`
- Build provider contract: `factory.build-view-provider/v1`
- Project-development relation: `factory.project-development/v1`
- Public read path: `FactoryBuildViewProvider::snapshot`
- Public action path: `FactoryActionExecutor::execute`
- Verification: Factory Rust quality baseline plus `python3 scripts/validate_factory_skills.py`
- Risk class: developmental/action-sensitive; a Run is not host authority

## Ownership boundaries

Factory owns developmental Project/Run/RunMap/Candidate/Claim/Evidence/HumanRequest and Factory-native Action semantics. It may also retain a Run-scoped `ProjectDevelopmentLedger` containing attributable orientation, reflection, praxis and returned-discrepancy refs. That ledger does not make Factory the owner of the referenced worlds.

Central owns human-authored ProjectCentral Ground and the NOW/DAY temporal working field. AIKit owns KnowledgeApplication/ProjectMap traversal, SemanticWiki/Contemplate projection and Skill/Method/SkillSet/Profile/UsageOverlay/Context resolution. Workcell binding refs remain Workcell-owned. Actuation Agency/WorldBinding/Return refs remain Actuation-owned. QL/MEF is the formal owner where a QL-shaped reading is used. Preserve opaque external identities; do not reinterpret them into Factory authority.

Keep these distinctions explicit:

```text
human Ground != implementation truth
local description != implementation truth
derived code topology != authored meaning
NOW / DAY != Run history
RunThought != RunMap mutation
QL-shaped reading != QL storage bucket
observation != Claim acceptance
Claim != Evidence
plausible Claim != evidenced Closure
partial evidence != whole satisfaction
Candidate != recognised change
praxis configuration != praxis fitness
RunMutationAuthority != host authority
Capability granted != Action authorised
Factory Action != arbitrary shell execution
Factory developmental return != Actuation ReturnReceipt
Return received != owner Recognition
successful Run != automatic promotion
```

## Whole-relative verification / plausibility barrier

A `Claim` is an attributable proposed determination. For Agent-produced Claims, plausibility, coherence and confidence describe the Agent's present judgement; they do not establish verification standing. A visually convincing artifact, a successful subset of checks, a persuasive explanation or a high-confidence Assessment may support a Claim while still failing to establish the whole outcome the Claim appears to name.

For any consequential Claim that implies `done`, `complete`, `adequate`, `satisfied`, `fixed`, `verified` or an equivalent transition, recover the operative Whole and the explicit conditions against which that statement is meaningful. Follow the current Factory `VerificationRequirement` / `VerificationPlan` / Evidence / Assessment / Closure / Gate relation when available. The verification ledger is an accounting of the materially relevant obligations of that Whole, not a prose summary of why the result seems right.

Required law:

```text
operative Whole + opening condition
    -> explicit verification obligations
    -> exact evidence / assessment at each required obligation
    -> unresolved | contradicted | inapplicable | satisfied standing remains visible
    -> Closure only when the effective plan warrants Closure
```

Omission is never satisfaction. Evidence for one part does not silently generalise to another part. Representative or sampled evidence can establish whole-relative Closure only when the effective verification requirement itself defines that sampling as sufficient and the sampling/coverage condition is evidenced. If an obligation cannot be established, say so and keep the Run/Claim partial, blocked, contradicted or insufficiently evidenced as appropriate rather than substituting an Agent completion narrative.

Where obligations are genuinely independent, they may be verified in parallel. Where independent Assessment is required, the verifier must be independent according to the current Factory execution-lineage contract and must inspect or rerun the actual proving operation rather than trusting the producing Agent's report that it passed.

## Current cognitive / temporal whole disclosure

A Run may already have durable source-backed cognition in `RunThoughtField`, and the Project may expose current human↔Agent material through Central NOW/DAY. These are valuable because they keep the Agent's developing image of the work outside any single model context. When AIKit can disclose them through the existing authored-Wiki / Knowledge Navigation / Contemplate path, treat the result as an attributable **aggregate under structure** over the active whole/grain.

The aggregate may use any supported QL morphology at the grain the material warrants — small positive wholes, progressively conjugated/twelvefold forms, D1–D3, `4×4`, `6×6` or relational-generation fields. The purpose is to keep relevant members, relations, absences, tensions and standing jointly visible; matrix forms are not mandatory.

Preserve this composition:

```text
Central NOW / DAY refs
+ Factory RunThought refs
+ RunMap / Claim / Evidence subjects
+ Wiki / Flow / source relations
        ↓ AIKit structured disclosure / Contemplate
whole + grain + positions / relations / provenance / standing
        ↓
Factory reads the returned pressure
        ↓
Claim | verification obligation | RunMap change proposal | Evidence | no-change
```

A QL-shaped reading can disclose a missing or independent determination, but it cannot mutate RunMap merely by being structurally available. Factory applies only the ordinary authority-bearing topology/Claim/Evidence operations. This keeps the current generic system directly usable while leaving the same shapes addressable later by Vāk and traversable by richer Ta-Onta/Anima/Aletheia orchestration without introducing a second Run or cognition system now.

## Inputs

Obtain the resolved Project reference, selected Run reference, an authority-bearing caller context, and the smallest sufficient orientation already returned by native owners. Where available this may include Central ProjectCentral/Ground/NOW refs, AIKit semantic/ProjectMap/WikiFrame route refs, exact CodeReferences, StructuralGround, relevant RunThought cognitive refs and resolved praxis condition. Do not require these richer relations for an ordinary Project and do not fabricate them from the Build view.

## Procedure

1. **Orient in the authored whole.** Confirm Project / Focus. For product-meaning work, recover human-authored Ground first where available, then maintained semantic vocabulary, applicable StructuralGround/local description, exact reflection anchors, resolved praxis and current frontier. Use the smallest sufficient context; do not dump the whole Project.
2. **Read Factory state through the native provider.** Build a `FactoryBuildSelection { project_ref, run_ref }` and request `FactoryBuildViewProvider::snapshot`. Verify snapshot contracts, revision and provenance before treating it as current.
3. **Inspect the Run, RunMap and current cognition.** Read `view.run`, its `run_map_ref`, status and `view.frontier`. Where the cognitive optic is present, inspect the bounded `RunThoughtField` snapshot relevant to the current RunMap subject rather than replaying transcript history. Where Central NOW/DAY materially bears on the Run, consume its current refs through the accepted owner/provider seam. The frontier may be work, decision, recognition or return. Do not turn one projection of the RunMap, Thought field or NOW into canonical history.
4. **Inspect retained project-development condition and structured whole reading when one exists.** A `ProjectDevelopmentLedger` is a Run-scoped receipt, not a second Project database. Check provider refs/revisions, semantic↔local-source↔exact-code anchors, praxis input condition, capability rows and returned observations. If AIKit supplies a QL-shaped WikiFrame/Contemplate reading over Wiki/Flow/NOW/RunThought sources, preserve the exact whole/grain/basis/provenance and use it as orientation or epistemic pressure only; do not infer a Factory-local ProjectMap, CodeIndex, QL store or automatic Run topology.
5. **Inspect epistemic state against the operative Whole.** Read Claims and their Evidence refs separately, then reconstruct the relevant `Claim -> operative Whole/opening condition -> VerificationRequirement/VerificationPlan -> Evidence/Assessment -> Closure/Gate` relation where the subject is consequential. Check exact subject state, evidence currency, native provenance and producing Execution refs. Do not infer whole satisfaction from a plausible Claim or representative evidence. A structured whole reading may expose an omitted/unresolved relation; treat that as a Claim/verification/RunMap question until the native relation establishes its standing. Reflection discrepancy or praxis-fitness observations should point to ordinary Evidence where consequential. Missing, unresolved or contradictory evidence remains visible as such.
6. **Inspect Candidates and human apertures.** Read Candidate revision/status, producing Executions, Claims, Evidence, artifacts, preview and trade-offs. Read `HumanRequestRecord` for actual authorial questions and blocked Executions rather than escalating routine implementation choices.
7. **Inspect Agency/Execution/Return relations without ownership collapse.** Agency records may carry root-scope/metagency/actuation/Return refs; Execution records may carry AIKit harness, HarnessComposition, AgentSession, SessionSpace and Surface refs plus Workcell bindings. These refs are evidence of relation, not Factory grants. A filesystem path is not a WorldBinding.
8. **Check structural, cognitive and documentary return pressure.** When a structural change occurred, inspect applicable StructuralGround evidence and whether a pre-existing local description, governance source, ProjectMap relation, Wiki account, RunThought/NOW account or verification account became stale. Surface discrepancy; never silently overwrite an authored/native source from the read side.
9. **Mutate Run topology only through Run authority.** Use the Run's `RunMutationAuthority` with `apply_run_topology_command`. Never treat file access, a Skill, ProjectMap route, QL/Wiki reading, model or returned Evidence as Run mutation authority.
10. **Invoke only advertised Factory Actions.** Read `view.actions`. For `FactoryActionExecutor::execute`, supply the exact Action ref/subject/Run plus `FactoryActionAuthority`. The executor must see the Factory native owner, non-empty authority ref, required capability ref with `capability_granted=true`, and `action_authorised=true`. Any missing dimension fails closed.
11. **Separate returned pressure from Recognition.** The current native `Request more evidence` Action creates a HumanRequest; it does not recognise a Candidate or promote a change. Wiki/local-description/Ground/Skill/Method/NOW changes remain proposals to their native owners until accepted. Recognition/promotion remains owner/human governed.
12. **Verify provenance and closure basis.** Record Factory state revision, Run revision, RunMap revision, material source/provider revisions and the exact semantic/source/code/Thought/NOW/evidence refs used for consequential decisions. If reporting completion, also expose the operative Whole/opening condition, the required verification obligations and their terminal standings, plus the Closure/Gate basis that warrants the wording.

## Human-facing output

Keep the primary result at human altitude while preserving the verification basis:

```text
Project intention / operative Whole
  -> what changed / what the Claim says
  -> what current Thought / NOW material mattered
  -> which completion conditions were actually checked
  -> what evidence supports each material condition
  -> what remains unresolved / contradicted / inapplicable
  -> whether Closure is warranted, partial or absent
  -> what actually requires Recognition
```

Never present `done`, `complete`, `adequate`, `satisfied`, `fixed` or `verified` merely because the returned state is plausible. The human should be able to move from the completion statement to the exact conditions and evidence that warrant it. Raw graph topology and provider internals remain progressively inspectable provenance, not the default review surface.

## Verification

Run:

```text
cargo fmt --manifest-path factory/Cargo.toml -- --check
cargo clippy --manifest-path factory/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path factory/Cargo.toml --all-targets
python3 scripts/validate_factory_skills.py
```

Acceptance must include fail-closed Action cases for missing capability grant and missing Action authority, not only a successful request. Rich project-development acceptance must additionally preserve owner attribution, surface stale reflection rather than accepting/overwriting it, and keep the generic no-ProjectCentral/no-Method/no-QL case valid. Verification acceptance must reject a high-confidence or plausible completion Claim when one materially required obligation is unsupported, and must preserve partial/unresolved standing rather than silently closing the whole.
