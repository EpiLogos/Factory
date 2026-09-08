# Software Factory — Praxis Primitive Relations

**Status:** canonical architecture clarification; implementation reconciled to current AIKit Method-classification law
**Date:** 2026-08-19; reconciliation 2026-09-08
**Companion to:** `QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md`, `QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md`  
**Purpose:** Ratify the relation among Skill, Method, SkillSet, usage adaptation, Context/Focus and returned Factory evidence without replacing AIKit's operational ownership.

---

## 0. Why this clarification exists

The Factory already treats `Capability` as a first-class part of the actor's developmental world, and its capability matrices already include methods, tools, sources, integrations and QL/use-type suitability. AIKit already treats Skills as reusable operational praxis and SkillSets as projection repertoires.

The missing distinction is the **contextual composition of praxis**.

A Skill is too atomic to describe every situated working method. A SkillSet is intentionally too simple: it says what repertoire is available/projected and composes by union. A Profile resolves what should be active. None of those should be overloaded into a workflow engine.

The Factory therefore ratifies `Method` as the product-level relation that answers:

> **How should the available praxis, knowledge and powers be related for this purpose and Focus?**

This is an ontology clarification, not a demand for a new central store or a new orchestration runtime.

### Current implementation reconciliation

The product vocabulary below remains useful, but current AIKit implementation does not create a
second `Method` resource. A Method is the same Skill/Capability identity whose Skill description
starts with `METHOD:` and which AIKit exposes through `aikit method list`. The Factory source
`skill/factory-native/factory-bounded-work` therefore remains a source-owned Skill classified as
METHOD; its former standalone `method.json` source is retired. Situated metadata, UsageOverlays,
and nested SkillSet relations remain attached to the ordinary Skill identity. Any retained
`method_ref`/`method_refs` field in a compatibility projection is non-authoritative and carries
that Skill reference; it does not create a standalone Method lifecycle.

---

## 1. Primitive relations

### Capability

A `Capability` remains the general power available to an actor.

It may be realised through a Skill, Action, tool, script, source integration, provider, runtime component or another native mechanism. Capability identity must not collapse into any one surface through which the power is exposed.

### Skill

A `Skill` is:

> **A reusable body of organised intelligent praxis which teaches or exposes a bounded way of inquiring, judging, transforming, verifying or returning.**

A Skill may itself route to Actions, ContextSources, tools or other capabilities. It remains source-owned and independently addressable.

In Factory terms, a Skill is a reusable praxis-bearing Capability form, not the whole situated method of a Run.

### UsageOverlay

A `UsageOverlay` is:

> **A scoped addition to how an unchanged Skill should be used in a particular context.**

Typical scope may be personal, Project, Profile, session, task, Run or Focus. An overlay can focus vocabulary, point toward Project-specific ContextSources, impose a return form, or adapt invocation to a local situation.

It does **not** mutate the Skill's authoritative source.

Repeated overlays are evidence. They do not automatically become durable praxis.

### Method

A `Method` is:

> **A named, inspectable, Focus-bearing composition of praxis and resources for a kind of act.**

A Method may relate:

```text
SkillRefs
UsageOverlays
ActionRefs / CapabilityRefs
ContextSourceRefs
ordering or conditional relations where genuinely required
expected intermediate and return forms
verification / recognition expectations
Project or domain vocabulary
```

A Method can compose several Skills, point one Skill toward another, establish a workflow relation among them, or specialise an existing Skill for Project conditions without forking it.

Method is therefore the main seam between **praxis out of context** and **praxis in context**.

### SkillSet

A `SkillSet` remains:

> **A selectable additive repertoire of capabilities for projection.**

It answers what should be made available to a harness, not how those capabilities must be orchestrated for a particular act.

The Factory therefore preserves AIKit Spec III's distinction:

```text
Profile : resolution
SkillSet : projection
Method   : situated praxis composition
```

A SkillSet may contain the Skills a Method refers to, but Method membership does not grant trust, authority or availability. Operational eligibility remains resolved by AIKit.

### Profile

A `Profile` remains resolution-side composition: what should be active in the operative world under the relevant scopes.

A Profile may make Methods or SkillSets eligible/default for a Project or actor, but Profile precedence must not be reimplemented inside Method.

---

## 2. Method is a product primitive; operational resolution remains external

`Method` belongs at the product/experienced level because both humans and Agents should be able to think in it directly:

```text
Use the Project bootstrap Method.
Use the accessibility review Method.
Use the research-source verification Method.
Use this Project's release Method.
```

The Factory may therefore address `MethodRef` in Project, Context and Run state. In current
serialized compatibility projections, that field carries the ordinary Skill reference for a
METHOD-classified Skill; it is not a Factory-owned Method resource identity.

This does not make the Factory the universal Method source owner.

- Central / ProjectCentral may host human-authored personal and Project Method source.
- product repositories may ship product-owned Methods.
- AIKit owns indexing, eligibility, resolution, overlays, SkillSet relation, harness projection and operational Explain/History.
- Factory owns developmental use, Run attribution, evaluation and returned praxis evidence.
- O:I may compose/project these relations at whole-suite/shared-world level without replacing native owners.

---

## 3. Praxis enters Context through Focus

Canonical Factory Context remains:

```text
Context = OperativeWorld + InformationHorizon + CurrentFocus
```

Praxis should be resolved in relation to that Context.

```text
Project
  + Profile / scopes
  + Agent / Agency
  + Run
  + QL position / use type
  + Focus
        ↓
resolved Capability horizon
resolved SkillSets
eligible Methods
relevant ContextSources
        ↓
selected Method / overlays
        ↓
act
```

A broad repertoire can remain addressable while only the Method and material required for the present Focus are loaded or projected.

The distinction is important:

```text
Skill exists       != Skill is trusted
Skill trusted      != Skill is selected
SkillSet selected  != Skill is loaded
Skill loaded       != Method requires it now
Method selected    != Action authority exists
Method used        != Method worked
```

---

## 4. Project ontology and operative vocabulary

Factory Project understanding already preserves authored Ground, current implementation, Claims/Evidence and Project language as distinct sources/standings.

Praxis should use those distinctions when they are real.

A Project Method may therefore state, for example:

```text
establish current Focus
inspect active Claims
retrieve Evidence from relevant ContextSources
invoke eligible Actions / Skills
return changed Artifacts + Claims + Evidence + Decisions
```

For a Project with a domain ontology, the same principle applies to domain-specific terms.

The goal is **operative vocabulary continuity**: the vocabulary an Agent reasons with should, where possible, point toward real Project resources, operations and return objects rather than requiring avoidable translation between prompt-language and product-language.

This is conditional rather than totalising. A term belongs in a Method because it is useful to the act and grounded in the Project, not because every available primitive must be mentioned.

---

## 5. Personal → Project composition

The intended recursive composition is:

```text
personal authored Ground / governance / praxis
                    ↓
                 Project
                    ↓
Project Ground / ontology / governance / praxis
                    ↓
           Profile / ContextResolution
                    ↓
              Run / Focus
```

A Project can specialise a person's standing praxis without copying every personal Skill or Method into the Project.

A Project Method may:

- refer to globally reusable SkillRefs;
- add Project-specific UsageOverlays;
- add Project-local Skills;
- bind Project ContextSources;
- require Project-specific evidence or return forms;
- select a narrower SkillSet for target projection.

This relation is one reason Project bootstrap should recover human intent, Project language, existing procedures and capability needs before generating a pile of generic instructions.

---

## 6. Capability matrices as praxis maps

The Factory capability matrix should treat praxis as more than a presence/absence list.

A useful matrix row can relate:

```text
Capability / Skill / Method
owner + provenance
QL position affinity
use type / task family
Project/domain relevance
required ContextSources / Actions
model / harness conditions
Agency / authority conditions
material / Workcell conditions where relevant
verification expectation
observed fitness evidence
```

QL alignment remains an architectural/research aid rather than an activation authority. Ordinary correctness must not require a live QL provider.

The matrix should help answer both forward and returned questions:

```text
What praxis appears germane to this position?
What capability is missing?
What was actually used?
Under which model/harness/world conditions?
What evidence says it helped or hurt?
What should change in the next Ground?
```

---

## 7. Factory evidence for praxis

A Run should be able to retain enough stable refs to reconstruct the praxis conditions under which a result was produced.

Where applicable, Run / Execution evidence should carry or resolve:

```text
MethodRef
SkillRefs actually selected/used
SkillSetRefs / Profile relation
UsageOverlay refs or immutable digests
ContextSourceRefs materially consulted
ActionRefs invoked
Agent / Agency
model / harness / runtime condition
QL position / use type / Focus
Artifacts
Claims
Evidence
verification result
human Recognition / user-experience return where obtained
```

This does not require every tiny Run to emit a giant receipt. The smallest sufficient evidence should be retained for the experiment or consequential decision at hand.

The crucial research distinction is:

> **Skill availability is configuration; Skill/Method fitness is returned evidence.**

The Factory must not infer praxis quality from frequency alone.

---

## 8. Praxis refinement and Return

Returned reality can pressure praxis through a recognisable path:

```text
UsageOverlay repeatedly useful
        ↓ review
Project Method
        ↓ evidence across contexts
reusable Method / Skill candidate
        ↓ recognition + source ownership
accepted reusable praxis
```

Other returns are equally legitimate:

```text
Method fails under a harness
Skill conflicts with Project ontology
extra instruction adds no value
verification disproves an assumed workflow
human experience rejects the technically successful result
```

These may lead to adaptation, narrowing, deletion or rejection.

Nothing about recurrence alone grants authored status.

The same Ground → act → Return relation that governs Project development therefore also governs the development of praxis itself.

---

## 9. Factory consequence

The Software Factory is opinionated in a precise sense.

It does not claim that every agentic development world must use one workflow. It does provide a native pattern for making multi-Agent development, project intention, capabilities, evidence and return visible enough to compose and study.

The praxis clarification extends that pattern:

> **The Factory should make not only what the Agent did, but the developed practice through which it acted, available to evidence and Recognition.**

That is how an open possibility space can still develop stronger methods rather than treating every arrangement as equivalent by default.
