import type { Mir3BridgeEnvelope } from '@/features/projects/workspace-bridge'

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

interface RegisteredGlobalTask extends AiTaskIdentity {
  draftIds: string[]
}

const globalTasks = new Map<string, RegisteredGlobalTask>()

export function registerGlobalTask(task: RegisteredGlobalTask): void {
  globalTasks.set(taskKey(task), task)
}

export function unregisterGlobalTask(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>): void {
  globalTasks.delete(taskKey(identity))
}

export function includeGlobalTaskDraft(identity: Pick<AiTaskIdentity, 'projectId' | 'taskId' | 'sessionId'>, draftId: string): void {
  const task = globalTasks.get(taskKey(identity))
  if (!task || task.draftIds.includes(draftId))
    return
  task.draftIds = [...task.draftIds, draftId]
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
  return type === 'mir3/globalSession.snapshot' || type === 'mir3/globalSession.completed'
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
