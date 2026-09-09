# Software Factory

The Software Factory is a system for **making agentic software development durable and intelligible from authored intention through design, development, evidence, candidate formation, Recognition and Return**.

Its purpose is not merely to make agents write code faster.

Fast agentic implementation creates a specific risk: software can preserve the nouns of a request while deleting the reason those nouns mattered. A system may faithfully implement `Project`, `Run`, `Agent`, `Candidate` or a requested feature and still drift away from the experience, purpose, judgement or human possibility that caused the work to exist.

The Factory exists to keep that relation alive while allowing agents and deterministic machinery to carry much more of the developmental labour.

## The human telos

The intended shift is not "human out of the loop" in the sense of removing human judgement. It is to move human attention away from work that should not require continuous authorship:

- repeated codebase orientation;
- source discovery;
- routine decomposition;
- implementation mechanics;
- test and verification invocation;
- environment management;
- context reconstruction;
- developmental bookkeeping.

Human attention can then stay nearer the places where human authorship is consequential:

- what is worth making;
- what experience is intended;
- taste and qualitative judgement;
- which of several coherent futures belongs to the Project;
- whether an encountered Candidate is recognisable as what should become real;
- whether returned reality should revise the original intention.

The Factory is successful when greater agentic capability gives the human **more room for vision, judgement and life away from babysitting agent mechanics**, not merely a larger volume of machine-produced changes to supervise.

## The developmental relation

A software change should remain intelligible as a relation among:

```text
authored intention
        ↓
experience / product meaning
        ↓
design and determinate programme
        ↓
agent-led development
        ↓
executable candidate reality
        ↓
evidence and encounter
        ↓
Recognition / redirection
        ↓
Return into Project ground
```

The important word is **relation**. A design document is not valuable because documentation is virtuous in the abstract. It is valuable when it preserves a determination that code can answer to. Evidence is not a gate ritual; it is how a claim about the developed thing meets something other than the producing model's confidence. Recognition is not a ceremonial approval button; it is where a human or authorised governing locus encounters the realised difference and decides what belongs to the durable Project.

## Commission and Recognition

Human participation concentrates around two especially consequential apertures.

### Commission

When the existing Project ground and initiating request already determine the intended change, the Factory should not ask the human to restate it.

When several materially different futures remain open, Commission is the point where human authorship supplies or ratifies the direction. The question belongs with the human because the difference changes what the Project is for or what experience should become true, not because agents are incapable of choosing a technical option.

### Recognition

After development, a Candidate should become directly encounterable with the evidence needed to understand it.

Recognition asks whether that encountered reality belongs to the Project. The answer may be:

```text
recognise this result
return it for further development
prefer another Candidate
accept the failure as a finding
revise the design
revise the original intention
```

A returned failure or unexpected possibility is therefore not merely a failed build. It can be information capable of changing an earlier determination.

## Runs preserve developmental continuity

A `Run` is one durable intended transformation of a Project. A `RunMap` makes that transformation inspectable across time.

A Run can outlive a terminal session, use several Agents or execution environments, branch into several Candidates, wait for a human decision, return from later work to an earlier assumption, and retain the evidence for why the present Project became what it is.

This is why the Factory is not reducible to an agent loop or CI pipeline. Its subject is **developmental continuity**.

A future human or agent should be able to ask not only:

> What code changed?

but:

> What were we trying to make true, which possibilities were considered, what evidence changed the decision, what was recognised, and what did the Project learn?

## Design before blind implementation

The Factory treats development as an intelligible transformation rather than an isolated patch.

That does not mean every small task needs a large planning ceremony. It means the system should preserve enough of the design relation for implementation to be evaluated as an implementation *of something*.

For a trivial change, that determination may be compact. For a larger change it may include experience design, architecture, program design, interfaces, source integration choices and vertical order.

The criterion is sufficiency, not document volume.

## Evidence and Candidates

A `Candidate` is a coherent possible Project reality, not just a diff.

It may include source state, a materialised application or service, claims about the intended behaviour, deterministic checks, screenshots or traces, provenance and the environment in which it was encountered.

Evidence exists to make consequential claims inspectable. Different evidence answers different questions:

- a test can show a deterministic property;
- a runtime observation can show what happened in a particular environment;
- an impact analysis can reveal affected structure;
- a human encounter can answer whether the resulting experience is recognisable;
- an agent review can identify tensions or missing evidence;
- a failed Candidate can falsify a design assumption.

No single evidence kind is universal merely because it is easy to automate.

## Returned reality can revise the plan

The Factory is deliberately not a one-way pipeline.

Later work can reveal that an earlier determination was wrong:

```text
Development
  ├── implementation defect        → revise development
  ├── design mismatch              → return to design
  └── ground assumption false      → return to ground

Application / encounter
  ├── behaviour misses intent      → return to intent
  ├── context was incomplete       → return to ground
  └── new possibility disclosed    → reopen authorship
```

This matters because a development system that can only move downward from instruction to code becomes insulated from the reality it produces. Return makes failure, resistance and discovery productive parts of the Project's future ground.

## The Factory as a Project-understanding system

A mature Project should become easier for both humans and agents to enter over time.

Its authored vision, design, architecture, code, actions, prior Runs, evidence and recognised decisions form a navigable developmental history. The current code tells us what is real now; it does not retroactively tell us why the Project exists. Vision tells us what is meant; it does not prove the implementation works. The Factory keeps both available and lets returned reality revise earlier understanding explicitly.

The durable result is not only code. It is a Project that knows more about itself.

## Relation to the wider {O:I} field

**O:I** is the whole technological-agency field. Factory is its developmental centre: it does not require every Project to adopt the rest of the suite.

**Central** supplies durable human-authored ground across technological change. Factory keeps Project-specific intention and canon with the Project and can return genuinely cross-context durable discoveries for explicit human adoption rather than silently rewriting Central.

**Actuation** owns the constitution of situated Agency, determination, delegation, federation, authority and Return. Factory may commission an `AgenticComposition` for developmental work without redefining those agency semantics as workflow primitives.

**AIKit** resolves the operative world available to an actor — capabilities, sources, models, sessions, runtime bodies and Surfaces. Factory gives those powers a developmental reason and records their provenance in the Run.

**Workcell** materialises the computational world required by development and application: workspaces, processes, services, containers, VMs, hosts, databases, browser surfaces and provider bindings. Factory reasons in Projects, Runs and Candidates; Workcell supplies the actual material embodiment.

Factory's bounded ExecutionTelemetry read can preserve an accepted
`workcell.resource-usage/v1` observation without turning Workcell identities or
metrics into Factory facts. The owner revision and schema digest travel with the
reading; exact replays collapse, conflicting replays fail, and unsupported or
unavailable metrics remain independently explicit. The checked-in owner-evidence
fixture was produced by the Workcell `0b93a4a` native CLI against a real local
process. It proves the consumption seam, not historical usage by the fixture's
illustrative Factory Execution.

**Quaternal Logic** can provide optional formal/refraction faculties and can be used for deeper QL-native development experiments. Ordinary Factory operation must remain valid without QL.

## Quaternal Logic and the Factory

The Factory has a historical and research relation to Quaternal Logic, but the two should not be collapsed.

Earlier Factory design used QL-aligned developmental forms and the Epi-Logos six-agent skeleton as a way to explore archetypal integrity in software. The deeper QL work now has its own native product boundary in `EpiLogos/QL-MEF`, while QL agent-runtime experiments have moved into Actuation.

The Factory remains a principal place where formal claims can become answerable through software development and evidence. That does not make QL terminology proof of architectural quality. Where QL is active, the standard is operational consequence and explicit provenance; where it is disabled, the ordinary developmental system should remain coherent.

## Current repository

This repository now contains several layers with different authority:

```text
docs/canon/
    governing Factory vision, experienced ontology and architecture

factory/ + factory-ui/
    current executable Factory implementation surfaces on main

contracts/
    cross-product and language-neutral contracts where accepted

skills/
    Factory-native operational procedures

seed-docs/ + transcripts/ + sites/
    research/source provenance that informed the design

super-simple-software-factory/
inkwell-agent-sandboxes-and-software-factory/
    retained upstream/reference systems and experiments

ql-agent-experiments/
    historical/experimental QL runtime material; canonical runtime ownership
    has moved to Actuation according to the migration documents
```

The source videos and imported factory systems remain valuable because they preserve where design ideas came from. They are **research provenance, not the definition of the Software Factory product**.

Current `main`, tests and accepted contracts determine implementation truth. Open implementation and integration PRs remain current development state until accepted.

## Read first

1. [`docs/canon/QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md`](docs/canon/QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md) — telos, document authority, system whole and developmental control body.
2. [`docs/canon/QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md`](docs/canon/QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md) — detailed product architecture and developmental contracts.
3. [`docs/canon/QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md`](docs/canon/QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md) — experienced ontology and primitive relations.
4. [`docs/canon/QL-SOFTWARE-FACTORY-DEEP-QL-INTEGRATION-FOUNDATIONS.md`](docs/canon/QL-SOFTWARE-FACTORY-DEEP-QL-INTEGRATION-FOUNDATIONS.md) — deeper QL framing and the operational-parity boundary.

The issue tracker and open PRs are the current development map; they should be read as temporal state, not as retroactive product purpose.
