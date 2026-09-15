import { describe, expect, it } from 'vitest'
import liveDshFixtureJson from '../fixtures/dsh-trajectory.json'
import { processEvents, toolEvents } from './read-model'
import type { ExecutionTraceView } from './types'
import { drainSssfEvents, type SssfApi, type SssfEvent } from './sssf'
import { dshMaximalTrace } from './fixtures/factory-build'
import { sssfParityTrace } from './fixtures/sssf-parity'

// Typed so the type system guards the fixture's shape: a field the fixture
// carries that ExecutionTraceView does not declare cannot be asserted on.
const liveDshFixture = liveDshFixtureJson as ExecutionTraceView

describe('SSSF source-fidelity trace conformance', () => {
  it('drains event pages through the insertion-ordered rowid cursor', async () => {
    const cursors: number[] = []
    const events = [
      { rowid: 1, event_id: 'one' } as SssfEvent,
      { rowid: 2, event_id: 'two' } as SssfEvent,
    ]
    const api: SssfApi = {
      session: async () => { throw new Error('not used') },
      events: async (_id, after) => {
        cursors.push(after)
        return after === 0
          ? { events: [events[0]!], cursor: 1, has_more: true }
          : { events: [events[1]!], cursor: 2, has_more: false }
      },
    }

    const result = await drainSssfEvents(api, 'adw-parity-001', 0, 1)
    expect(cursors).toEqual([0, 1])
    expect(result.cursor).toBe(2)
    expect(result.events.map((event) => event.rowid)).toEqual([1, 2])
  })

  it('retains failed tool args, result, duration, attribution and native identity', () => {
    const span = sssfParityTrace.spans.find((item) => item.name === 'test')!
    const failed = toolEvents(span).find((event) => event.toolCall?.ok === false)!

    expect(failed.toolCall?.tool).toBe('bash')
    expect(failed.toolCall?.args).toEqual({ command: 'npm test' })
    expect(failed.toolCall?.result).toBe('1 failed, 12 passed')
    expect(failed.toolCall?.durationMs).toBe(3100)
    expect(failed.toolCall?.agentRef).toMatch(/^agent:/)
    expect(failed.toolCall?.nativeRef).toBe('e-test')
  })

  it('does not invent process material absent from the pinned SSSF event vocabulary', () => {
    expect(sssfParityTrace.spans.flatMap(processEvents)).toHaveLength(0)
  })
})

describe('heterogeneous native trajectory conformance', () => {
  it('keeps DSH-native process evidence additive to portable trace', () => {
    const process = dshMaximalTrace.spans.flatMap(processEvents)
    expect(dshMaximalTrace.nativeTrajectory?.kind).toBe('dsh-session-events')
    expect(dshMaximalTrace.harnessCompositionFingerprint).toBeTruthy()
    expect(process).toHaveLength(1)
    expect(process[0]?.nativeRef).toBe('dsh:event:1050')
    expect(process[0]?.workcellBindingRef).toBe('workcell-binding:preview-01')
  })

  it('keeps target-native permission evidence distinct from a Factory HumanRequest', () => {
    const permission = dshMaximalTrace.spans.flatMap((span) => span.events).find((event) => event.kind === 'permission')
    expect(permission?.payload).toMatchObject({ factoryHumanRequest: false })
  })

  it('keeps an unbound SessionSpace explicit rather than fabricated', () => {
    // The producer reports no SessionSpace for this execution: the trajectory
    // and every event say so with null, and no event carries an invented ref,
    // while the AIKit-owned Surface refs it does have stay attributable.
    expect(liveDshFixture.sessionSpaceRef).toBeNull()
    expect(liveDshFixture.spans.flatMap((span) => span.events).some((event) => event.sessionSpaceRef)).toBe(false)
    expect(liveDshFixture.agentSessionRef).toBe('agent-session/dsh-demo')
    expect(liveDshFixture.surfaceRefs).toContain('surface/aikit/tui')
  })
})
