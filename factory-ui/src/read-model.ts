import type { ExecutionTraceView, Status, TraceEvent, TraceSpan } from './types'

export interface WaterfallGeometry {
  spanRef: string
  leftPct: number
  widthPct: number
}

export interface TraceLane {
  laneRef: string
  label: string
  kind: TraceSpan['kind']
  agentRef?: string
  agencyRef?: string
  modelLabel?: string
  spans: TraceSpan[]
}

export function timestamp(value?: string | null): number {
  if (!value) return Number.NaN
  const n = Date.parse(value)
  return Number.isFinite(n) ? n : Number.NaN
}

export function durationMs(start?: string, end?: string, nowMs = Date.now()): number {
  const a = timestamp(start)
  if (!Number.isFinite(a)) return Number.NaN
  const b = end ? timestamp(end) : nowMs
  return Number.isFinite(b) ? Math.max(0, b - a) : Number.NaN
}

export function statusRank(status: Status): number {
  switch (status) {
    case 'fail': return 6
    case 'blocked': return 5
    case 'running': return 4
    case 'queued': return 3
    case 'cancelled': return 2
    case 'success': return 1
  }
}

export function orderedTraces(traces: ExecutionTraceView[]): ExecutionTraceView[] {
  return [...traces].sort((a, b) => {
    const at = timestamp(a.startedAt)
    const bt = timestamp(b.startedAt)
    if (Number.isFinite(at) && Number.isFinite(bt)) return bt - at
    return statusRank(b.status) - statusRank(a.status)
  })
}

export interface TraceSelection {
  execution?: ExecutionTraceView
  span?: TraceSpan
  /** The requested execution is absent; a fallback is supplied only for recovery. */
  executionLost: boolean
  /** The requested span is absent from the selected execution. */
  spanLost: boolean
}

/**
 * Resolve a person-held selection against a refreshed producer snapshot.
 * Identities do not migrate across executions: if an execution disappears, its
 * phase cannot silently become a same-ref phase in an unrelated replacement.
 */
export function resolveTraceSelection(
  traces: ExecutionTraceView[],
  requestedExecutionRef?: string,
  requestedSpanRef?: string,
): TraceSelection {
  if (!traces.length) return { executionLost: false, spanLost: false }

  const requestedExecution = requestedExecutionRef
    ? traces.find((trace) => trace.executionRef === requestedExecutionRef)
    : undefined
  const execution = requestedExecution ?? orderedTraces(traces)[0]
  const executionLost = Boolean(requestedExecutionRef && !requestedExecution)

  // Search only the exact selected execution. A matching span ref elsewhere is
  // a different subject and must remain invisible to the stale selection.
  const span = requestedSpanRef && requestedExecution
    ? requestedExecution.spans.find((candidate) => candidate.spanRef === requestedSpanRef)
    : undefined
  const spanLost = Boolean(requestedSpanRef && (requestedExecution ? !span : executionLost))

  return { execution, span, executionLost, spanLost }
}

export function chronologicalSpans(trace: ExecutionTraceView): TraceSpan[] {
  return [...trace.spans].sort((a, b) => {
    const at = timestamp(a.startedAt)
    const bt = timestamp(b.startedAt)
    if (!Number.isFinite(at) && !Number.isFinite(bt)) return a.name.localeCompare(b.name)
    if (!Number.isFinite(at)) return 1
    if (!Number.isFinite(bt)) return -1
    return at - bt
  })
}

export function deriveLanes(trace: ExecutionTraceView): TraceLane[] {
  const lanes = new Map<string, TraceLane>()
  for (const span of chronologicalSpans(trace)) {
    const key = span.kind === 'agent'
      ? `agent:${span.agencyRef ?? span.agentRef ?? span.ownerLabel ?? 'unknown'}`
      : span.kind
    let lane = lanes.get(key)
    if (!lane) {
      lane = {
        laneRef: key,
        label: span.ownerLabel ?? (span.kind === 'engineer' ? 'engineer' : span.kind),
        kind: span.kind,
        agentRef: span.agentRef,
        agencyRef: span.agencyRef,
        modelLabel: span.modelLabel,
        spans: [],
      }
      lanes.set(key, lane)
    }
    lane.spans.push(span)
  }
  const order: Record<TraceSpan['kind'], number> = { engineer: 0, code: 1, agent: 2, process: 3, other: 4 }
  return [...lanes.values()].sort((a, b) => order[a.kind] - order[b.kind] || a.label.localeCompare(b.label))
}

/**
 * Source-fidelity port of the SSSF waterfall's most important geometric law:
 * tiny phases receive a readable floor, later phases are shifted rather than
 * overlapped, then the whole post-request sequence is normalised into the track.
 */
export function waterfallGeometry(
  trace: ExecutionTraceView,
  nowMs = Date.now(),
  requestZonePct = 16,
  minBlockPct = 3.5,
): WaterfallGeometry[] {
  const spans = chronologicalSpans(trace)
  const request = spans.find((span) => span.kind === 'engineer' && span.startedAt)
  const timed = spans.filter((span) => span.startedAt && span.spanRef !== request?.spanRef)
  const starts = timed.map((span) => timestamp(span.startedAt)).filter(Number.isFinite)
  const requestStart = timestamp(request?.startedAt)
  const traceStart = timestamp(trace.startedAt)
  const t0 = Number.isFinite(traceStart) ? traceStart : (Number.isFinite(requestStart) ? requestStart : Math.min(...starts))
  if (!Number.isFinite(t0)) return []
  const earliestPostRequest = starts.length ? Math.min(...starts) : t0
  const origin = request ? Math.max(earliestPostRequest, t0) : t0
  const ended = timestamp(trace.endedAt)
  const spanEnds = timed.map((span) => {
    const start = timestamp(span.startedAt)
    const end = timestamp(span.endedAt)
    return Number.isFinite(end) ? end : (span.status === 'running' ? nowMs : start)
  }).filter(Number.isFinite)
  const t1 = trace.status === 'running'
    ? Math.max(nowMs, ...spanEnds, origin + 1000)
    : Math.max(Number.isFinite(ended) ? ended : -Infinity, ...spanEnds, origin + 1000)
  const postSpan = Math.max(t1 - origin, 1000)
  const zone = request ? requestZonePct : 0
  const available = 100 - zone - 0.4
  let shift = 0
  let previousEdge = 0
  const provisional: WaterfallGeometry[] = []

  for (const span of timed) {
    const start = timestamp(span.startedAt)
    if (!Number.isFinite(start)) continue
    const rawEnd = timestamp(span.endedAt)
    const end = Number.isFinite(rawEnd) ? rawEnd : (span.status === 'running' ? nowMs : start)
    let left = ((start - origin) / postSpan) * available + shift
    const rawWidth = (Math.max(end, start) - start) / postSpan * available
    if (left < previousEdge) {
      shift += previousEdge - left
      left = previousEdge
    }
    const width = Math.max(rawWidth, minBlockPct)
    shift += width - rawWidth
    previousEdge = left + width
    provisional.push({ spanRef: span.spanRef, leftPct: left, widthPct: width })
  }

  const scale = available / Math.max(previousEdge, available)
  const out = provisional.map((item) => ({
    spanRef: item.spanRef,
    leftPct: zone + item.leftPct * scale,
    widthPct: item.widthPct * scale,
  }))
  if (request) out.unshift({ spanRef: request.spanRef, leftPct: 0.4, widthPct: Math.max(zone - 0.8, minBlockPct) })
  return out
}

export function eventDurationMs(event: TraceEvent): number {
  if (event.toolCall?.durationMs != null) return event.toolCall.durationMs
  return durationMs(event.timestamp, event.endedAt)
}

export function toolEvents(span: TraceSpan): TraceEvent[] {
  return span.events.filter((event) => event.kind === 'tool_call')
}

export function processEvents(span: TraceSpan): TraceEvent[] {
  return span.events.filter((event) => event.kind === 'process')
}

export function nextSpanRef(trace: ExecutionTraceView, current: string | undefined, delta: -1 | 1): string | undefined {
  const spans = chronologicalSpans(trace)
  if (!spans.length) return undefined
  const index = current ? spans.findIndex((span) => span.spanRef === current) : -1
  const next = index < 0 ? (delta > 0 ? 0 : spans.length - 1) : Math.min(spans.length - 1, Math.max(0, index + delta))
  return spans[next]?.spanRef
}
