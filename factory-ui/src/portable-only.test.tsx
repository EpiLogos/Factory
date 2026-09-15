import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import thinTrajectoryJson from '../fixtures/thin-trajectory.json'
import { BuildSurface } from './BuildSurface'
import { SpanDetail } from './components/SpanDetail'
import { factoryBuildFixture } from './fixtures/factory-build'
import type { ExecutionTraceView } from './types'

/**
 * fixtures/thin-trajectory.json is a portable-only harness: no native
 * trajectory link, no harness composition, no native refs on its events.
 * The surface must say what is missing rather than fabricate it.
 */
const thinTrajectory = thinTrajectoryJson as ExecutionTraceView

describe('portable-only harness renders honestly', () => {
  it('names each missing native detail in span detail instead of inventing it', () => {
    render(<SpanDetail span={thinTrajectory.spans[0]!} />)
    expect(screen.getAllByText('run_tests').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('unavailable')).toBeTruthy()
    expect(screen.getByText('portable only')).toBeTruthy()
    expect(screen.getByText(/Unavailable in this trajectory\./)).toBeTruthy()
  })

  it('declares the absent native trajectory in the Build surface provenance strip', () => {
    render(<BuildSurface view={{ ...factoryBuildFixture, trajectories: [thinTrajectory] }} initialDepth="trajectory" />)
    expect(screen.getByText('native trajectory unavailable')).toBeTruthy()
    expect(screen.getByText('native-thin/v1')).toBeTruthy()
    expect(screen.queryByText(/^body /)).toBeNull()
  })
})
