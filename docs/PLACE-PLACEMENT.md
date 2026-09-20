# Room placement for attempts

Status: implemented in Factory as an opt-in material provenance seam
(`factory::attempt_place`, the optional `placeGrant` field on the attempt
record, and the authored `start-serial`/`retry` operations). The Workcell side
of the pinned contract below is represented in this repository by protocol
fixtures; the real Workcell binary is extended in its own repository. No
Factory pipeline stage requests, releases or renews a place automatically
today, and none is proposed here.

## What a place is

Some developmental attempts need a long-lived persistent place to work in —
a room that keeps running while agents, panes and sessions come and go.
Factory does not build rooms. An attempt that needs one asks the Workcell
native owner for it, records what was granted, and ends it deliberately when
the work's semantics call for an end.

The provider order is Workcell's own policy, not Factory's: `auto` means
Herdr first, with a tmux session as the fallback substrate. Factory passes
the policy through and treats whichever provider answered as observed fact.

## The provenance-not-identity law

A place is a room, not a self.

- The grant is **material provenance** in the attempt receipt. Run identity
  lives in typed artifacts: `attempt_ref`, `task_ref`,
  `reserved_execution_ref`. Nothing in the record derives from `pane_id`,
  `session_name` or any other place field.
- A place outliving the attempt is **disclosure, never continuation**. That a
  tmux session is still alive says nothing about whether the attempt is, and
  the attempt's state is never read from the room.
- Ending the place is a **semantic act, never inferred from run state**.
  Factory never kills a room because an attempt failed, returned or timed
  out; release happens when someone chooses to release, with proof.

This is the same law the canon already states for sessions and terminals:

- `docs/canon/wayfinders/03-AGENTIC-EXECUTION-BODY.md:868` — "terminal panes
  are not canonical telemetry".
- `docs/canon/wayfinders/03-AGENTIC-EXECUTION-BODY.md:910` — prohibited
  hidden decision: "pane/tab IDs as AgentSession IDs".
- `docs/canon/wayfinders/03-AGENTIC-EXECUTION-BODY.md:925` — "tmux is
  process/session substrate, not sync protocol".
- `docs/canon/PERSISTENT-AGENCY-MATERIAL-HOSTING-ALIGNMENT.md:145` — "a
  stable semantic ref must not be used to hide a real session replacement,
  runtime-body change, lost state or failed recovery".

A `place_ref` is stable only while the room it names is the same room. If the
room is replaced, that is a new grant and a new provenance entry — never a
quiet rewrite of the old one.

## The pinned Workcell contract

Factory pins `workcell.place-grant/v1`. A Workcell that answers with any
other schema is refused, not normalised. The two calls, exactly as
`attempt_place.rs` issues them (explicit argv, no shell, one bounded
invocation per call, output capped like every native owner transport):

```text
workcell place request --provider auto|herdr|tmux --name <slug>
workcell place release --place-ref <ref> --pid <pid> --start-marker <marker>
```

A successful request prints the grant on stdout:

```json
{
  "schema": "workcell.place-grant/v1",
  "place_ref": "workcell:place:tmux:<socket>:<session_name>",
  "provider": "tmux",
  "session_name": "attempt-room",
  "pane_id": "%3",
  "pane_pid": 4242,
  "process_start_marker": "1747344000.123456-3",
  "created_utc": "2026-09-15T09:30:00Z"
}
```

Herdr grants carry `pane_id`, `pane_pid` and `process_start_marker` as
`null`. Refusals are non-zero exits with an explanatory stderr
(`already-exists`, `provider-cannot-create`, no provider available). Release
proves the pid and start marker before killing; a stale binding is a non-zero
exit naming the mismatch.

## How Factory integrates it

- `factory::attempt_place` — `request_place`, `release_place` and
  `validate_grant` over the pinned contract, bounded through the shared
  native-process transport. A refusal is captured verbatim (operation, exit
  code, stdout, stderr) as typed evidence and is **never implicitly sent
  again**; deciding what follows is an explicit act.
- The attempt record carries `place_grant` as an optional provenance field,
  admitted only through the authored `start-serial` / `retry` operations and
  validated against the pinned contract at the boundary. The grant keeps its
  native snake_case contract names inside the record's camelCase envelope.
- Attempts that do not request a place behave byte-identically to earlier
  state: the field is omitted from serialisation entirely.
- A retry never inherits a room. A place travels only when the retry's author
  explicitly grants one again; Vāk chain starts pass `place_grant: None` for
  the same reason.
- Release uses the grant's own pid and start marker. A grant without proof
  tokens (a Herdr grant, today) is refused before anything is sent — Factory
  does not invent a pid to satisfy the CLI.
- Release refusals, including stale bindings, are surfaced verbatim and once.
  No call site retries.

## What is deliberately not wired

No dispatch, material or orchestration path calls `request_place` or
`release_place` yet. Today the module is the seam: an owner-facing operator
(or a future, explicitly authored pipeline stage) requests a room, authors
the returned grant onto the attempt, and performs the release. The tests pin
the contract so that a later wiring cannot silently drift from Workcell's.

## Open questions

- **Census-driven placement.** Workcell's `workcell.place-census/v1` census
  is named in `attempt_place.rs` as a doc-facing pin but not consumed.
  Whether placement decisions should consult a census (room budget, provider
  health) is open.
- **Herdr release.** The release CLI requires pid + start-marker, which
  Herdr grants do not carry. Either Workcell extends the Herdr grant with
  proof tokens or exposes its own semantic end-of-place call; until then
  Factory honestly refuses (`UnprovableBinding`) rather than killing without
  proof.
- **Where the semantic act lives.** Which stage of an attempt's life may
  perform a release — and whether a retained-material-style disposition
  ("leave the room; the owner will end it") is wanted — is undecided. The
  current seam keeps the decision explicit and outside Factory's inference.
