export type Status = 'queued' | 'running' | 'success' | 'fail' | 'blocked' | 'cancelled'
/** Mirrors contracts/factory/build-view.schema.json $defs/executionStatus exactly. */
export type ExecutionStatus =
  | 'queued'
  | 'running'
  | 'blocked'
  | 'returned'
  | 'success'
  | 'fail'
  | 'cancelled'
  | 'contract-fixture'
export type ViewDepth = 'semantic' | 'live' | 'trajectory'

export type TraceEventKind =
  | 'phase_start'
  | 'phase_end'
  | 'agent_start'
  | 'agent_end'
  | 'tool_call'
  | 'handoff'
  | 'gate_pass'
  | 'gate_fail'
  | 'log'
  | 'error'
  | 'process'
  | 'permission'
  | 'artifact'
  | 'evidence'
  | 'model'
  | 'actuation'

export interface NativeTraceLink {
  kind: string
  ref: string
  url?: string
  fingerprint?: string
}

export interface ToolCallDetail {
  tool: string
  args?: unknown
  result?: unknown
  error?: string
  ok?: boolean
  durationMs?: number
  agentRef?: string
  nativeRef?: string
}

export interface TraceEvent {
  eventRef: string
  kind: TraceEventKind
  timestamp: string
  endedAt?: string
  name?: string
  parentRef?: string
  spanRef?: string
  projectRef: string
  runRef: string
  executionRef: string
  agentRef?: string
  agencyRef?: string
  actuationRef?: string
  harnessRef?: string
  agentSessionRef?: string
  /** null = the producer reported no SessionSpace bound (see ExecutionTraceView). */
  sessionSpaceRef?: string | null
  workcellBindingRef?: string
  status?: Status
  severity?: 'info' | 'warning' | 'error'
  payload?: unknown
  toolCall?: ToolCallDetail
  nativeRef?: string
}

export interface TraceSpan {
  spanRef: string
  nativeSpanRef?: string
  name: string
  description?: string
  kind: 'engineer' | 'code' | 'agent' | 'process' | 'other'
  status: Status
  startedAt?: string
  endedAt?: string
  attempt?: number
  retries?: number
  agentRef?: string
  agencyRef?: string
  ownerLabel?: string
  modelLabel?: string
  error?: string
  events: TraceEvent[]
}

export interface ExecutionTraceView {
  executionRef: string
  projectRef: string
  runRef: string
  status: Status
  request?: string
  startedAt?: string
  endedAt?: string
  agentRef?: string
  agencyRef?: string
  harnessRef?: string
  harnessCompositionRef?: string
  harnessCompositionRevision?: number
  harnessCompositionFingerprint?: string
  agentSessionRef?: string
  /**
   * Opaque AIKit-owned SessionSpace identity. Factory's execution condition
   * (factory/src/build.rs ExecutionRecord.session_space_ref,
   * developmental_read.rs FactoryExecutionCondition) carries it as
   * Option<String>; a trajectory says "no SessionSpace bound" with null
   * rather than inventing a ref. Consumers treat null and undefined alike.
   */
  sessionSpaceRef?: string | null
  /** AIKit-owned Surface refs from the same execution condition (surface_refs). */
  surfaceRefs?: string[]
  workcellBindingRefs?: string[]
  totalTokens?: number
  totalCost?: number
  spans: TraceSpan[]
  nativeTrajectory?: NativeTraceLink
}

export interface ClaimView {
  claimRef: string
  statement: string
  status: 'standing' | 'challenged' | 'supported' | 'superseded'
  evidenceRefs: string[]
}

export interface EvidenceView {
  evidenceRef: string
  label: string
  assessment?: string
  nativeRef?: string
  producingExecutionRef?: string
}

export interface CandidateView {
  candidateRef: string
  revision: number
  label: string
  status: 'developing' | 'ready' | 'recognised' | 'returned' | 'rejected'
  producingExecutionRefs: string[]
  claimRefs: string[]
  evidenceRefs: string[]
  artifactRefs?: string[]
  previewRef?: string
  tradeoffs?: string[]
}

export interface HumanRequestView {
  humanRequestRef: string
  decisionRef: string
  question: string
  whyHuman: string
  blockedExecutionRefs?: string[]
  evidenceRefs?: string[]
}

export interface LiveAgencyView {
  agencyRef: string
  agentRef: string
  label: string
  position?: 'root' | 'local' | 'participating'
  rootScopeRef?: string
  metagencyGrantRefs?: string[]
  actuationRef?: string
  returnRef?: string
  returnState?: string
}

export interface LiveExecutionView {
  executionRef: string
  agencyRef?: string
  agentRef?: string
  status: ExecutionStatus
  harnessRef?: string
  harnessCompositionRef?: string
  agentSessionRef?: string
  sessionSpaceRef?: string
  surfaceRefs?: string[]
  workcellBindingRefs?: string[]
  nativeTrajectoryRef?: string
}

export interface FactoryActionView {
  actionRef: string
  label: string
  subjectKinds: Array<'run' | 'candidate' | 'execution' | 'human-request'>
  requiredCapabilityRef?: string
}

export interface FrontierView {
  subjectRef: string
  title: string
  mode: 'work' | 'decision' | 'verification' | 'recognition' | 'return'
  summary: string
  closureState?: string
  gateState?: string
}

export interface FactoryBuildView {
  project: {
    projectRef: string
    label: string
  }
  run: {
    runRef: string
    runMapRef: string
    label: string
    status: Status
  }
  frontier: FrontierView
  claims: ClaimView[]
  evidence: EvidenceView[]
  candidates: CandidateView[]
  humanRequests: HumanRequestView[]
  agencies: LiveAgencyView[]
  executions: LiveExecutionView[]
  trajectories: ExecutionTraceView[]
  actions: FactoryActionView[]
}

export interface ActionInvocation {
  actionRef: string
  subjectRef: string
}
