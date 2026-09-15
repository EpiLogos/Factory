import { useMemo, useState } from 'react'
import { chronologicalSpans, orderedTraces } from '../read-model'
import type { ExecutionTraceView } from '../types'
import { PhaseDetail } from './PhaseDetail'
import { SessionCards } from './SessionCards'
import { TraceWaterfall } from './TraceWaterfall'

function totalRuntime(trace: ExecutionTraceView): string {
  if (!trace.startedAt) return '—'
  const start = Date.parse(trace.startedAt)
  const end = trace.endedAt ? Date.parse(trace.endedAt) : Date.now()
  if (!Number.isFinite(start) || !Number.isFinite(end)) return '—'
  const ms = Math.max(0, end - start)
  return ms < 60_000 ? `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s` : `${Math.floor(ms / 60_000)}m ${Math.round(ms % 60_000 / 1000)}s`
}

export function ExecutionTraceExplorer({ traces, initialExecutionRef }: {
  traces: ExecutionTraceView[]
  initialExecutionRef?: string
}) {
  const ordered = useMemo(() => orderedTraces(traces), [traces])
  const firstRef = initialExecutionRef && ordered.some((trace) => trace.executionRef === initialExecutionRef)
    ? initialExecutionRef
    : ordered[0]?.executionRef
  const [executionRef, setExecutionRef] = useState<string | undefined>(firstRef)
  const selected = ordered.find((trace) => trace.executionRef === executionRef) ?? ordered[0]
  const [selectedSpanRef, setSelectedSpanRef] = useState<string | undefined>(() => selected ? chronologicalSpans(selected)[0]?.spanRef : undefined)

  function selectExecution(ref: string) {
    const trace = ordered.find((item) => item.executionRef === ref)
    setExecutionRef(ref)
    setSelectedSpanRef(trace ? chronologicalSpans(trace)[0]?.spanRef : undefined)
  }

  if (!selected) return <section className="fb-build-surface"><p className="fb-empty-state">no executions available</p></section>
  const selectedSpan = selected.spans.find((span) => span.spanRef === selectedSpanRef)

  // The explorer is an exported standalone composition, so its root carries
  // the scope class the package styles are written under.
  return <section className="fb-build-surface">
    <header className="fb-section-head">
      <div><span className="fb-eyebrow">execution review</span><h2>Sessions</h2></div>
      <span>{ordered.length} execution{ordered.length === 1 ? '' : 's'}</span>
    </header>
    <SessionCards traces={ordered} selectedExecutionRef={selected.executionRef} onSelect={selectExecution} />

    <section className="fb-trace-shell">
      <header className="fb-trace-strip">
        <div className="fb-trace-request"><span className={`fb-status fb-status-${selected.status}`}>{selected.status}</span><strong>{selected.request ?? selected.runRef}</strong></div>
        <div className="fb-trace-stats">
          <span>{totalRuntime(selected)}</span>
          <span>{selected.totalTokens?.toLocaleString() ?? '—'} tokens</span>
          <span>{selected.totalCost == null ? '—' : `$${selected.totalCost.toFixed(4)}`}</span>
          {selected.harnessRef ? <code>{selected.harnessRef}</code> : null}
        </div>
      </header>
      <TraceWaterfall trace={selected} selectedSpanRef={selectedSpanRef} onSelectSpan={setSelectedSpanRef} />
      {selectedSpan ? <PhaseDetail span={selectedSpan} onClose={() => setSelectedSpanRef(undefined)} /> : <div className="fb-detail-hint">Select a phase to inspect exact calls and retained evidence. Use ←/→ while the waterfall is focused.</div>}
      <footer className="fb-native-strip">
        <span>execution</span><code>{selected.executionRef}</code>
        <span>run</span><code>{selected.runRef}</code>
        {selected.nativeTrajectory ? <><span>native trajectory</span><code>{selected.nativeTrajectory.kind}:{selected.nativeTrajectory.ref}</code></> : <><span>native trajectory</span><em>unavailable</em></>}
      </footer>
    </section>
  </section>
}
