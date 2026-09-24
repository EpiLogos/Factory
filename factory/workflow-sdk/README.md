# Factory workflow authoring

Author an ordinary commission as inspectable TypeScript data. Factory compiles it
into its existing WorkflowUnit / RunMap / Execution Intelligence / attempt model;
this package does not own a scheduler, identity registry, authority or model client.
QL, Epi-Logos, Claude, DSH and an Expression renderer are not required.

## Install and inspect

Build the native binary from this repository with `cargo install --locked --path factory`.
The binary embeds the public types, data builders, native schema and examples:

```sh
factory workflow sdk ./factory-workflow-sdk
npm install --ignore-scripts ./factory-workflow-sdk
factory workflow check ./factory-workflow-sdk/examples/single.workflow.ts --json
factory workflow compile ./factory-workflow-sdk/examples/plural.workflow.ts --json
```

The local package is named `@epilogos/factory-workflow`. No npm registry publication
is presumed. Authoring and native validation do not need Node; TypeScript is an
optional editor/build-time type checker. `workflow sdk` only creates a new target
directory and refuses to overwrite it. `workflow schema` returns the embedded schema.

## Restricted source language

Use `defineWorkflow`, `unit`, `barrier` and `nesting` from this package, literal
objects/arrays/strings/finite JSON numbers/booleans/null, previously declared
`const` values and named literal relative `.ts` data imports. Optional Factory
`import type` declarations, aliases, type annotations, `as const` and `satisfies`
are accepted. End with one `export default`. Imports stay under the explicit
`--root` (the entry's directory by default); symlinks, cycles and escaping paths
are rejected. All supplied modules are parsed, not evaluated.

Functions, arbitrary calls, expressions, property access, spreads, templates,
loops, npm/URL imports, dynamic imports, side-effect imports and ambient globals
are rejected with source locations. There is no transpile-and-evaluate fallback.
The compiler cannot read an environment variable or obtain filesystem, network,
clock, random or credential access on behalf of the submitted code. The trusted
loader reads only the selected bounded source/import bundle. Maximums: 64 modules,
512 KiB per module, 2 MiB total, 64 levels, 32,768 expanded data nodes.

Domain adapters must use the same native source/attempt route. Unrecognised domain
syntax or a required interpretation without an implemented binding is refused;
a type declaration is not an available execution capability.

Registered domain type modules are the one exception to "no packages". The
registry (`factory workflow help --json` → `domainAdapters`) currently admits
QL's `@epilogos/ql-vak`: `import type { CPrime } from "@epilogos/ql-vak"` lets a
unit carry `composition: {...} satisfies CPrime`, which Factory lowers to its
native `CPrimeExecutionBinding` and validates against the unit and the compiled
topology. Adapters supply types only; any other domain import is refused.

## Source, compilation and execution are different

`source.ref` identifies the authored workflow and `source.revision` its explicit
native semantic edition, not the byte hash. Factory computes `source.digest`; do not supply it in
TypeScript. Exact source bytes, transitive import digests, compiler/schema basis
and source locations are separately retained in `source.authoring`.

Formatting or comments change the byte revision, not the native semantic digest
or compiled unit identities. Execution-relevant fields do affect that digest.
Provider/session changes and retries do not create a new semantic unit; each
attempt is still separate. A retry grant does not authorise a provider/model
replacement: the current native owner must admit that choice. A bare changed
provider in a Retry is refused, not silently accepted as re-resolution. Editing a file cannot rewrite an admitted or completed
attempt. `source.successorOf: {ref, revision, digest}` explicitly refers to an exact
retained original when commissioning a new source identity and new Run.

Participant requirements name native Agents/AgentSets/Agencies; they do not mint
or resolve them. Example refs are declared specimen inputs, not installed actors.
An optional unit `contribution` supplies a description, qualified role source,
explicit child context refs, required tools/actions/modalities and optional exact
harness/model requirements. These enter native delegation and ExecutionDemand;
missing delivery requirements refuse attempt admission. Parent context and Skills
are not inherited by assertion. Tools/Skills are not grants; actual owner admission
and provider observation remain separate from a configured arrangement.

## Commission and inspect through Factory

Create a source-qualified generic `factory.commission-request/v1` with ordinary
`participantRequirements` and a `rootAct`. `examples/commission.json` is a complete
request for `examples/single.workflow.ts`. Its identities and timestamp are
explicit specimen inputs: replace them with the actual commissioning basis. The legacy Central Guardian composition is not required.

```sh
factory workflow commission ./factory-state.json ./commission.json ./work.workflow.ts --json
# Use the runRef in the returned native Commission.
factory workflow inspect ./factory-state.json 'run:…' --json
factory workflow inspect ./factory-state.json 'run:…' --unit 'inspect-source' --json
factory workflow inspect ./factory-state.json 'run:…' --attempt 'attempt:…' --json
factory workflow locate ./factory-state.json 'artifact:…' --json
factory workflow source ./factory-state.json 'run:…' work.workflow.ts --json
```

Commission atomically admits the source into the existing native state before
attaching the normal attempt coordinator. It does not start a provider. Normal
`factory attempt` Actions own start/fork/dispatch/verification/retry/cancellation/
Return. Use the actual AIKit/Actuation/Workcell/Central owner contracts and current
configuration; do not turn a declared source ref into a body or permission.

Inspection omits original source bodies and transport payloads by default. It
retains source → unit → attempt → Return and the reverse relation. Explicit
`workflow source` reads the original retained module, not today's file contents.
A cursor is pinned to native revision and selection; a changed reading requires
an explicit refresh, not silent replacement of historical selection. Unknown
usage/model observations are not filled from source intent or guessed pricing.

## Evidence boundary

Repository tests exercise parsing, compiler identity, atomic admission, native
attempt state changes, refusal and navigation. Controlled owner receipts in those
tests do not establish a real ACP model, actual child uptake, rendered creative
quality or human acceptance. The subsequent live provider commission is a distinct,
operator-authorised proving stage; it must never silently fall back to a fixture.
