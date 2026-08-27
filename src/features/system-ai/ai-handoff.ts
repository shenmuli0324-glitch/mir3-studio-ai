import type { GlobalTaskHandoff } from './global-task-handoff'
import type { Mir3BridgeEnvelope } from '@/features/projects/workspace-bridge'
import { parseGlobalTaskHandoff, sanitizeTaskSemanticText } from './global-task-handoff'

export interface DomainDraftHandoff {
  draftId: string
  revision: number
  systemId: string
  validation?: {
    valid: boolean
    diagnostics: string[]
  } | null
  changedResources: string[]
  resourceId?: string | null
}

export interface DevtoolsReturnTarget {
  view: 'devtools'
  projectId: string
  systemId: string
  resourceId?: string | null
  draftId?: string | null
}

export interface VerifiedDevtoolsTarget extends DevtoolsReturnTarget {
  nonce: string
  relativePath?: string | null
  revision?: number | null
}

export interface DevtoolsTargetVerification {
  isKnownSystem: (systemId: string) => boolean
  getResource: (projectId: string, systemId: string, resourceId: string) => Promise<{
    id: string
    systemId: string
    files: Array<{ path: string }>
  }>
  previewDraft: (projectId: string, draftId: string) => Promise<{
    preview: { draft: { revision: number } }
  }>
  validateDraft: (projectId: string, draftId: string) => Promise<{ systemId: string }>
  nonce: () => string
}

export interface AiTaskIdentity {
  projectId: string
  systemId: string
  taskId: string
  sessionId: string
  allowedSystems: string[]
  allowedWriteSystems?: string[]
}

export interface RegisteredGlobalTask extends AiTaskIdentity {
  compositeId: string
  draftIds: string[]
  handoff: GlobalTaskHandoff
  mcpStatus: 'active' | 'disabled'
  mcpError: string | null
  reviewPending: boolean
  updatedAt: number
}

const globalTasks = new Map<string, RegisteredGlobalTask>()
const GLOBAL_TASK_STORAGE_KEY = 'mir3-global-tasks:v1'
const GLOBAL_TASK_MAX_AGE = 24 * 60 * 60 * 1000
export const GLOBAL_WORKBENCH_EVENT = 'mir3:global-workbench-open'

export function registerGlobalTask(task: Omit<RegisteredGlobalTask, 'mcpStatus' | 'mcpError' | 'reviewPending' | 'updatedAt'> & { mcpStatus?: RegisteredGlobalTask['mcpStatus'], mcpError?: string | null, reviewPending?: boolean, updatedAt?: number }): void {
  const registered = parseStoredGlobalTask({
    projectId: task.projectId,
    systemId: task.systemId,
    taskId: task.taskId,
    sessionId: task.sessionId,
    compositeId: task.compositeId,
    allowedSystems: task.allowedSystems,
    allowedWriteSystems: task.allowedWriteSystems ?? task.allowedSystems,
    draftIds: task.draftIds,
    handoff: task.handoff,
    mcpStatus: task.mcpStatus ?? 'active',
    mcpError: sanitizeTaskSemanticText(task.mcpError),
    reviewPending: task.reviewPending ?? false,
    updatedAt: task.updatedAt ?? Date.now(),
  })
  if (!registered)
    throw new Error('GLOBAL_TASK_REGISTRATION_INVALID')
  globalTasks.set(taskKey(registered), registered)
  persistGlobalTasks()
}

export function markGlobalTaskMcpActive(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): RegisteredGlobalTask | null {
  return updateGlobalTaskMcp(identity, 'active', null)
}

export function markGlobalTaskMcpDisabled(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>, reason: unknown): RegisteredGlobalTask | null {
  return updateGlobalTaskMcp(identity, 'disabled', sanitizeTaskSemanticText(String(reason)) ?? 'GLOBAL_TASK_MCP_DISABLED')
}

export function unregisterGlobalTask(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): void {
  globalTasks.delete(taskKey(identity))
  persistGlobalTasks()
}

export function includeGlobalTaskDraft(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>, draftId: string): void {
  const task = globalTasks.get(taskKey(identity))
  if (!task || task.draftIds.includes(draftId))
    return
  task.draftIds = [...task.draftIds, draftId]
  task.handoff.references.draftIds = [...task.handoff.references.draftIds, draftId]
  task.updatedAt = Date.now()
  persistGlobalTasks()
}

export function markGlobalTaskReviewPending(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): RegisteredGlobalTask | null {
  const task = globalTasks.get(taskKey(identity))
  if (!task)
    return null
  task.reviewPending = true
  task.updatedAt = Date.now()
  persistGlobalTasks()
  return task
}

export function registeredGlobalTasks(): RegisteredGlobalTask[] {
  return [...globalTasks.values()]
}

export function restoreGlobalTasks(now = Date.now()): RegisteredGlobalTask[] {
  const stored = readStoredGlobalTasks()
  globalTasks.clear()
  for (const task of stored) {
    if (now - task.updatedAt <= GLOBAL_TASK_MAX_AGE) {
      globalTasks.set(taskKey(task), {
        ...task,
        mcpStatus: 'disabled',
        mcpError: null,
      })
    }
  }
  persistGlobalTasks()
  return [...globalTasks.values()]
}

export function requestGlobalWorkbench(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): void {
  if (typeof window === 'undefined')
    return
  window.dispatchEvent(new CustomEvent(GLOBAL_WORKBENCH_EVENT, {
    detail: {
      projectId: identity.projectId,
      taskId: identity.taskId,
      sessionId: identity.sessionId,
    },
  }))
}

export function registeredGlobalTask(message: Mir3BridgeEnvelope): RegisteredGlobalTask | null {
  const task = globalTasks.get(taskKey(message))
  if (!task || !matchesTaskIdentity(message, task))
    return null
  return task
}

export function matchesTaskIdentity(message: Mir3BridgeEnvelope, identity: AiTaskIdentity): boolean {
  return message.projectId === identity.projectId
    && message.taskId === identity.taskId
    && message.sessionId === identity.sessionId
    && identity.allowedSystems.includes(message.systemId)
}

export function draftHandoffs(message: Mir3BridgeEnvelope, identity: AiTaskIdentity): DomainDraftHandoff[] {
  if (!matchesTaskIdentity(message, identity) || !isSnapshotOrComplete(message.type))
    return []
  const payload = asRecord(message.payload)
  const values = Array.isArray(payload?.domainResults) ? payload.domainResults : []
  const writableSystems = identity.allowedWriteSystems ?? identity.allowedSystems
  return values.flatMap((value) => {
    const handoff = parseDraftHandoff(value, writableSystems)
    return handoff ? [handoff] : []
  })
}

export function returnTarget(message: Mir3BridgeEnvelope, identity: AiTaskIdentity): DevtoolsReturnTarget | null {
  if (!matchesTaskIdentity(message, identity) || !isSnapshotOrComplete(message.type))
    return null
  const payload = asRecord(message.payload)
  return parseReturnTarget(payload?.returnTo, identity.projectId, identity.allowedSystems)
}

export function isGlobalDraftEvent(type: string): boolean {
  return type === 'mir3/globalSession.snapshot' || type === 'mir3/globalSession.resumed' || type === 'mir3/globalSession.completed'
    || type === 'mir3/globalSession.cancelled' || type === 'mir3/bridge.error'
}

export function isGlobalTerminalEvent(type: string): boolean {
  return type === 'mir3/globalSession.completed'
    || type === 'mir3/globalSession.cancelled'
    || type === 'mir3/bridge.error'
}

export function isCompletedGlobalTask(message: Mir3BridgeEnvelope): boolean {
  if (message.type === 'mir3/globalSession.completed')
    return true
  if (message.type !== 'mir3/globalSession.resumed')
    return false
  const payload = asRecord(message.payload)
  return payload?.running === false
    && Array.isArray(payload.nodes) && payload.nodes.length > 0
    && Array.isArray(payload.pending) && payload.pending.length === 0
    && Array.isArray(payload.queue) && payload.queue.length === 0
    && Array.isArray(payload.runningCalls) && payload.runningCalls.length === 0
}

export async function verifyDevtoolsTarget(
  target: DevtoolsReturnTarget,
  handoffs: DomainDraftHandoff[],
  verification: DevtoolsTargetVerification,
): Promise<VerifiedDevtoolsTarget | null> {
  if (!verification.isKnownSystem(target.systemId))
    return null
  let relativePath: string | null = null
  if (target.resourceId) {
    const resource = await verification.getResource(target.projectId, target.systemId, target.resourceId)
    if (resource.systemId !== target.systemId || resource.id !== target.resourceId || !resource.files[0])
      return null
    relativePath = resource.files[0].path
  }
  const reportedDraft = handoffs.find(handoff => handoff.systemId === target.systemId && (!target.draftId || handoff.draftId === target.draftId))
  const draftId = target.draftId ?? reportedDraft?.draftId ?? null
  let revision: number | null = reportedDraft?.revision ?? null
  if (draftId) {
    const [preview, validation] = await Promise.all([
      verification.previewDraft(target.projectId, draftId),
      verification.validateDraft(target.projectId, draftId),
    ])
    if (validation.systemId !== target.systemId || (revision != null && preview.preview.draft.revision < revision))
      return null
    revision = preview.preview.draft.revision
  }
  return {
    ...target,
    draftId,
    relativePath,
    revision,
    nonce: verification.nonce(),
  }
}

function parseDraftHandoff(value: unknown, allowedSystems: string[]): DomainDraftHandoff | null {
  const record = asRecord(value)
  if (!record)
    return null
  const draftId = portableIdentifier(record.draftId, 160)
  const systemId = portableIdentifier(record.systemId, 64)
  const revision = record.revision
  if (!draftId || !systemId || !allowedSystems.includes(systemId) || !Number.isSafeInteger(revision) || Number(revision) < 0)
    return null
  const validationRecord = asRecord(record.validation)
  const diagnostics = validationRecord && Array.isArray(validationRecord.diagnostics)
    ? validationRecord.diagnostics.filter((item): item is string => typeof item === 'string').slice(0, 500)
    : []
  const validation = validationRecord && typeof validationRecord.valid === 'boolean'
    ? { valid: validationRecord.valid, diagnostics }
    : null
  const changedResources = Array.isArray(record.changedResources)
    ? record.changedResources.flatMap((item) => {
        const resource = portableIdentifier(item, 256)
        return resource ? [resource] : []
      }).slice(0, 10_000)
    : []
  return {
    draftId,
    revision: Number(revision),
    systemId,
    validation,
    changedResources,
    resourceId: portableIdentifier(record.resourceId, 256),
  }
}

function parseReturnTarget(value: unknown, projectId: string, allowedSystems: string[]): DevtoolsReturnTarget | null {
  const record = asRecord(value)
  if (!record || record.view !== 'devtools' || record.projectId !== projectId)
    return null
  const systemId = portableIdentifier(record.systemId, 64)
  if (!systemId || !allowedSystems.includes(systemId))
    return null
  const resourceId = portableIdentifier(record.resourceId, 256)
  const draftId = portableIdentifier(record.draftId, 160)
  if ((record.resourceId != null && !resourceId) || (record.draftId != null && !draftId))
    return null
  return {
    view: 'devtools',
    projectId,
    systemId,
    resourceId,
    draftId,
  }
}

function isSnapshotOrComplete(type: string): boolean {
  return type === 'mir3/systemSession.snapshot'
    || type === 'mir3/systemSession.completed'
    || type === 'mir3/globalSession.snapshot'
    || type === 'mir3/globalSession.resumed'
    || type === 'mir3/globalSession.completed'
}

function portableIdentifier(value: unknown, maxLength: number): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > maxLength)
    return null
  if (!/^[\w:./@-]+$/u.test(value) || value.includes('..') || value.includes('\\'))
    return null
  return value
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    return null
  return value as Record<string, unknown>
}

function taskKey(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): string {
  return `${identity.projectId}\u241F${identity.taskId}\u241F${identity.sessionId}`
}

function persistGlobalTasks(): void {
  if (typeof window === 'undefined')
    return
  try {
    window.localStorage.setItem(GLOBAL_TASK_STORAGE_KEY, JSON.stringify({ schemaVersion: 3, tasks: [...globalTasks.values()] }))
  }
  catch {}
}

function readStoredGlobalTasks(): RegisteredGlobalTask[] {
  if (typeof window === 'undefined')
    return []
  try {
    const parsed = JSON.parse(window.localStorage.getItem(GLOBAL_TASK_STORAGE_KEY) ?? '') as unknown
    const record = asRecord(parsed)
    if (record?.schemaVersion !== 3 || !Array.isArray(record.tasks))
      return []
    return record.tasks.flatMap((value) => {
      const task = parseStoredGlobalTask(value)
      return task ? [task] : []
    }).slice(0, 64)
  }
  catch {
    return []
  }
}

function parseStoredGlobalTask(value: unknown): RegisteredGlobalTask | null {
  const record = asRecord(value)
  if (!record)
    return null
  const projectId = portableIdentifier(record.projectId, 160)
  const systemId = portableIdentifier(record.systemId, 64)
  const taskId = portableIdentifier(record.taskId, 200)
  const sessionId = portableIdentifier(record.sessionId, 200)
  const compositeId = portableIdentifier(record.compositeId, 200)
  const allowedSystems = portableIdentifierArray(record.allowedSystems, 64, 33)
  const allowedWriteSystems = portableIdentifierArray(record.allowedWriteSystems, 64, 33)
  const draftIds = portableIdentifierArray(record.draftIds, 160, 64)
  const handoff = parseGlobalTaskHandoff(record.handoff)
  const mcpStatus = record.mcpStatus === 'active' || record.mcpStatus === 'disabled' ? record.mcpStatus : null
  const mcpError = record.mcpError == null ? null : sanitizeTaskSemanticText(record.mcpError)
  const updatedAt = record.updatedAt
  if (!projectId || !systemId || !taskId || !sessionId || !compositeId
    || allowedSystems.length === 0 || allowedWriteSystems.length === 0 || draftIds.length === 0
    || !allowedSystems.includes(systemId) || allowedWriteSystems.some(item => !allowedSystems.includes(item))
    || !handoff || !mcpStatus || handoff.source.projectId !== projectId || handoff.source.systemId !== systemId
    || !sameStringSet(handoff.scope.allowedReadSystems, allowedSystems)
    || !sameStringSet(handoff.scope.allowedWriteSystems, allowedWriteSystems)
    || draftIds.some(draftId => !handoff.references.draftIds.includes(draftId))
    || !Number.isSafeInteger(updatedAt) || Number(updatedAt) < 1) {
    return null
  }
  return {
    projectId,
    systemId,
    taskId,
    sessionId,
    compositeId,
    allowedSystems,
    allowedWriteSystems,
    draftIds,
    handoff,
    mcpStatus,
    mcpError,
    reviewPending: record.reviewPending === true,
    updatedAt: Number(updatedAt),
  }
}

function updateGlobalTaskMcp(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>, status: RegisteredGlobalTask['mcpStatus'], error: string | null): RegisteredGlobalTask | null {
  const task = globalTasks.get(taskKey(identity))
  if (!task)
    return null
  task.mcpStatus = status
  task.mcpError = error
  task.updatedAt = Date.now()
  persistGlobalTasks()
  return task
}

function portableIdentifierArray(value: unknown, maxLength: number, maximumItems: number): string[] {
  if (!Array.isArray(value) || value.length > maximumItems)
    return []
  const parsed = value.flatMap((item) => {
    const identifier = portableIdentifier(item, maxLength)
    return identifier ? [identifier] : []
  })
  if (parsed.length !== value.length)
    return []
  return [...new Set(parsed)]
}

function sameStringSet(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every(value => right.includes(value))
}
