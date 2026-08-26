import type { TaskScopeLease } from '@/features/devtools/domain/types'

export interface ScopeLeaseIdentity {
  projectId: string
  taskId: string
  sessionId: string
}

type ScopeLeaseTimer = number | ReturnType<typeof setTimeout>

export interface ScopeLeaseRegistration {
  identity: ScopeLeaseIdentity
  lease: TaskScopeLease
  renew: (lease: TaskScopeLease) => Promise<TaskScopeLease>
  revoke: (lease: TaskScopeLease) => Promise<void>
  now?: () => number
  schedule?: (callback: () => void, delay: number) => ScopeLeaseTimer
  cancelSchedule?: (timer: ScopeLeaseTimer) => void
}

interface ManagedLease extends ScopeLeaseRegistration {
  active: boolean
  timer: ScopeLeaseTimer | null
}

const RENEW_BEFORE_MILLIS = 5 * 60 * 1_000
const MIN_RENEW_DELAY_MILLIS = 1_000
const leases = new Map<string, ManagedLease>()

export function manageScopeLease(registration: ScopeLeaseRegistration): void {
  if (registration.lease.taskId !== registration.identity.taskId) {
    void registration.revoke(registration.lease).catch(() => {})
    throw new Error('SCOPE_LEASE_IDENTITY_MISMATCH: lease belongs to another task')
  }
  stopScopeLease(registration.identity, false)
  const managed: ManagedLease = { ...registration, active: true, timer: null }
  leases.set(leaseKey(registration.identity), managed)
  scheduleRenewal(managed)
}

export function currentScopeLease(identity: ScopeLeaseIdentity): TaskScopeLease | null {
  const managed = leases.get(leaseKey(identity))
  if (!managed || !managed.active)
    return null
  return managed.lease
}

export function includeScopeLeaseDraft(identity: ScopeLeaseIdentity, draftId: string): void {
  const managed = leases.get(leaseKey(identity))
  if (!managed || !managed.active || managed.lease.draftIds.includes(draftId))
    return
  managed.lease = { ...managed.lease, draftIds: [...managed.lease.draftIds, draftId] }
}

export function stopScopeLease(identity: ScopeLeaseIdentity, revoke = true): void {
  const key = leaseKey(identity)
  const managed = leases.get(key)
  if (!managed)
    return
  managed.active = false
  if (managed.timer)
    cancelTimer(managed, managed.timer)
  leases.delete(key)
  if (revoke)
    void managed.revoke(managed.lease).catch(() => {})
}

async function renewScopeLease(managed: ManagedLease): Promise<void> {
  const key = leaseKey(managed.identity)
  if (!managed.active || leases.get(key) !== managed)
    return
  const previous = managed.lease
  try {
    const renewed = await managed.renew(previous)
    if (renewed.taskId !== managed.identity.taskId) {
      await managed.revoke(renewed).catch(() => {})
      throw new Error('SCOPE_LEASE_IDENTITY_MISMATCH: renewed lease belongs to another task')
    }
    if (!managed.active || leases.get(key) !== managed) {
      await managed.revoke(renewed).catch(() => {})
      return
    }
    managed.lease = renewed
    await managed.revoke(previous).catch(() => {})
    scheduleRenewal(managed)
  }
  catch {
    if (managed.active && leases.get(key) === managed)
      managed.timer = scheduleTimer(managed, () => void renewScopeLease(managed), MIN_RENEW_DELAY_MILLIS)
  }
}

function scheduleRenewal(managed: ManagedLease): void {
  if (!managed.active)
    return
  const now = managed.now?.() ?? Date.now()
  const delay = Math.max(MIN_RENEW_DELAY_MILLIS, managed.lease.expiresAt - now - RENEW_BEFORE_MILLIS)
  managed.timer = scheduleTimer(managed, () => void renewScopeLease(managed), delay)
}

function scheduleTimer(managed: ManagedLease, callback: () => void, delay: number): ScopeLeaseTimer {
  if (managed.schedule)
    return managed.schedule(callback, delay)
  return window.setTimeout(callback, delay)
}

function cancelTimer(managed: ManagedLease, timer: ScopeLeaseTimer): void {
  if (managed.cancelSchedule) {
    managed.cancelSchedule(timer)
    return
  }
  window.clearTimeout(timer as number)
}

function leaseKey(identity: ScopeLeaseIdentity): string {
  return `${identity.projectId}\u241F${identity.taskId}\u241F${identity.sessionId}`
}
