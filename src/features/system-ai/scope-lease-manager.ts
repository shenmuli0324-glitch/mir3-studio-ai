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
  onError?: (reason: unknown) => void
}

interface ManagedLease extends ScopeLeaseRegistration {
  active: boolean
  timer: ScopeLeaseTimer | null
  renewFailures: number
}

interface PendingRevocation {
  identity: ScopeLeaseIdentity
  lease: TaskScopeLease
  revoke: (lease: TaskScopeLease) => Promise<void>
  now?: () => number
  schedule?: (callback: () => void, delay: number) => ScopeLeaseTimer
  cancelSchedule?: (timer: ScopeLeaseTimer) => void
  onError?: (reason: unknown) => void
  timer: ScopeLeaseTimer | null
  failures: number
}

const RENEW_BEFORE_MILLIS = 5 * 60 * 1_000
const MIN_RENEW_DELAY_MILLIS = 1_000
const leases = new Map<string, ManagedLease>()
const pendingRevocations = new Map<string, PendingRevocation>()

export function manageScopeLease(registration: ScopeLeaseRegistration): void {
  if (registration.lease.taskId !== registration.identity.taskId) {
    void registration.revoke(registration.lease).catch(reason => registration.onError?.(reason))
    throw new Error('SCOPE_LEASE_IDENTITY_MISMATCH: lease belongs to another task')
  }
  const now = registration.now?.() ?? Date.now()
  if (registration.lease.expiresAt <= now) {
    void registration.revoke(registration.lease).catch(reason => registration.onError?.(reason))
    throw new Error('SCOPE_LEASE_EXPIRED: lease already expired')
  }
  void stopScopeLease(registration.identity, false)
  const managed: ManagedLease = { ...registration, active: true, timer: null, renewFailures: 0 }
  leases.set(leaseKey(registration.identity), managed)
  scheduleRenewal(managed)
}

export function currentScopeLease(identity: ScopeLeaseIdentity): TaskScopeLease | null {
  const managed = leases.get(leaseKey(identity))
  if (!managed || !managed.active)
    return null
  const now = managed.now?.() ?? Date.now()
  if (managed.lease.expiresAt <= now) {
    void stopScopeLease(identity).catch(reason => managed.onError?.(reason))
    return null
  }
  return managed.lease
}

export function includeScopeLeaseDraft(identity: ScopeLeaseIdentity, draftId: string): void {
  const managed = leases.get(leaseKey(identity))
  if (!managed || !managed.active || managed.lease.draftIds.includes(draftId))
    return
  managed.lease = { ...managed.lease, draftIds: [...managed.lease.draftIds, draftId] }
}

export function hasPendingScopeRevocation(identity: ScopeLeaseIdentity): boolean {
  const prefix = `${leaseKey(identity)}\u241F`
  return [...pendingRevocations.keys()].some(key => key.startsWith(prefix))
}

export function stopScopeLease(identity: ScopeLeaseIdentity, revoke = true): Promise<void> {
  const key = leaseKey(identity)
  const managed = leases.get(key)
  if (!managed)
    return Promise.resolve()
  managed.active = false
  if (managed.timer)
    cancelTimer(managed, managed.timer)
  leases.delete(key)
  if (!revoke)
    return Promise.resolve()
  return queueScopeRevocation(managed, managed.lease)
}

async function renewScopeLease(managed: ManagedLease): Promise<void> {
  const key = leaseKey(managed.identity)
  if (!managed.active || leases.get(key) !== managed)
    return
  const nowBeforeRenewal = managed.now?.() ?? Date.now()
  if (managed.lease.expiresAt <= nowBeforeRenewal) {
    await stopScopeLease(managed.identity).catch(reason => managed.onError?.(reason))
    return
  }
  const previous = managed.lease
  try {
    const renewed = await managed.renew(previous)
    if (renewed.taskId !== managed.identity.taskId) {
      await queueScopeRevocation(managed, renewed)
      throw new Error('SCOPE_LEASE_IDENTITY_MISMATCH: renewed lease belongs to another task')
    }
    if (!managed.active || leases.get(key) !== managed) {
      await queueScopeRevocation(managed, renewed)
      return
    }
    managed.lease = renewed
    managed.renewFailures = 0
    await queueScopeRevocation(managed, previous)
    scheduleRenewal(managed)
  }
  catch (reason) {
    managed.onError?.(reason)
    if (!managed.active || leases.get(key) !== managed)
      return
    const now = managed.now?.() ?? Date.now()
    if (managed.lease.expiresAt <= now) {
      await stopScopeLease(managed.identity).catch(revokeReason => managed.onError?.(revokeReason))
      return
    }
    managed.renewFailures += 1
    const retryDelay = Math.min(60_000, MIN_RENEW_DELAY_MILLIS * 2 ** Math.min(managed.renewFailures - 1, 6))
    const delay = Math.min(retryDelay, Math.max(1, managed.lease.expiresAt - now))
    managed.timer = scheduleTimer(managed, () => void renewScopeLease(managed), delay)
  }
}

function queueScopeRevocation(managed: ScopeLeaseRegistration, lease: TaskScopeLease): Promise<void> {
  const key = revocationKey(managed.identity, lease.token)
  const existing = pendingRevocations.get(key)
  if (existing)
    return Promise.resolve()
  const pending: PendingRevocation = {
    identity: managed.identity,
    lease,
    revoke: managed.revoke,
    now: managed.now,
    schedule: managed.schedule,
    cancelSchedule: managed.cancelSchedule,
    onError: managed.onError,
    timer: null,
    failures: 0,
  }
  pendingRevocations.set(key, pending)
  return attemptScopeRevocation(key, pending)
}

async function attemptScopeRevocation(key: string, pending: PendingRevocation): Promise<void> {
  if (pendingRevocations.get(key) !== pending)
    return
  const now = pending.now?.() ?? Date.now()
  if (pending.failures > 0 && pending.lease.expiresAt <= now) {
    clearPendingRevocation(key, pending)
    return
  }
  try {
    await pending.revoke(pending.lease)
    clearPendingRevocation(key, pending)
  }
  catch (reason) {
    pending.onError?.(reason)
    if (pendingRevocations.get(key) !== pending)
      return
    pending.failures += 1
    const retryDelay = Math.min(60_000, MIN_RENEW_DELAY_MILLIS * 2 ** Math.min(pending.failures - 1, 6))
    const delay = Math.min(retryDelay, Math.max(1, pending.lease.expiresAt - now))
    pending.timer = schedulePendingTimer(pending, () => {
      void attemptScopeRevocation(key, pending)
    }, delay)
  }
}

function clearPendingRevocation(key: string, pending: PendingRevocation): void {
  if (pending.timer)
    cancelPendingTimer(pending, pending.timer)
  pendingRevocations.delete(key)
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

function schedulePendingTimer(pending: PendingRevocation, callback: () => void, delay: number): ScopeLeaseTimer {
  if (pending.schedule)
    return pending.schedule(callback, delay)
  return window.setTimeout(callback, delay)
}

function cancelPendingTimer(pending: PendingRevocation, timer: ScopeLeaseTimer): void {
  if (pending.cancelSchedule) {
    pending.cancelSchedule(timer)
    return
  }
  window.clearTimeout(timer as number)
}

function leaseKey(identity: ScopeLeaseIdentity): string {
  return `${identity.projectId}\u241F${identity.taskId}\u241F${identity.sessionId}`
}

function revocationKey(identity: ScopeLeaseIdentity, token: string): string {
  return `${leaseKey(identity)}\u241F${token}`
}
