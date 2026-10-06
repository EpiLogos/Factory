# Software Factory

Software Factory is a Rust command-line tool, `factory`. It keeps a file-based record of agent-driven software work: what was asked for and why, the runs and attempts made against it, what each produced, the evidence gathered, and where a person's judgement was asked for and recorded. It holds O:I's **development** facet: how an intention becomes work, and work becomes evidence that can be judged.

The load-bearing word is **Commission**. A Commission is a recorded request for a piece of development: its purpose, its frontier (what is to change), where its runs go, and the agent and scope it is given. Submitting one creates a Project, a Journey (the longer thread for that request) and a first Run, in one atomic write. It runs nothing. Execution happens later, as attempts that Factory dispatches to the products that own it.

## What it does today

Factory stores everything as JSON files in the project, with no database. A project's placement is `.factory/project.json`; its state is `.factory/development-state.json`. Each Run has its own development ledger. Every write takes a lock and is published by atomic replacement.

**Works now** (CI green on `main`; about 600 Rust tests):

- **Commission and read back.** `factory development commission` takes a versioned request and creates or extends Project → Journey → Run. Replaying it is idempotent; a conflicting replay is refused. `factory development commission-read | project | journey | run` read the result back. A successor Run under the same Commission keeps its predecessor.
- **Typed workflows.** You author a workflow as restricted TypeScript data: units, barriers and nesting. Each unit declares what it must change, its permitted effects, its verification obligations and where its result returns. `factory workflow check | compile` parses and compiles it without executing any source. `factory workflow commission` attaches the compiled workflow to a Run. `factory workflow sdk <dir>` writes the type package.
- **Attempts.** `factory attempt prepare | list | task | return | material | receiving | learn` records each attempt at a workflow unit as an immutable record; a retry is a new record, never a replacement. Factory records its intent before it hands work to the owning product. It hands agent tasks to AIKit, agent delivery to Actuation's gateway, and workspaces and places to Workcell.
- **Results for review.** `factory attempt receiving` submits a finished attempt's result to Central's review queue through `ctrl`. A person reviews it in Central. Factory records the outcome as a correlation and does not decide it.
- **Custody of work.** `factory development custody assign | update | list` records which World Position (a Central address such as `central:position:project:Factory:factory-sensing-guardian`) currently carries which piece of work. `factory development current-work --position P` answers none, one or ambiguous, and never guesses.
- **Telemetry.** `factory telemetry status | stats | signals | digest | lookback | day | watch | search` reads Factory's own state together with the GitHub issues and workflow runs named in the project's sensing policy, grouped by civil day. Collecting signals never starts coding work by itself.
- **Disclosure to O:I.** `factory system`, `factory config-contribution` and `factory capabilities`.

**In development, stated plainly:**

- **A whole development Run end to end is unfinished.** Owner-world execution, desktop acceptance and installed-world proof are not established. In Factory's own dogfood state, three Commissions and three Journeys are live, every Run is still at `seeded`, and the one real attempt ended in a provider refusal.
- **Candidates, claims and evidence.** The data model for candidate versions of a project, and for the claims and evidence about them, exists, but no production command writes those records yet. Candidate lineage and evidence grades currently flow only through the Development Field ledger (`factory development field return`).
- **Evidence grades.** Factory grades evidence as D (deterministic owner command), C (conformance), P (provider), M (material/process) and H (human judgement). The O:I proving floor (`oi prove factory`) currently establishes only D and C.
- **Judgement.** A person's judgement that a result belongs to the project (the design calls it Recognition) is never performed by Factory. Receipts, receiving and archive links are explicitly not that judgement.
- **Builds.** The released archives are source components, not prebuilt binaries. `oi` reports that the builds "have not passed physical acceptance".

## How it fits O:I

O:I gives each facet of an agent's world its own product. Factory holds **development**. It owns Projects, Commissions, Journeys, Runs, workflow units, attempts and custody. It records which other product did the work and does not take over that product's identities. Neighbours are reached through their binaries and versioned JSON, with no crate dependencies.

| Neighbouring facet | Where they meet |
|---|---|
| Ground / Central | `factory project setup-central` binds to a Central project (`central.project/v1`). Custody uses Central's World Positions. `attempt prepare` allocates NOW records, and `attempt receiving` submits finished work to Central's review queue (`central.receiving.*`). A person reviews through Central. |
| Agency / Actuation | Attempts are delivered through Actuation's Agency Gateway (`actuation.gateway/v1`). Custody changes are checked against Actuation's current occupancy generation. Telemetry mutations need an Actuation local-authority admission. A Commission never grants Actuation authority. |
| Capability / AIKit | Factory consumes AIKit's model-selection, task-dispatch, encounter-delivery and Routine evidence. `telemetry search` goes through AIKit knowledge. Factory's own skills are projected through AIKit as `skill/factory-native/*`. |
| Environment / Workcell | `attempt material` and place grants use Workcell worlds and places (`workcell.place-grant/v1`). Execution telemetry can carry a Workcell `workcell.resource-usage/v1` observation without making it a Factory fact. A Workcell run can exist without any Factory ancestry. |
| Reflection / QL | Optional. The only link is QL's `@epilogos/ql-vak` workflow type adapter and an optional Vāk orchestration lowering. Ordinary Factory operation does not need QL. |
| O:I | `oi factory …` dispatches to `factory`. `oi work factory` chooses an explicit Commission. `oi prove factory` exercises Factory's self-hosting Commission through the real CLI and keeps the evidence grades separate. |

**In the Cradle.** In the O:I desktop, Factory is a mode. The work surface becomes structured development, with a Desk, Tasks and a Run page (map, trajectory, live view and handoff), built on the fields Factory and its neighbours actually write. The Cradle's kernel reads Factory's developmental state as one of its owner readings.

## Install and quick start

Through O:I (ordinary route):

```sh
oi install software-factory
factory verify
```

From source (developer route; stable Rust):

```sh
git clone https://github.com/EpiLogos/Factory
cd Factory
cargo install --locked --path factory
```

First commands:

```sh
factory project setup . <project-key>                  # place a Factory project here
factory project locate .                               # find an existing placement
factory workflow check factory/workflow-sdk/examples/single.workflow.ts
factory development commission .factory/development-state.json request.json
factory development project .factory/development-state.json <project-ref>
factory telemetry status .factory/development-state.json
```

`factory --help` lists every route. `factory attempt --help` and `factory development custody --help` cover attempts and custody. `factory workflow --help` prints the workflow command set as JSON. `factory conformance developmental-state <out>` writes a test specimen state for contract checks; it is never used for ordinary work.

---

## Why it exists

Its purpose is not merely to make agents write code faster.

Fast agentic implementation creates a specific risk: software can preserve the nouns of a request while deleting the reason those nouns mattered. A system may faithfully implement `Project`, `Run`, `Agent`, `Candidate` or a requested feature and still drift away from the experience, purpose, judgement or human possibility that caused the work to exist.

The Factory exists to keep that relation alive while allowing agents and deterministic machinery to carry much more of the developmental labour.

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

## Runs preserve developmental continuity

A `Run` is one durable intended transformation of a Project. A `RunMap` makes that transformation inspectable across time.

A Run can outlive a terminal session, use several Agents or execution environments, branch into several Candidates, wait for a human decision, return from later work to an earlier assumption, and retain the evidence for why the present Project became what it is.

This is why the Factory is not reducible to an agent loop or CI pipeline. Its subject is **developmental continuity**.

A future human or agent should be able to ask not only:

> What code changed?

but:

> What were we trying to make true, which possibilities were considered, what evidence changed the decision, what was recognised, and what did the Project learn?

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

## Design before blind implementation

The Factory treats development as an intelligible transformation rather than an isolated patch.

That does not mean every small task needs a large planning ceremony. It means the system should preserve enough of the design relation for implementation to be evaluated as an implementation *of something*.

For a trivial change, that determination may be compact. For a larger change it may include experience design, architecture, program design, interfaces, source integration choices and vertical order.

The criterion is sufficiency, not document volume.

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

## The Factory as a Project-understanding system

A mature Project should become easier for both humans and agents to enter over time.

Its authored vision, design, architecture, code, actions, prior Runs, evidence and recognised decisions form a navigable developmental history. The current code tells us what is real now; it does not retroactively tell us why the Project exists. Vision tells us what is meant; it does not prove the implementation works. The Factory keeps both available and lets returned reality revise earlier understanding explicitly.

The durable result is not only code. It is a Project that knows more about itself.

## Current repository: implementation notes

Routine-backed developmental continuation is available through
`factory development admit-routine-continuation` and the corresponding
`routine-continuation` Explain read. It consumes the pinned AIKit owner envelope
without turning Factory into a scheduler or upgrading owner-attested authority;
`factory conformance developmental-state` generates a validated provider state
and all stable CLI locators for downstream contract checks. See
`docs/canon/ROUTINE-JOURNEY-CONTINUATION.md`.

Factory can also commission its own bounded developmental state with
`factory development commission`. The versioned request creates or appends one
Factory-owned Project → Journey → initial Run atomically; replay is idempotent,
conflict is refused, and the caller cannot choose another write owner. Central
AgentSet/profile receipts remain exact membership evidence with the explicit
standing `membership-non-authoritative`: they do not authorize Actuation or
claim execution. `factory development mutate` accepts only the tagged native
workflow/Activity/Return/Recognition correlations, and `commission-read`
exposes the preserved relation. The checked-in O:I specimen was generated by
these native commands and deliberately contains only seeded, ready/planned
work—no Agency, Execution, Activity or Return is inferred. See
`docs/canon/FACTORY-SELF-HOSTING-COMMISSION.md`.

Work is held by stable World Positions (`central:position:<world>:<slug>`, defined
by Central) through `factory development custody assign|update|list`: durable
`factory.work-custody/v1` records in the same developmental state, written under
its lock. Closed custody reopens only with `--reopen`; a hand-off names its
receiver and creates the successor in the same write. `factory development
current-work --position P` answers `none`, `one` or `ambiguous` from every
in-progress custody and every running attempt whose participant names `P` — never
from a capped list, never the most recent — and refuses to guess when relations
name more than one work node or one cannot be resolved. `factory development
inhabitation` projects, per Run, the Positions in custody and the occupant
relations each attempt already records, with absent foreign refs marked absent.
Without a state path these commands use the nearest `.factory/project.json`. See
the O:I World inhabitation contract v1, section 3.


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

### Build file provider (publication mechanics)

The Build-only `FactoryBuildFileProvider` (`factory.build-local-provider-state/v1`)
keeps explicit cached reads while evaluating each Action against current durable
owner state under its file lock, with a two-second acquisition deadline. Bootstrap
refuses an existing destination; mutation delegates to `FactoryActionExecutor`,
publishes a unique synced replacement, then updates the handle and returns the
receipt. The Build and developmental file providers and Run ledger share one
private physical publisher: held directory/source/lock/stage descriptors,
no-follow regular-file admission, exclusive bootstrap, source write admission,
and exact byte/permission/ownership/ACL/xattr readback. Mac and Linux metadata
preservation is bounded to 1,024 xattrs and 4 MiB of attribute data (plus 4 MiB of
Mac ACL text); unsupported metadata refuses replacement. Failed refresh or
pre-publication mutation leaves the prior cached reading intact. A failure after
replacement retains its native source path and original I/O cause through
projected Actions and CLI dispatch. Under `--json`, the existing native failure
route reports `factory.publication-failure/v1` with `published=true`, unknown
outcome and no automatic retry; success schemas stay unchanged. Native readback
is required before another mutation. Failed
publication retains its hidden candidate stage and never deletes a pathname
that could now designate foreign state. Explicit empty v1 fields stay on the
same native record; ambiguous transfer and unsupported nonempty extension data
refuse mutation without erasure. This provider and the Run developmental ledger remain
distinct native stores; neither requires another suite product. The ledger's
current-state transaction and old-writer cutover are specified in
[Project Development Orientation and Recursive Return](docs/canon/PROJECT-DEVELOPMENT-ORIENTATION-AND-RETURN.md#native-ledger-retention-and-mutation).

## Relation to the wider O:I field

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

## Read first

1. [`docs/canon/QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md`](docs/canon/QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md) — telos, document authority, system whole and developmental control body.
2. [`docs/canon/QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md`](docs/canon/QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md) — detailed product architecture and developmental contracts.
3. [`docs/canon/QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md`](docs/canon/QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md) — experienced ontology and primitive relations.
4. [`docs/canon/QL-SOFTWARE-FACTORY-DEEP-QL-INTEGRATION-FOUNDATIONS.md`](docs/canon/QL-SOFTWARE-FACTORY-DEEP-QL-INTEGRATION-FOUNDATIONS.md) — deeper QL framing and the operational-parity boundary.
5. [`docs/ARCHITECTURE-NAVIGATION.md`](docs/ARCHITECTURE-NAVIGATION.md) — Commission, custody, attempt and verification mapped to their source files.
6. [`docs/canon/FACTORY-SELF-HOSTING-COMMISSION.md`](docs/canon/FACTORY-SELF-HOSTING-COMMISSION.md) and [`docs/canon/ROUTINE-JOURNEY-CONTINUATION.md`](docs/canon/ROUTINE-JOURNEY-CONTINUATION.md) — the Commission and Routine-continuation contracts.

The canon documents keep their historical `QL-SOFTWARE-FACTORY-` prefix; ordinary Factory operation does not require QL.

The issue tracker and open PRs are the current development map; they should be read as temporal state, not as retroactive product purpose.

---

## Background

O:I stands for Objective : Internality. It names the means through which a life knows and acts within a world: memory, language, tools, permissions and other people. Those means are internal because every act proceeds through them, and objective because each can be examined and changed. Software Factory keeps one of those means, the developmental history of a project and the reasons for it, available for later people and agents to read and revise. The idea is developed in the essay [*Confronting the Limit: Determination, Subjectivity and Mind as Objective Internality*](https://oi.epi-logos.org/essay/).

The Software Factory is a system for **making agentic software development durable and intelligible from authored intention through design, development, evidence, candidate formation, Recognition and Return**.
