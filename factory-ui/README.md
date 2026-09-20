# Factory Build surface

React/TypeScript implementation for Factory #143–#145. It deliberately has two layers.

## F0 — source-fidelity execution optic

`ExecutionTraceExplorer`, `SessionCards`, `TraceWaterfall`, `PhaseDetail`/`SpanDetail` and `sssf.ts` mechanically port the useful current SSSF visualizer behaviour while isolating SSSF-native storage and IDs behind a read-model adapter.

The pinned SSSF source has no `process` event type. The portable trace can represent richer native process material, but the SSSF adapter does not fabricate it.

## F1 — Factory semantic envelope

`BuildSurface` starts from Project → Run → frontier → Candidate / Claim / Evidence / HumanRequest / Recognition meaning. Live working-world and trajectory are explicit deeper views rather than the default product language.

The package preserves opaque external refs for AIKit SessionSpace/Surface/HarnessComposition, Actuation positional RootScope/MetagencyGrant/Return, Workcell bindings and target-native harness trajectories. It does not re-own those contracts.

The deterministic F1 fixture proves a maximal DeepSeek Harness-shaped trajectory and a thinner SSSF/Pi path through the same Build experience. Rich native evidence remains linked; missing detail stays visibly unavailable.

Canonical mutations are emitted as `{ actionRef, subjectRef }` through `onAction`. The GUI does not maintain a duplicate Run/Candidate mutation store.

When the host omits `onAction`, native Action buttons are disabled with a visible, accessible availability explanation. Depth navigation, execution selection and span inspection remain available. Adding or removing the callback updates Action availability without remounting the surface.

## O:I host fit

The package uses O:I semantic CSS variable names with conservative standalone fallbacks. The O:I host should provide `@epilogos/oi-design-system/tokens.css`; Factory does not copy a second token authority into this repository. Scarce O:I gold is not used as a generic success colour.

The host owns the theme. The surface sets no `color-scheme` and no appearance class of its own; it inhabits whichever of O:I's light or dark grounds the host selects. The package-private status hues (`--fb-ok`, `--fb-error`, `--fb-running`, `--fb-warn`) are `light-dark()` pairs that follow the host's scheme, and the standalone fallbacks follow it the same way.

A library does not style the host document. Every rule in `styles.css` and `build-surface.css` is scoped under `.fb-build-surface`, the root class of `BuildSurface` and of the standalone `ExecutionTraceExplorer`; nothing is applied to `:root`, `*`, `button` or `code` globally.

## Verify

```bash
cd factory-ui
npm install --no-audit --no-fund
npm run verify
```

The GitHub `Factory Build UI` workflow executes typecheck, Vitest and production library build for every change to this package.

See `../docs/GUI-SSSF-SOURCE-FIDELITY.md` for the exact upstream revision, licence, inspected source paths, behavioural inventory and reuse/replacement map.
