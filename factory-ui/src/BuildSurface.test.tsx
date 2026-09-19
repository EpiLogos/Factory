import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { factoryBuildFixture } from './fixtures/factory-build'
import type { ExecutionTraceView } from './types'
import { BuildSurface } from './BuildSurface'

describe('Factory Build semantic envelope', () => {
  it('opens at Factory meaning rather than execution logs', () => {
    render(<BuildSurface view={factoryBuildFixture} />)
    expect(screen.getByText('Choose the execution optic that preserves meaning and evidence')).toBeTruthy()
    expect(screen.getByText('Candidate A · DSH maximal')).toBeTruthy()
    expect(screen.getByText('Candidate B · SSSF thin')).toBeTruthy()
    expect(screen.getByText(/contradictions remain visible/)).toBeTruthy()
    expect(screen.queryByLabelText('Execution trace waterfall')).toBeNull()
  })

  it('reports stale execution selection without attaching it to a replacement phase', () => {
    const oldExecution: ExecutionTraceView = {
      ...factoryBuildFixture.trajectories[0]!,
      executionRef: 'execution:removed',
      spans: [{ spanRef: 'span:stale', name: 'removed build', kind: 'agent', status: 'success', events: [] }],
    }
    const replacementExecution: ExecutionTraceView = {
      ...factoryBuildFixture.trajectories[1]!,
      executionRef: 'execution:replacement',
      spans: [{ spanRef: 'span:stale', name: 'different build', kind: 'agent', status: 'success', events: [] }],
    }
    const initialView = { ...factoryBuildFixture, trajectories: [oldExecution, replacementExecution] }
    const refreshedView = { ...initialView, trajectories: [replacementExecution] }

    const { rerender } = render(<BuildSurface view={initialView} initialDepth="trajectory" />)
    expect(screen.getByLabelText('Span detail removed build')).toBeTruthy()

    rerender(<BuildSurface view={refreshedView} initialDepth="trajectory" />)
    expect(screen.getByText(/execution:removed/)).toBeTruthy()
    expect(screen.getByText(/is no longer in this Run/)).toBeTruthy()
    expect(screen.getByText(/choose another execution/i)).toBeTruthy()
    expect(screen.getByText(/is no longer present in/)).toBeTruthy()
    expect(screen.queryByLabelText('Span detail different build')).toBeNull()
  })

  it('keeps semantic, live working-world and trajectory views distinct', () => {
    render(<BuildSurface view={factoryBuildFixture} />)
    fireEvent.click(screen.getByRole('button', { name: 'live' }))
    expect(screen.getByText('Live working world')).toBeTruthy()
    expect(screen.getByText('Mahāmāyā · implementation agency')).toBeTruthy()
    expect(screen.getAllByText(/SessionSpace unavailable/)).toHaveLength(2)

    fireEvent.click(screen.getByRole('button', { name: 'trajectory' }))
    expect(screen.getByText('Trajectory')).toBeTruthy()
    expect(screen.getByLabelText('Execution trace waterfall')).toBeTruthy()
    expect(screen.queryByText('Live working world')).toBeNull()
  })

  it('preserves maximal native DSH evidence and visibly degrades the thinner path', () => {
    render(<BuildSurface view={factoryBuildFixture} initialDepth="trajectory" />)
    expect(screen.getByText(/body sha256:dsh-maximal-cordis-a7f9/)).toBeTruthy()
    expect(screen.getByText(/dsh-session-events/)).toBeTruthy()

    const thinExecutionRef = 'execution:01ARZ3NDEKTSV4RRFFQ69G5FAG'
    const thinCard = screen.getAllByText(thinExecutionRef).map((element) => element.closest('button')).find(Boolean)
    if (!thinCard) throw new Error('thin execution card missing')
    fireEvent.click(thinCard)
    expect(screen.getByText(/native sssf:/)).toBeTruthy()
    expect(screen.queryByText(/sha256:dsh-maximal-cordis-a7f9/)).toBeNull()
    expect(screen.getByText(/pinned SSSF trace does not emit a process event type/)).toBeTruthy()
  })

  it('dispatches canonical Factory Actions without mutating local business state', () => {
    const onAction = vi.fn()
    render(<BuildSurface view={factoryBuildFixture} onAction={onAction} />)
    const buttons = screen.getAllByRole('button', { name: 'recognise Candidate' })
    fireEvent.click(buttons[0]!)
    expect(onAction).toHaveBeenCalledWith({
      actionRef: 'action:01ARZ3NDEKTSV4RRFFQ69G5FAN',
      subjectRef: 'candidate:01ARZ3NDEKTSV4RRFFQ69G5FAC',
    })
    expect(screen.getByText('Candidate A · DSH maximal')).toBeTruthy()
  })

  it('does not present a native Action as available without a native handler', () => {
    render(<BuildSurface view={factoryBuildFixture} />)
    const button = screen.getAllByRole('button', { name: 'recognise Candidate' })[0]!
    expect(button).toHaveProperty('disabled', true)
    expect(button.getAttribute('title')).toContain('Native Action handler unavailable')
  })

  it('keeps durable HumanRequest semantics distinct from transport permission events', () => {
    render(<BuildSurface view={factoryBuildFixture} />)
    expect(screen.getByText('Which applied Candidate should become the recognised Build surface?')).toBeTruthy()
    expect(screen.queryByText('target-native permission response')).toBeNull()
  })

  it('projects positional root agency without manager/worker ontology classes', () => {
    render(<BuildSurface view={factoryBuildFixture} initialDepth="live" />)
    expect(screen.getByText('root agency')).toBeTruthy()
    expect(screen.getByText('root-scope:factory-project-world')).toBeTruthy()
    expect(document.body.textContent?.toLowerCase()).not.toContain('manageragent')
    expect(document.body.textContent?.toLowerCase()).not.toContain('workeragent')
  })
})
