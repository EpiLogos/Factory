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

## Native ledger retention and mutation

`FileProjectDevelopmentStore::transact` holds the existing Run lock across current-state loading, native semantic mutation, complete ledger validation and durable publication. Development Field commands and observation/learning intake use this operation; their receipts and readings come from the committed ledger. Orientation, Intent, reflection, praxis, capability, operative/material and Return relations are retained together without copying the authority of their referenced owners. Input parsing and external owner operations occur outside the transaction.

The existing `save` interface is limited to bootstrap and append-only observation compatibility: retained non-observation state must match exactly. Other metadata changes use `transact`, so a stale whole-ledger snapshot cannot silently replace them. The v1 encoding and native Run identity remain unchanged. Missing additive default fields remain compatible; explicit empty JSON forms are retained without assigning extension meaning. Unrecognised nonempty extension fields refuse mutation and preserve the original bytes until their owner contract is supported. Old binaries that do not follow the current transaction boundary must be drained before the installation cutover; an advisory lock does not constrain a non-cooperating writer.

The Run ledger, Build provider and developmental provider use the same private physical transaction helper, including `transact_developmental_state` and ordinary Commission/Action writes. It retains the existing native lock names, with a two-second acquisition deadline. Actual source/lock/stage opens use no-follow, nonblocking, close-on-exec descriptors and require regular files. Existing directory aliases are resolved to a physical parent once and rechecked against its held inode before and after publication; final source/lock/stage aliases are refused. Every create, replacement, sync and readback stays relative to the held directory. Existing source write admission is required before semantic mutation; a writable parent cannot bypass a read-only source.

Publication uses an exclusive unique hidden temporary file, retained mode/uid/gid/ACL/xattrs, file sync, atomic replacement and directory sync. Source and candidate privacy metadata must agree before replacement and at readback. Metadata preservation on Mac/Linux is bounded to 1,024 xattrs and 4 MiB total names/values, plus 4 MiB Mac ACL text; unsupported metadata or a changed source/lock/parent binding refuses mutation. New files retain owner-native directory defaults. Existing owner document capacity is unchanged. A failure before replacement retains the prior ledger and its candidate stage; no failure blindly unlinks a temporary pathname. Recovery must inspect the reported stage and current owner state before retiring any retained candidate. A sync, identity or readback failure after replacement is reported as uncertain and requires owner readback before retry, rather than a fabricated rollback. Process death releases the OS lock; recovery reads the same Run ledger.

Empty extension fields follow explicit native identities when records move within an array. A new Intent Return cannot inherit a prior Return's criterion metadata by position. Changed nested records without a supported identity and ambiguous array transfers refuse before publication, preserving the original bytes. These storage guarantees do not recognise a Return, promote learning or transfer source ownership.

### Native publication uncertainty and replacement basis

A native physical publication error carries the admitted source path and the original I/O cause when atomic rename succeeded but durability or source/privacy readback is unconfirmed. The Build provider, Run ledger and developmental provider retain that typed cause through projected Actions and CLI dispatch. They never infer an effect from error prose. The owner JSON failure contract is `factory.publication-failure/v1`: `ok=false`, `error.code=factory.publication_uncertain`, and `error.details` contains `source_path`, `published=true`, `outcome=unknown`, `automatic_retry=false`, and the original cause's `kind`, `raw_os_error` and `message`. This is a result of the existing owner operation, not another journal, authority, or operation identity. No `operation_ref` is invented.

Under `--json`, ordinary Factory CLI uncertainty uses that failure document on stdout while retaining the existing nonzero exit and human stderr text. The always-JSON headless Action projection exposes the same owner failure details. Workflow diagnostics and learning Return keep their existing result contracts and add typed `publicationUncertainty` only for an actual native publication uncertainty. Success schemas and nonpublication error behavior remain unchanged. The attempt store retains the same native cause through its existing error type. Owner-action Return preserves its actual successful owner receipt even when subsequent Factory retention is uncertain; its existing `retentionError` text remains, alongside conditional `publicationUncertainty`. A later read failure cannot erase the original publication cause. Revision-based contention retries stop for actual typed postpublication uncertainty, because a changed revision is not proof of a failed effect. The existing work-custody refusal route keeps ordinary pre-effect refusals unchanged. An actual publication uncertainty instead states that a candidate was published with unknown durability/readback and no automatic retry, retains the original cause, and adds conditional `publicationUncertainty`. It must never claim that nothing persisted. Sensing transactions use the same owner-native typed failure route. A consumer must inspect the original owner/source before retrying; a committed source is neither rolled back nor represented as unmodified. Native Central preparation, receiving, material and unit-decision projections retain their actual owner responses when subsequent attempt retention is uncertain. In the existing CLI failure document, `error.details.native_result` is optional and contains only an actual native result already obtained by that invocation; it appears only with typed publication uncertainty, independently of the original physical cause. Existing successful Return receipts keep their native-response fields and add conditional `publicationUncertainty` where retention failed. An uncertain publication stops automatic mutation on that same attempt source, including later settlement writes. An uncertainty in the separate learning ledger may still be observed through the attempt source; this does not retry the ledger effect. Project setup preserves the same typed failure when its existing developmental-state initialization publishes, without claiming placement or execution completed.

Whole-state `FactoryDevelopmentalFileProvider::create` remains an explicit validated replacement operation; `create_new` remains exclusive bootstrap. Before retaining omitted fields from a previous document, the owner validates its actual provider schema and native state. Unsupported nonempty fields refuse mutation. Empty omitted fields can remain only with the exact previous native Project identity. They cannot silently become metadata of another Project at the same physical path. A valid explicit replacement with no omitted fields remains supported, including a different Project. A refusal preserves all previous bytes and creates no migration or substitute identity.

If an ordinary dispatch completion refusal is followed by an uncertain fallback observation, the original `retentionError` and actual owner receipt remain primary. The returned transport detail separately retains `secondaryRetentionError` and `secondaryPublicationUncertainty`, and the existing `publicationUncertainty` discloses that actual unknown publication. No further settlement or retry follows it. Learning intake similarly returns conditional secondary fields when a separate ledger publication and its later attempt-source observation both fail; both physical source paths and causes remain available. A pre-replacement error keeps its actual I/O cause in the error source chain while naming the retained candidate stage.

Native physical publication qualification runs on Linux and Mac. Ordinary targets exercise real locks, files, ACLs, publication faults and restart behavior without privilege. One separately selected hosted test uses actual runner uid/gid and scoped superuser fixture authority to verify replacement of a source whose owner and group differ from the new stage. It operates only inside its owned temporary directory. Component publication requires both native platform gates; source parsing and workflow-shape checks do not constitute physical or installed acceptance.

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

The configuration owner retains its existing `oi.config-error/v1` document and frozen error-code enum. An actual publication uncertainty during link apply/reset uses `error_code=internal`, `retryable=false`, conditional `native_error_code=factory.publication_uncertain` and `publicationUncertainty` with the native physical facts; it is never a validation refusal. Its existing error result retains the original typed cause through in-process dispatch without parsing the serialized document. Setting and scope fields, stdout and nonzero exit remain unchanged. Workflow commissioning keeps the preceding actual Commission receipt as conditional `nativeResult` when later attempt attachment or reading fails. A retained successful Commission is distinct from an unconfirmed attachment; neither stage silently recommissions or retries.
