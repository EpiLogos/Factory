import { describe, expect, it } from 'vitest'
import parityExpected from '../fixtures/source-parity-expected.json'
import sourcePin from '../source-integrations/sssf-visualizer.json'
import type { ExecutionTraceView } from './types'
import { chronologicalSpans, deriveLanes, nextSpanRef, orderedTraces, processEvents, resolveTraceSelection, toolEvents, waterfallGeometry } from './read-model'
import { sssfParityTrace } from './fixtures/sssf-parity'

describe('execution trace read model', () => {
  it('orders phases chronologically while retaining queued detail last', () => {
    const spans = chronologicalSpans(sssfParityTrace)
    expect(spans.map((span) => span.name)).toEqual(['request', 'plan', 'build', 'test', 'review'])
  })

  it('ports the SSSF readable-block geometry without overlaps', () => {
    const geometry = waterfallGeometry(sssfParityTrace, Date.parse('2026-08-16T20:00:18.000Z'))
    expect(geometry[0]).toMatchObject({ spanRef: 'sssf-span:p-request', leftPct: 0.4 })
    const postRequest = geometry.slice(1)
    for (const item of postRequest) expect(item.widthPct).toBeGreaterThanOrEqual(3.4)
    for (let i = 1; i < postRequest.length; i += 1) {
      const previous = postRequest[i - 1]!
      const current = postRequest[i]!
      expect(current.leftPct + 0.001).toBeGreaterThanOrEqual(previous.leftPct + previous.widthPct)
    }
    expect(geometry.every((item) => item.leftPct >= 0 && item.leftPct + item.widthPct <= 100.01)).toBe(true)
  })

  it('supports deterministic keyboard-relative selection', () => {
    expect(nextSpanRef(sssfParityTrace, undefined, 1)).toBe('sssf-span:p-request')
    expect(nextSpanRef(sssfParityTrace, 'sssf-span:p-request', 1)).toBe('sssf-span:p-plan')
    expect(nextSpanRef(sssfParityTrace, 'sssf-span:p-plan', -1)).toBe('sssf-span:p-request')
  })

  it('orders execution cards newest first', () => {
    const older = { ...sssfParityTrace, executionRef: 'execution:older', startedAt: '2026-08-16T19:00:00.000Z' }
    expect(orderedTraces([older, sssfParityTrace])[0]?.executionRef).toBe(sssfParityTrace.executionRef)
  })
})

/**
 * fixtures/source-parity-expected.json is the structured parity expectation
 * cited by docs/SSSF-SOURCE-FIDELITY.md and source-integrations/
 * sssf-visualizer.json. Every field is asserted against what the read model
 * actually produces from the deterministic SSSF specimen, so the fixture
 * cannot drift from the code it claims to describe.
 */
describe('structured SSSF parity expectation', () => {
  const parityNow = Date.parse('2026-08-16T20:00:18.000Z')

  it('pins the same upstream revision as the source-integration record', () => {
    expect(parityExpected.upstreamRevision).toBe(sourcePin.revision)
  })

  it('derives exactly the pinned lanes: agent phases sharing one agency share one lane', () => {
    expect(deriveLanes(sssfParityTrace).map((lane) => lane.kind)).toEqual(parityExpected.laneKinds)
  })

  it('orders spans as pinned', () => {
    expect(chronologicalSpans(sssfParityTrace).map((span) => span.spanRef)).toEqual(parityExpected.orderedSpanRefs)
  })

  it('gives the engineer request its pinned leading zone', () => {
    const [request] = waterfallGeometry(sssfParityTrace, parityNow)
    expect(request).toEqual({
      spanRef: parityExpected.orderedSpanRefs[0],
      leftPct: parityExpected.requestZone.leftPct,
      widthPct: parityExpected.requestZone.nominalWidthPct,
    })
  })

  it('holds every timed post-request block at or above the pinned minimum width', () => {
    const timed = waterfallGeometry(sssfParityTrace, parityNow).slice(1)
    expect(timed.length).toBeGreaterThan(0)
    for (const item of timed) expect(item.widthPct).toBeGreaterThanOrEqual(parityExpected.minimumBlockPct)
  })

  it('retains the pinned failed tool call', () => {
    const span = sssfParityTrace.spans.find((item) => item.name === parityExpected.failedTool.span)!
    const failed = toolEvents(span).find((event) => event.toolCall?.ok === false)!
    expect(failed.toolCall).toMatchObject({
      tool: parityExpected.failedTool.tool,
      args: parityExpected.failedTool.args,
      result: parityExpected.failedTool.result,
      durationMs: parityExpected.failedTool.durationMs,
      nativeRef: parityExpected.failedTool.nativeRef,
    })
  })

  it('emits process material only if the pinned source supports it', () => {
    const processCount = sssfParityTrace.spans.flatMap(processEvents).length
    expect(processCount > 0).toBe(parityExpected.sourceProcessEventSupported)
  })
})

describe('execution selection continuity', () => {
  const executionA: ExecutionTraceView = {
    executionRef: 'execution:a',
    projectRef: 'project:test',
    runRef: 'run:test',
    status: 'success',
    spans: [{ spanRef: 'span:shared', name: 'old build', kind: 'agent', status: 'success', events: [] }],
  }
  const executionB: ExecutionTraceView = {
    executionRef: 'execution:b',
    projectRef: 'project:test',
    runRef: 'run:test',
    status: 'success',
    spans: [{ spanRef: 'span:shared', name: 'different build', kind: 'agent', status: 'success', events: [] }],
  }

  it('keeps a valid execution and span exactly where the person selected them', () => {
    const selection = resolveTraceSelection([executionA, executionB], 'execution:a', 'span:shared')
    expect(selection.execution?.executionRef).toBe('execution:a')
    expect(selection.span?.spanRef).toBe('span:shared')
    expect(selection.executionLost).toBe(false)
    expect(selection.spanLost).toBe(false)
  })

  it('refuses to reinterpret a stale span on a replacement execution', () => {
    const selection = resolveTraceSelection([executionB], 'execution:a', 'span:shared')
    expect(selection.execution?.executionRef).toBe('execution:b')
    expect(selection.span).toBeUndefined()
    expect(selection.executionLost).toBe(true)
    expect(selection.spanLost).toBe(true)
  })

  it('keeps the selected execution while reporting a removed span', () => {
    const changedExecution: ExecutionTraceView = {
      ...executionA,
      spans: [{ ...executionA.spans[0]!, spanRef: 'span:replacement' }],
    }
    const selection = resolveTraceSelection([changedExecution], 'execution:a', 'span:shared')
    expect(selection.execution?.executionRef).toBe('execution:a')
    expect(selection.span).toBeUndefined()
    expect(selection.executionLost).toBe(false)
    expect(selection.spanLost).toBe(true)
  })
})
