# Factory Build GUI — SSSF source-fidelity record

Status: implementation evidence for #143 / #144

## Source lock

The execution-UX source is the actual current `disler/super-simple-software-factory` repository, inspected before implementation.

- repository: `https://github.com/disler/super-simple-software-factory`
- pinned revision: `de31374882e7a4e3e5b7bb9bd09e69dc2f779356`
- revision date: 2026-08-02
- visualizer root: `.claude/skills/sssf/apps/visualizer`
- client: Vue 3.5 / Vite 7 / TypeScript
- read-only server: Bun
- storage seam: `sssf.db` / SQLite with WAL-safe reads
- licence: MIT, copyright 2026 IndyDevDan; retained in `factory-ui/THIRD_PARTY_NOTICES.md`

Primary inspected upstream files:

- `src/components/SessionsList.vue`
- `src/components/SessionCard.vue`
- `src/components/SessionTrace.vue`
- `src/components/PhaseDetail.vue`
- `src/components/PhaseDots.vue`
- `src/components/StatusChip.vue`
- `src/components/StatChip.vue`
- `src/lib/api.ts`
- `src/lib/events.ts`
- `shared/types.ts`
- `server/db.ts`
- `server/index.ts`

## Behavioural pin

The pinned source establishes these behaviours as the F0 fidelity target:

1. session/run list sorted newest-first and refreshed on a 500 ms cadence;
2. fixed-density session cards with request, status, phase progress, cost/runtime/tokens and per-agent event chronology;
3. detail view with engineer/code/agent lanes;
4. an exclusive leading request zone, followed by chronological phase blocks;
5. minimum readable phase-block width with later blocks shifted rather than overlapped;
6. queued phases retained visibly;
7. tool calls marked inside their carrying phase;
8. phase selection opening progressive deep detail;
9. tool call rows preserving arguments, result snippet, success/failure, duration and agent attribution;
10. event polling through monotonic SQLite `rowid` cursors, draining `has_more` pages sequentially;
11. gates/envelopes/prompts are side evidence around phase detail, not replacements for the trace;
12. missing/legacy payload fields render as unavailable rather than being fabricated.

The pinned `EventType` is exactly:

`phase_start | phase_end | agent_start | agent_end | tool_call | handoff | gate_pass | gate_fail | log | error`.

**There is no upstream process event at this revision.** F0 therefore does not claim process parity. The destination read model can represent richer native process events for F1/harness-native trajectories, but the SSSF adapter never invents them.

## Source-fidelity map

| Upstream component / behaviour | Treatment | Destination / evidence |
|---|---|---|
| `SessionsList.vue` newest-first session review | mechanically ported | `ExecutionTraceExplorer` session grid; deterministic fixture test |
| `SessionCard.vue` status/request/stats/phase progress | mechanically ported | `SessionCard`; component assertions |
| per-agent card event chronology | mechanically ported | compact event rail grouped by owner |
| `SessionTrace.vue` engineer/code/agent lanes | mechanically ported | `TraceWaterfall` lane grouping |
| request leading zone | mechanically ported | `layoutTraceSpans`, 16% reserved zone |
| readable non-overlapping phase geometry | mechanically ported | `layoutTraceSpans`, 3.5% floor; geometry tests |
| tool ticks inside phase blocks | mechanically ported | `ToolTicks` derived from carrying span events |
| phase click/select | mechanically ported | buttons plus left/right keyboard selection |
| `PhaseDetail.vue` progressive detail | mechanically ported | `PhaseDetail`; collapsed event/tool rows with deep payload expansion |
| tool args/result/error/duration/agent | mechanically ported | `ToolCallRow`; failure/deep-payload tests |
| 500 ms live refresh | isolated, not embedded in presentation component | `SssfHttpClient` + caller-owned polling; UI remains read-model driven |
| rowid cursor pagination | directly preserved in adapter semantics | `drainSssfEvents`; pagination ordering test |
| direct SQLite/Bun server | intentionally replaced | Factory consumes adapters/read models; SSSF storage does not become Factory storage |
| Vue runtime | intentionally replaced | React/TypeScript to fit the O:I Surface/application line |
| SSSF colours/branding | intentionally replaced after behaviour parity | O:I semantic CSS variables with local fallbacks; gold not used as generic success |
| archive write | not ported into F0 | not a canonical Factory Action and therefore not reproduced as a GUI-local mutation |
| process event panel | not present upstream | not claimed in F0; only rendered if a richer native F1 trace explicitly supplies it |

## Identity and authority boundary

SSSF `adw_id`, phase IDs and event IDs remain native evidence identifiers. They never replace canonical Factory `ProjectRef`, `RunRef`, `ExecutionRef`, `AgentRef`, `AgencyRef`, `AgentSessionRef`, Candidate identity or Action identity.

The F0 adapter therefore requires an explicit binding from the SSSF-native record to canonical Factory refs. Native IDs are retained as `nativeRef` / `nativeSpanRef` and as an optional native trajectory link.

## O:I fit

The current O:I desktop line is React-based and exports a shared semantic token layer. Factory does not copy that package into this repository. The Build surface consumes `--oi-*` variables when supplied by the host and has conservative fallbacks for standalone development. This keeps Factory business/read-model ownership here while allowing O:I to own suite placement and house styling.

## Verification target

`cd factory-ui && npm install && npm run verify`

The package tests cover cursor drain ordering, source-shaped adaptation, failed tool rendering, deep payload inspection, phase ordering/geometry, keyboard selection and the deterministic parity fixture. The GitHub workflow runs the same verification command.

## Native workflow source and richer trajectory — 19 September 2026

**Design/acceptance amendment, not additional F0 execution evidence.** [Native workflow authoring](program/NATIVE-WORKFLOW-AUTHORING.md) is the Factory-owned continuation under #195/#197/#199/#145. TypeScript run/workflow definition, its validation/compilation and first authored Run basis are generic Factory functionality. QL/Vāk/Epi are optional interpretations through it; no QL installation or domain role names are required to author work or inspect this GUI.

The same native public reading must connect an authored workflow revision/source unit to its compiled WorkflowUnit, selected Run/frontier, actual execution attempts, emitted tool/model/process/permission events and returned artifacts/evidence. Source and log navigation work in both directions. Planned roles, dependencies and effects remain distinguishable from actual participants, order, resource use and outcomes; retries and successor source revisions retain their earlier evidence.

DSH is the richer native-trajectory case already required by #145: when used, retain its native SessionEvent/trajectory refs and supplied HarnessComposition provenance alongside the portable Factory trace. Actuation/AIKit retain their native ownership; no copied DSH log store, DSH-shaped Run identity or mandatory DSH dependency is introduced. A thinner or DSH-absent harness remains fully operable, with missing native details disclosed rather than simulated. The historical SSSF event list above is not extended retroactively.

Land through the current O:I Desk/Tasks and Run/Agents/Context arrangement: full SSSF view in Desk detail; actual conversation in Tasks; correlated control/trajectory in Run; participants in Agents; authored source and produced work in Context. Use existing panes/popouts for depth. This defines data/action correspondence, not a new layout or competing application.

The new proof must execute a no-QL typed-source workflow and traverse source → compiled unit → actual attempt/tool result → evidence → source. Include stale source, alternate attempts, held history/reconnect and optional QL projection. Existing F0 component/fixture evidence remains valid at its own scope and does not certify this new end-to-end join.
