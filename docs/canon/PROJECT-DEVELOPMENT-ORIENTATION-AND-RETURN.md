# Project Development Orientation and Recursive Return

Status: **Factory canonical development relation**  
Owner: **Software Factory (`Factory`)**  
Primary programme: Factory #155

## Why this exists

A development Agent can make locally coherent changes while becoming progressively less faithful to the Project it is meant to develop. The failure is not merely missing context. It happens when the most convenient local decomposition becomes architectural authority before the Agent has recovered what the Project is, what its authored terms mean, which structural descriptions already exist, and how those descriptions relate to executable reality.

Factory therefore owns a developmental obligation: orient a consequential Run through the Project's existing meaning and praxis, retain the exact relations materially used, test returned reality against those relations, and return discrepancies to their native owners without collapsing their authority.

This is downstream of the O:I founding position that authored human intention is ground rather than generated implementation metadata. It is also downstream of Central's ProjectCentral contract, AIKit's KnowledgeApplication/ProjectMap/praxis contracts, Factory StructuralGround, and Actuation's situated Agency/Return boundary.

Factory does **not** gain ownership of those systems by retaining their references.

## Authority map

| Representation | Native authority | What Factory may retain |
|---|---|---|
| Human-authored Project Ground | Central / human source | attributable refs and revisions consulted for a Run |
| SemanticWiki identity and maintained knowledge | Wiki/Knowledge owner | semantic refs and traversal result refs |
| Local structural description / header / contract | native source owner | source ref, revision and declared relation |
| ProjectMap / CodeReference / CodeIndex | AIKit and provider owners | returned route/anchor refs, exact code refs, provider revision |
| Skill / UsageOverlay / Method / SkillSet / Profile resolution | AIKit | smallest sufficient resolved praxis condition |
| Generic Agency / WorldBinding / ReturnReceipt | Actuation | Factory developmental refs when germane, never substituted identity |
| Structural fidelity obligation, Run Claims/Evidence, developmental discrepancy | Factory | canonical Factory records |
| Epi/Bimba or other target coordinates | target domain/formal owner | opaque semantic/source identity and evidence of declared reflection |

The fact that two representations share a label is never parity evidence. Source truth, implementation truth and experienced truth remain distinguishable.

## Project entry order

For product-meaning work the useful order is:

```
human-authored purpose / intended experience
        ↓
SemanticWiki / Project vocabulary
        ↓
StructuralGround where one exists
        ↓
semantic ↔ local-description ↔ code reflection
        ↓
exact design / architecture / code / tests / evidence
        ↓
Project praxis / capability matrix
        ↓
current frontier
```

This is an ordering by relevance, not a demand to dump every source into context. The Agent should recover the smallest sufficient articulation needed to understand **what it is changing before feature-local convenience starts determining what the Project means**.

## ProjectCentral is discovered, not copied

Central owns ProjectCentral discovery and human-source identity. The public contract is rooted at:

- `<ProjectRoot>/ProjectCentral/project.json` (`central.project/v1`);
- `ProjectCentral/user` for human-authored Ground;
- `ProjectCentral/agents/governance` for agent governance;
- `ProjectCentral/agents/wiki/wiki.json` for the maintained Wiki source.

Factory consumes a Central/AIKit orientation result by reference. It does not parse ProjectCentral into a second Factory Project model and does not infer canonical identity from filesystem paths.

A Project may remain valid without ProjectCentral. Richer orientation is developmental capacity, not minimum execution validity.

## Reflection is an attributable route, not a Factory graph

AIKit KnowledgeApplication federates maintained Wiki knowledge, source material, CodeIndex intelligence and explicit ProjectMap relations. Factory records only the route that mattered for the Run:

```
semantic Project concept
    ↔ local structural source/description
    ↔ exact CodeReference(s)
    ↔ verification/evidence
```

The implementation type `ProjectDevelopmentLedger` retains `ReflectionAnchor` values with semantic ref, optional local source ref, exact code refs, verification refs, provider identity/revision and relation. Reverse code → meaning inspection walks those retained anchors only. It does not query, copy or reconstruct a Factory CodeIndex or ProjectMap.

A local description saying "this module is / part of / implements / owned by / constrained by" is attributable source material. It is not implementation truth. Derived code intelligence likewise does not become authored meaning merely because it reflects current code.

## Structural Source Fidelity

Factory #156/#157 established `StructuralGround` and the obligation to preserve constitutive structure where the target owns one. #155 composes with that gate rather than replacing it.

For a consequential structural claim, ask separately about:

1. **source fidelity** — did the Run retain the actual source and revision it claimed to preserve?
2. **identity / coordinate fidelity** — did source-owned identities survive without feature-local substitutes?
3. **relation fidelity** — did constitutive parentage and relations survive rather than flatten into similarly named features?
4. **operational fidelity** — does the executable system actually behave as the declared relation requires?
5. **experiential fidelity** — did returned human/agent experience bear out the intended result?

`verify_structural_ground` covers the structural subset it can prove. Behavioural and experiential evidence remain separate Claims/Evidence. No single passing check licenses a blanket parity claim.

## Praxis is an input condition; fitness is returned evidence

Factory does not resolve Skill, Method, SkillSet, Profile, UsageOverlay or harness composition. AIKit does. For a consequential Run, Factory may retain the smallest sufficient `PraxisCondition` returned by that resolution:

- Project / Focus;
- MethodRef where one materially conditioned the act;
- materially used SkillRefs;
- SkillSet / Profile relation where relevant;
- UsageOverlay refs/digests where relevant;
- ContextSources actually consulted;
- Actions invoked;
- model / harness / harness-composition refs;
- Agency and material/Workcell conditions where germane;
- AIKit resolution/provider ref and revision.

That is **praxis configuration = input condition**.

What reality discloses about the fitness of that choice returns as ordinary Factory Claim/Evidence plus, where useful, a `PraxisFitness` developmental observation. That is **praxis fitness = returned evidence**. A repeated fitness signal may justify a proposal to the Skill/Method owner; it never silently mutates the praxis source.

Trivial operations do not need giant receipts.

## Forward and returned capability matrix

`CapabilityPraxisRow` is deliberately a relation record rather than an activation engine. Where supported it can relate:

- Capability / Skill / Method;
- Project concept or structural target;
- optional QL-position affinity;
- use type;
- ContextSources and Actions;
- model / harness;
- Agency / authority;
- material conditions;
- verification expectation;
- observed fitness evidence.

It asks two different questions:

- what looked germane before the act?
- what did returned reality say about that judgement?

QL affinity remains descriptive metadata. It is never activation authority.

## Code ↔ meaning return

A Run may reveal a difference between maintained meaning, a local description and current executable reality. Examples include:

- a semantic concept no longer has its declared implementation binding;
- code moved while Project meaning remained stable;
- a local structural description is stale;
- derived topology contradicts an authored structural claim;
- verification falsifies an `implemented-by` relation;
- new code reality suggests Wiki knowledge should be revised;
- repeated returned reality puts pressure on authored Ground/design.

Factory represents the returned difference as Claims/Evidence and, when a native mutation is implicated, an `OwnerReturnProposal`. The proposal names the native owner/source and whether Recognition is required. It does not apply the mutation.

Correct return routes remain:

```
derived/code index     → provider rebuild/update
Agent Wiki              → Wiki owner maintenance
local description       → native source-authority change/proposal
human Project Ground    → proposal / Decision / Recognition
Skill / Method          → fitness evidence → proposal → owner Recognition
```

## Filesystem and document governance

Before meaningful structural work an Agent should determine the nearest applicable local source/contract chain for the region it will change. After a structural change it should ask whether an existing durable articulation became stale: local description, Agent governance, ProjectMap relation, Wiki knowledge, or verification account.

This does **not** mean every edit rewrites `AGENTS.md`, `CONTEXT.md`, a generated index, or any particular document system. DOX/ICM are not Factory law.

The law is narrower:

> A structural change should not leave the Project less intelligible when an applicable durable articulation existed beforehand.

Generated indexes refresh through their provider. Authored/local sources change under their own authority.

## Human review altitude

`ProjectDevelopmentLedger::human_review()` derives a compact review relation:

```
Project intention / Ground
        ↓
semantic meaning
        ↓
local structural source
        ↓
exact executable CodeReferences
        ↓
verification / Run evidence
        ↓
remaining discrepancies
        ↓
owner returns requiring Recognition
```

This is deliberately not a raw code-graph UI. Exact refs remain progressively inspectable for people or agents who need to descend into the evidence.

## Actuation boundary

These distinctions remain hard:

- Factory Run != Actuation;
- Factory Agency specification != generic Agency identity;
- filesystem path != WorldBinding;
- AIKit ContextResolution != AgenticComposition;
- Factory developmental observation != Actuation ReturnReceipt.

The Agent/Agency works in a world through Actuation's situated relation. Factory records developmental conditions and evidence. Returned difference carries enough semantic/source/code refs to reconstitute the developmental situation without taking ownership of the world or its Recognition path.

## Real Epi / holographic acceptance specimen

The strongest current source-faithful acceptance specimen is the Epi/QL relation already exercised by AIKit project-reflection work and Factory StructuralGround:

- Epi source revision: `daa660cbc1b8c5da83828698665a753852cb0287`;
- QL-MEF implementation revision: `de7d50c9f7dcfec33cfa0fd5f8a8a1068b4fbe84`;
- semantic/source identity: `formal:sixfold-complement`;
- local structural manifest: `QL-MEF/docs/integrations/epi-logos/EPI-HOLOGRAPHIC-KERNEL-MANIFEST.json`;
- exact implementation: `QL-MEF/c/src/primitive.c#ql_position_invert`.

Factory retains this as an opaque target-owned coordinate/identity relation. There is no generic `Mx`/`Mx′` field in Factory. The #155 acceptance test deliberately supplies a stale implementation revision, requires `StructuralFidelityIssueKind::StaleBinding`, retains the original reflection anchor unchanged, and returns the discrepancy to QL-MEF as an owner proposal.

Target-specific adversarial coordinate-law verification remains target/AIKit-owned. Factory consumes its evidence; it does not reimplement the law.

## Current physical boundary

At implementation time, no committed `ProjectCentral/project.json` instance was discoverable in the EpiLogos GitHub repositories inspected for this tranche. Therefore the automated repository acceptance can prove:

- Central's **real public ProjectCentral contract** is the orientation authority;
- Factory retains that contract by reference rather than copying it;
- the **real Epi/QL source-faithful reflection** is traversable and testable;
- Run praxis/evidence/recursive return work without a Factory-local resolver.

It cannot honestly claim that a physically instantiated local ProjectCentral world was discovered and opened from a developer filesystem. That remains a physical/local acceptance boundary, not a synthetic fixture to be disguised as closure.

## Generic contrast

An ordinary Factory Project remains executable with:

- no ProjectCentral praxis directories;
- no Method resource;
- no Bimba/QL coordinates;
- no special module header system;
- no ProjectMap reflection.

Generic Capability, Context/source mechanisms, Actions and ordinary Run Claim/Evidence remain enough. The richer relation is optional developmental capacity whenever the Project actually has richer authored structure.
