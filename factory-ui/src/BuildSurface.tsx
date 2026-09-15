import { useId, useMemo, useState } from 'react'
import type { ActionInvocation, FactoryBuildView, ViewDepth } from './types'
import { SessionCards } from './components/SessionCards'
import { SpanDetail } from './components/SpanDetail'
import { TraceWaterfall } from './components/TraceWaterfall'
import { chronologicalSpans } from './read-model'
import './styles.css'
import './build-surface.css'

export interface BuildSurfaceProps {
  view: FactoryBuildView
  initialDepth?: ViewDepth
  onAction?: (invocation: ActionInvocation) => void
}

function Ref({ children }: { children: string }) {
  return <code className="fb-ref" title={children}>{children}</code>
}

function ActionButton({ actionRef, subjectRef, label, onAction, unavailableDescriptionId }: {
  actionRef: string; subjectRef: string; label: string; onAction?: (invocation: ActionInvocation) => void; unavailableDescriptionId: string
}) {
  return <button type="button" className="fb-action" disabled={!onAction} aria-describedby={!onAction ? unavailableDescriptionId : undefined} onClick={() => onAction?.({ actionRef, subjectRef })}>{label}</button>
}

export function BuildSurface({ view, initialDepth = 'semantic', onAction }: BuildSurfaceProps) {
  const actionAvailabilityId = useId()
  const [depth, setDepth] = useState<ViewDepth>(initialDepth)
  const [executionRef, setExecutionRef] = useState(view.trajectories[0]?.executionRef)
  const trace = useMemo(() => view.trajectories.find((item) => item.executionRef === executionRef) ?? view.trajectories[0], [executionRef, view.trajectories])
  const [spanRef, setSpanRef] = useState<string | undefined>(trace ? chronologicalSpans(trace)[0]?.spanRef : undefined)
  const selectedSpan = trace?.spans.find((span) => span.spanRef === spanRef)

  function selectExecution(ref: string) {
    setExecutionRef(ref)
    const next = view.trajectories.find((item) => item.executionRef === ref)
    setSpanRef(next ? chronologicalSpans(next)[0]?.spanRef : undefined)
  }

  const candidateActions = view.actions.filter((action) => action.subjectKinds.includes('candidate'))
  const runActions = view.actions.filter((action) => action.subjectKinds.includes('run'))

  return <main className="fb-build-surface factory-build">
    <header className="fb-header">
      <div className="fb-eyebrow">Factory / Build</div>
      <div className="fb-title-row">
        <div><h1>{view.project.label}</h1><p>{view.run.label}</p></div>
        <div className="fb-run-state"><span className={`fb-status fb-status-${view.run.status}`}>{view.run.status}</span><Ref>{view.run.runRef}</Ref></div>
      </div>
      <nav className="fb-depth-tabs" aria-label="Build view depth">
        {(['semantic', 'live', 'trajectory'] as const).map((item) => <button key={item} type="button" className={depth === item ? 'is-selected' : ''} onClick={() => setDepth(item)}>{item}</button>)}
      </nav>
    </header>

    {depth === 'semantic' ? <section className="fb-depth fb-semantic">
      <div className="fb-frontier">
        <div><span className="fb-kicker">current frontier · {view.frontier.mode}</span><h2>{view.frontier.title}</h2><p>{view.frontier.summary}</p></div>
        <div className="fb-frontier-meta"><Ref>{view.frontier.subjectRef}</Ref><span>closure {view.frontier.closureState ?? 'open'}</span><span>gate {view.frontier.gateState ?? '—'}</span></div>
      </div>

      {!onAction && (candidateActions.length > 0 || runActions.length > 0) ? <p id={actionAvailabilityId} className="fb-action-unavailable fb-muted">Actions are unavailable in this view.</p> : null}
      <section><div className="fb-section-head"><h3>Candidates</h3><span>{view.candidates.length} possible realities</span></div>
        <div className="fb-candidates">{view.candidates.map((candidate) => <article key={candidate.candidateRef} className="fb-candidate">
          <div className="fb-card-top"><strong>{candidate.label}</strong><span className="fb-status">{candidate.status}</span></div>
          <Ref>{candidate.candidateRef}</Ref>
          <dl><dt>Executions</dt><dd>{candidate.producingExecutionRefs.map((ref) => <Ref key={ref}>{ref}</Ref>)}</dd><dt>Claims</dt><dd>{candidate.claimRefs.length}</dd><dt>Evidence</dt><dd>{candidate.evidenceRefs.length}</dd></dl>
          {candidate.tradeoffs?.length ? <ul>{candidate.tradeoffs.map((item) => <li key={item}>{item}</li>)}</ul> : null}
          <div className="fb-actions">{candidateActions.map((action) => <ActionButton key={action.actionRef} actionRef={action.actionRef} subjectRef={candidate.candidateRef} label={action.label} onAction={onAction} unavailableDescriptionId={actionAvailabilityId} />)}</div>
        </article>)}</div>
      </section>

      <div className="fb-two-col">
        <section><div className="fb-section-head"><h3>Claims / Evidence</h3><span>contradictions remain visible</span></div>
          <div className="fb-claims">{view.claims.map((claim) => <article key={claim.claimRef}><span className={`fb-status fb-claim-${claim.status}`}>{claim.status}</span><p>{claim.statement}</p><Ref>{claim.claimRef}</Ref><div>{claim.evidenceRefs.map((ref) => <Ref key={ref}>{ref}</Ref>)}</div></article>)}</div>
        </section>
        <section><div className="fb-section-head"><h3>Human requests</h3><span>authorial, not protocol prompts</span></div>
          {view.humanRequests.length ? view.humanRequests.map((request) => <article className="fb-human-request" key={request.humanRequestRef}><strong>{request.question}</strong><p>{request.whyHuman}</p><Ref>{request.decisionRef}</Ref></article>) : <p className="fb-muted">No durable human authorship request is open.</p>}
        </section>
      </div>
      <div className="fb-actions fb-run-actions">{runActions.map((action) => <ActionButton key={action.actionRef} actionRef={action.actionRef} subjectRef={view.run.runRef} label={action.label} onAction={onAction} unavailableDescriptionId={actionAvailabilityId} />)}</div>
    </section> : null}

    {depth === 'live' ? <section className="fb-depth">
      <div className="fb-section-head"><h2>Live working world</h2><span>read-only projection of external owners</span></div>
      <div className="fb-live-grid">
        {view.agencies.map((agency) => <article key={agency.agencyRef} className="fb-live-card"><span className="fb-kicker">{agency.position ?? 'local'} agency</span><h3>{agency.label}</h3><Ref>{agency.agencyRef}</Ref><p>Agent <Ref>{agency.agentRef}</Ref></p>{agency.rootScopeRef ? <p>Root scope <Ref>{agency.rootScopeRef}</Ref></p> : null}{agency.metagencyGrantRefs?.map((ref) => <p key={ref}>Grant <Ref>{ref}</Ref></p>)}{agency.actuationRef ? <p>Actuation <Ref>{agency.actuationRef}</Ref></p> : null}{agency.returnRef ? <p>Return <Ref>{agency.returnRef}</Ref> · {agency.returnState}</p> : null}</article>)}
        {view.executions.map((execution) => <article key={execution.executionRef} className="fb-live-card"><span className="fb-kicker">execution</span><div className="fb-card-top"><Ref>{execution.executionRef}</Ref><span className={`fb-status fb-status-${execution.status}`}>{execution.status}</span></div><p>Harness <Ref>{execution.harnessRef ?? 'unavailable'}</Ref></p>{execution.harnessCompositionRef ? <p>Body <Ref>{execution.harnessCompositionRef}</Ref></p> : <p className="fb-muted">No rich harness composition supplied.</p>}{execution.agentSessionRef ? <p>Session <Ref>{execution.agentSessionRef}</Ref></p> : null}{execution.sessionSpaceRef ? <p>SessionSpace <Ref>{execution.sessionSpaceRef}</Ref></p> : <p className="fb-muted">SessionSpace unavailable / not yet bound.</p>}{execution.surfaceRefs?.map((ref) => <p key={ref}>Surface <Ref>{ref}</Ref></p>)}{execution.workcellBindingRefs?.map((ref) => <p key={ref}>Material binding <Ref>{ref}</Ref></p>)}</article>)}
      </div>
    </section> : null}

    {depth === 'trajectory' ? <section className="fb-depth">
      <div className="fb-section-head"><h2>Trajectory</h2><span>portable trace + native evidence, without flattening</span></div>
      <SessionCards traces={view.trajectories} selectedExecutionRef={trace?.executionRef} onSelect={selectExecution} />
      {trace ? <>
        <div className="fb-trace-provenance"><Ref>{trace.executionRef}</Ref><span>{trace.harnessRef ?? 'harness unavailable'}</span>{trace.harnessCompositionFingerprint ? <span>body {trace.harnessCompositionFingerprint}</span> : null}{trace.nativeTrajectory ? <span>native {trace.nativeTrajectory.kind}: <Ref>{trace.nativeTrajectory.ref}</Ref></span> : <span>native trajectory unavailable</span>}</div>
        <TraceWaterfall trace={trace} selectedSpanRef={spanRef} onSelectSpan={setSpanRef} />
        {selectedSpan ? <SpanDetail span={selectedSpan} onClose={() => setSpanRef(undefined)} /> : null}
      </> : <p className="fb-muted">No trajectory is attached to this Run.</p>}
    </section> : null}
  </main>
}
