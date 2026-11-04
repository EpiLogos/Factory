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

## Read the score and edit the song

```sh
factory workflow score ./factory-state.json 'run:…' --json
factory workflow edit ./factory-state.json 'run:…' ./edit.json --json   # or - to read the edit on stdin
```

`score` (`factory.workflow-score/v1`) reads the song a Run performs as one score:
its units with their compositions (`heads`: CPF, CT, CP, CF, CFP, CS, direction;
null for a unit with none), each unit's `row` (who does it: its first agent, else
its first agent set, else its first agency), the `rows` (every agent, agent set
and agency the units name), its `joins` (the join the Run keeps, else `all`), its
`waves` (`factory development wave read`) and each unit's `leg`. It reads the
song's own file at `<project root>/<entry>` while that file is the same song
(`song.edition: "file"`, with its `byteRevision`; `edited` marks the units the file
changed and `song.performed` says whether the Run still performs the file's
edition). A song commissioned from another root, a file that holds another song
or does not compile reads the retained edition (`"retained"`) with the reason in
`song.why` and the compiler's word in `check`.

`edit` writes that file in place. The request is `factory.workflow-edit/v1`:

```json
{"contract": "factory.workflow-edit/v1", "expectedRevision": "blake3:…",
 "edits": [{"op": "allocate", "units": ["verify"], "to": {"kind": "agent", "ref": "agent/eros"}},
           {"op": "connect", "unit": "integrate", "waitsOn": "verify"},
           {"op": "heads", "unit": "verify", "heads": {"CS": "CS3"}}]}
```

`allocate` gives units to an agent (its `agentRefs`) or an agent set (its
`agentSetRefs`, the agents cleared) and moves a composition's `actor` with them;
`connect` adds a dependency; `heads` rewrites a composition's heads (a unit with
no composition is refused: compose it in the song first). Each edit is spliced
into the authored text at the field the compiler located, so comments and the rest
of the file stand; a field the song leaves out is written into its object. The
edited song is compiled apart before the file changes, and the receipt
(`factory.workflow-edit-receipt/v1`) carries the new byte revision. Refusals keep
the file as it was: `workflow.edit_stale` (the file changed since it was read),
`workflow.edit_shared` (the field is one const several fields share),
`workflow.edit_elsewhere` (it is written in another module),
`workflow.edit_by_hand` (it holds comments), `workflow.song_unplaced` (no file
under the project root holds the song), and the compiler's own diagnostics. The
Run keeps performing the edition it was commissioned with; the song's next Run
performs the edited one. Nothing is executed.

## Songs: one clip, made again, used whole

```sh
factory workflow export ./factory-state.json release.workflow.ts [--name release-check] [--rows agent/eros,agent-set/anima] [--units gather,check] --json
factory workflow songs  ./factory-state.json --json
factory workflow remake ./factory-state.json release-check --json
```

`export` renders the song in `<project root>/<entry>` — whole, or the rows (the
tracks: units whose row is one of them) and units (the range) chosen — once into
one clip, `factory.song-clip/v1`, kept in the project at
`.factory/songs/<name>.song.json` (the name is the song's `workflowKey` unless
`--name` gives one). The clip is a render: `clipRef` is
`factory-song:<projectKey>/<name>`; `set` names what it was made from (`entry`,
`sourceRef`, `sourceRevision`, `workflowKey`, the `byteRevision` it was rendered
at, `semanticDigest`); `selection` keeps the rows and range; `score` carries the
chosen units as the score draws them (`key`, `row`, `rowKind`, `dependencies`
inside the selection, `heads`, `concern`, `requiredDifference`, `returns`,
`returnAddress`, `subjectRef`, `praxisRefs`, `capabilityRefs`, `success`, `may`,
`stop`, `escalate`), the `rows` they stand on, the `joins` among them and the
ties the selection `cut`; `render` counts the renders. A clip is never edited in
place: a second export of a name the project keeps is refused
(`workflow.song_rendered`), and `remake` makes it again from its set as that set
now stands, with the same rows and range (`render.renders` + 1,
`previousByteRevision`). `songs` (`factory.song-clips/v1`) lists the project's
clips with each one's `standing`: `current`, `moved` (the set's bytes changed
since the render: remake it), `missing`, `another-song`, `does-not-compile`,
`imported` or `unreadable`, and `why`.

A clip dropped on a track is one unit of the receiving song with the thread form
canon, written by `workflow edit` (above):

```json
{"contract": "factory.workflow-edit/v1", "expectedRevision": "blake3:…",
 "edits": [{"op": "canon", "clip": "/…/.factory/songs/release-check.song.json",
            "to": {"kind": "agent", "ref": "agent/eros"}, "waitsOn": ["verify"], "key": "release-check"}]}
```

`clip` is the clip's file in this project or another. The unit is appended to the
song's `units` in the song's own form (its first unit's builder call and
indentation), allocated to `to` (an agent or an agent set), waiting on `waitsOn`,
keyed `key` (default the clip's name, made unique), its `subjectRef` the clip's
ref in the receiving project and its words the song's: what its last units must
make true and return, everything its units carry and may do, stopping when the
song returns. A clip from another project is imported first — a copy kept under
the receiving project's songs with `importedFrom` naming where it came from —
written only once the song admits the unit, and an import is never remade where
it was imported (`workflow.song_import`). The receipt adds `added` (`key`,
`subjectRef`, `form: "canon"`) and `imports` (the paths written). The score marks
a unit whose `subjectRef` is a `factory-song:` ref `form: "canon"` with its
`songClip` and draws the clip's `score` inside it as `song` (or says why not in
`songWhy`); a clip rendered from a song that holds canons carries their songs the
same way, four deep. Refusals keep the song and the project as they were:
`workflow.song_missing`, `workflow.song_invalid`, `workflow.song_name`,
`workflow.song_selection`, `workflow.song_unplaced`, `workflow.edit_unit` (an
unknown `waitsOn` or a `key` the song holds), `workflow.edit_invalid` (an agency),
`workflow.edit_stale`. Nothing is executed; the song keeps no schedule — a Run, a
repeat or a routine performs it again.

A song runs again from its canon unit, each run a take:

```bash
factory workflow perform ./factory-state.json run:… release-check --json
```

`perform` (`factory.song-take/v1`) starts a Run of the song the canon unit names:
the set its clip names, as it was rendered (read in this project, or for an import
in the project that made it). A song's edition runs once in a developmental state,
so each take performs an edition of its own — the set's modules byte for byte but
for the source's `ref`, which Factory mints for the take, and `successorOf`, which
names the set. The take is commissioned as the Run the canon unit stands in was
(the same project, participants and root act) under the request
`<that Run's request>:take/<unit>/<n>`; the receipt carries `take`, `clipRef`,
`set`, the commission and the take Run's attempts, and the score lists a canon
unit's `takes` (`take`, `runRef`) in order. Refused, with nothing commissioned:
`workflow.song_take` (no such unit, a unit that is no song, another project's clip,
a clip of chosen rows or units — a Run performs a whole song — or a Run Factory did
not commission) and `workflow.song_set` (the set has moved since the render, is
missing, does not compile or is another song now: remake it first). Nothing
executes: Factory's attempt Actions start the take's units as any Run's.

## Evidence boundary

Repository tests exercise parsing, compiler identity, atomic admission, native
attempt state changes, refusal and navigation. Controlled owner receipts in those
tests do not establish a real ACP model, actual child uptake, rendered creative
quality or human acceptance. The subsequent live provider commission is a distinct,
operator-authorised proving stage; it must never silently fall back to a fixture.
