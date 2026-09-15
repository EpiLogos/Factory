# SSSF visualizer source-fidelity map

**Factory tickets:** #143 / #144 / #145 / #18  
**Observed upstream:** `disler/super-simple-software-factory`  
**Pinned visualizer revision:** `de31374882e7a4e3e5b7bb9bd09e69dc2f779356`  
**Upstream path:** `.claude/skills/sssf/apps/visualizer`  
**Licence:** MIT, Copyright (c) 2026 IndyDevDan  
**Machine-readable pin:** `factory-ui/source-integrations/sssf-visualizer.json`

This map is evidence for the requirement that SSSF is an execution-UX source, not visual inspiration. The Factory semantic envelope is intentionally added *around* a mechanically recognisable execution optic rather than substituted for it.

## Observed upstream read model and runtime

At the pinned revision the visualizer is a Vue 3/Vite application backed by a read-only Bun HTTP server over the SSSF SQLite trace database. UI geometry and presentation are derived from table-shaped session/phase/event/agent/gate/envelope rows; the browser does not own execution truth.

The source interaction rhythm is live polling, not a one-shot report:

- sessions refresh every 500 ms;
- live cards tail their own event streams;
- trace detail refreshes every 500 ms;
- event transport uses monotonically increasing SQLite `rowid` as a cursor and drains pages sequentially because the next request depends on the cursor returned by the previous page;
- envelopes and gates are side tables refreshed at meaningful phase/agent boundaries.

The current source event vocabulary is exactly:

```text
phase_start · phase_end · agent_start · agent_end · tool_call
handoff · gate_pass · gate_fail · log · error
```

There is **no process event at this pinned revision**. Factory F0 therefore renders no invented process lane/state. `process` exists only in the wider Factory native-trajectory read model so richer harness/Workcell evidence can be represented in F1 when it actually exists.

## Fidelity map

| Upstream component / behaviour | Treatment | Destination | Conformance evidence |
|---|---|---|---|
| `SessionsList.vue`: latest-first run list; loading/error/empty states; 500 ms refresh | mechanically ported | `SessionCards.tsx` + host data refresh seam | `read-model.test.ts`; structured parity fixture |
| `SessionCard.vue`: fixed triage card with id/request/status, phase dots, per-agent activity and cost/runtime/token summary | mechanically ported, labels adapted to portable execution refs | `SessionCards.tsx` | `BuildSurface.test.tsx`; `sssf-parity.ts` |
| card event tail using `rowid` cursor | directly preserved as source adapter behaviour | `drainSssfEvents()` in `sssf.ts` | `trace-conformance.test.ts` sequential cursor assertion |
| `SessionTrace.vue`: engineer/code/agent lanes | mechanically ported | `deriveLanes()` + `TraceWaterfall.tsx` | `read-model.test.ts` |
| engineer request owns an exclusive 16% leading timeline zone | mechanically ported | `waterfallGeometry()` | `read-model.test.ts`; `source-parity-expected.json` |
| short phases receive 3.5% visual floor | mechanically ported | `waterfallGeometry()` | `read-model.test.ts` |
| later short phases shift right instead of visually overlapping; sequence normalises back into available width | mechanically ported | `waterfallGeometry()` | no-overlap/bounds assertions in `read-model.test.ts` |
| phase/span click selection | mechanically ported | `TraceWaterfall.tsx` | `BuildSurface.test.tsx` |
| keyboard-relative phase traversal | intentional accessibility extension preserving the same ordered selection model | `nextSpanRef()` / `TraceWaterfall.tsx` | `read-model.test.ts` |
| phase-local tool tick marks | mechanically ported | `TraceWaterfall.tsx` | source parity fixture + component tests |
| `PhaseDetail.vue`: status/owner/kind/attempt/error and collapsible deep detail | mechanically ported in a smaller React composition | `SpanDetail.tsx` | `BuildSurface.test.tsx` |
| tool call name, arguments, result/error, duration and attribution | directly preserved | portable `ToolCallDetail` + `SpanDetail.tsx` | `trace-conformance.test.ts` failed-call assertion |
| SSSF native IDs | retained as provenance, intentionally **not** promoted to Factory identity | `nativeRef` / `nativeSpanRef` | binding fixture + conformance tests |
| SSSF sessions/phases as the top-level product ontology | intentionally replaced at F1 | Project / Run / RunMap / frontier / Candidate semantic envelope | `BuildSurface.tsx`, `BuildSurface.test.tsx` |
| SSSF Vue component technology | intentionally replaced, behaviour retained | React 18 package matching current O:I Surface line | package build/typecheck |
| SSSF dark visual tokens | intentionally restyled *after* behavioural port | O:I semantic token names with local fallbacks; package-private status hues follow the host colour scheme (`light-dark()`) so the surface inhabits O:I light and dark | `styles.css`; host owns final token values |
| process/service timeline material | **not present upstream; not fabricated** | absent on SSSF path; optional on richer native trajectory | `trace-conformance.test.ts` proves SSSF absence and DSH presence |

## Factory F1 envelope

The port distinguishes three readings of the same Run rather than mixing them into one log dashboard.

### Semantic view

Default altitude: Project, Run/RunMap, current frontier, Candidate lineage, Claims/Evidence/trade-offs, durable HumanRequests and canonical Actions. Contradictory Evidence remains visible. Recognition/Return Actions are dispatched by stable ActionRef; the view does not mutate canonical business state locally.

### Live working-world view

Read-only attribution of current Agent/Agency/Actuation/Harness/AgentSession/SessionSpace/Surface/Workcell relations. These are externally owned where constitutionally appropriate. Root Agency is projected positionally through `RootScope` and `MetagencyGrant`; there is no `ManagerAgent`, `WorkerAgent`, `RootAgent` or comparable class ontology.

### Trajectory view

The SSSF-derived execution optic becomes progressive drill-down. Factory keeps a portable trace for cross-harness comparison and may additionally link a richer target-native trajectory without flattening it.

The maximal fixture uses DeepSeek Harness as the rich case: target-native session-event references, opaque HarnessComposition revision/fingerprint, native permission evidence, process/service material, Workcell binding and artifact/evidence events remain attributable. The thinner SSSF/Pi case omits data the source cannot prove and says so explicitly.

## Identity and authority rules

The adapter accepts a `SssfBinding` from Factory. This is deliberate: an SSSF `adw_id`, phase ID, agent label, session ID, DSH SessionEvent ID, target plugin/component ID, Workcell binding or endpoint cannot become canonical Project, Run, Candidate, Agent, Agency, Capability, Action or Execution identity merely because it is convenient to render.

Likewise:

- target-native permission requests are not automatically Factory HumanRequests;
- HarnessComposition remains AIKit/target-owned provenance;
- SessionSpace and Surface remain AIKit-owned;
- process/service/binding/storage/network material remains Workcell-owned;
- RootScope/MetagencyGrant/Return meaning remains Actuation-owned;
- the Factory read model correlates these relations but does not acquire their authority.

## Deterministic comparison evidence

`factory-ui/fixtures/source-parity-expected.json` fixes the upstream revision and key structured expectations. `src/fixtures/sssf-parity.ts` provides a deterministic execution specimen including a failed tool call and queued post-failure phase. The tests prove:

- lane and phase ordering;
- source-derived waterfall geometry, minimum block width, non-overlap and bounds;
- deterministic selection traversal;
- failed tool arguments/result/duration/attribution/native ref;
- source-faithful rowid cursor pagination;
- explicit absence of process material in SSSF;
- additive process/native trajectory evidence in the maximal DSH case;
- semantic/live/trajectory separation;
- canonical Action dispatch without local semantic mutation;
- HumanRequest vs transport-permission distinction;
- positional Root Agency without manager/worker Agent classes.

This is structured parity evidence rather than a screenshot imitation test: the behaviours and data relations which determine the visual surface are pinned and asserted directly.
