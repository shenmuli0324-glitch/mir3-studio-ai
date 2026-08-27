import type { RegisteredGlobalTask } from './ai-handoff'
import type { TaskScopeLease } from '@/features/devtools/domain/types'
import { manageScopeLease, stopScopeLease } from './scope-lease-manager'

export interface GlobalTaskRecoveryDependencies {
  recover: (task: RegisteredGlobalTask, previous?: TaskScopeLease) => Promise<TaskScopeLease>
  revoke: (projectId: string, token: string) => Promise<void>
  postPrompt: (task: RegisteredGlobalTask, content: string) => boolean
  now?: () => number
  onActive?: () => void
  onError?: (reason: unknown) => void
}

export interface SourceScopeRetirementDependencies {
  revokeTask: (projectId: string, taskId: string) => Promise<void>
}

/**
 * 全局会话恢复前由 Tauri 重新核验并签发租约；续租沿用同一恢复入口，避免绕过组合任务身份。
 */
export async function recoverAndManageGlobalTaskScope(
  task: RegisteredGlobalTask,
  dependencies: GlobalTaskRecoveryDependencies,
): Promise<TaskScopeLease> {
  const lease = await dependencies.recover(task)
  try {
    assertRecoveredLease(task, lease, dependencies.now?.() ?? Date.now())
  }
  catch (reason) {
    await dependencies.revoke(task.projectId, lease.token).catch(revokeReason => dependencies.onError?.(revokeReason))
    throw reason
  }
  manageScopeLease({
    identity: task,
    lease,
    now: dependencies.now,
    renew: async (previous) => {
      const renewed = await dependencies.recover(task, previous)
      assertRecoveredLease(task, renewed, dependencies.now?.() ?? Date.now(), previous)
      if (!deliverGlobalTaskScope(task, renewed, dependencies.postPrompt)) {
        await dependencies.revoke(task.projectId, renewed.token)
        throw new Error('GLOBAL_TASK_SCOPE_DELIVERY_FAILED: renewed scope was not delivered')
      }
      dependencies.onActive?.()
      return renewed
    },
    revoke: value => dependencies.revoke(task.projectId, value.token),
    onError: dependencies.onError,
  })
  return lease
}

/** Session resume 成功后才投递新 token，避免把凭证发给尚未重新绑定身份的会话。 */
export function deliverGlobalTaskScope(
  task: RegisteredGlobalTask,
  lease: TaskScopeLease,
  postPrompt: GlobalTaskRecoveryDependencies['postPrompt'],
): boolean {
  return postPrompt(
    task,
    `[MIR3 Scope Renewal] scopeToken=${lease.token}; expiresAt=${lease.expiresAt}.`,
  )
}

/**
 * 全局任务接管 Draft 前先在后端确认撤销源系统租约；撤销失败时保留源租约并中止交接。
 */
export async function retireSourceTaskScope(
  identity: { projectId: string, taskId: string, sessionId: string },
  dependencies: SourceScopeRetirementDependencies,
): Promise<void> {
  await dependencies.revokeTask(identity.projectId, identity.taskId)
  await stopScopeLease(identity, false)
}

function assertRecoveredLease(
  task: RegisteredGlobalTask,
  lease: TaskScopeLease,
  now: number,
  previous?: TaskScopeLease,
): void {
  const expectedRead = previous?.readSystems ?? task.allowedSystems
  const expectedWrite = previous?.writeSystems ?? task.allowedWriteSystems ?? task.allowedSystems
  const expectedDrafts = previous?.draftIds ?? task.draftIds
  const expectedVersions = previous?.pluginVersions ?? task.handoff.pluginVersions
  if (lease.taskId !== task.taskId
    || lease.expiresAt <= now
    || !sameStringSet(lease.readSystems, expectedRead)
    || !sameStringSet(lease.writeSystems, expectedWrite)
    || !sameStringSet(lease.draftIds, expectedDrafts)
    || !sameVersionMap(lease.pluginVersions, expectedVersions)) {
    throw new Error('GLOBAL_TASK_SCOPE_RECOVERY_MISMATCH: Tauri returned a different task scope')
  }
}

function sameStringSet(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every(value => right.includes(value))
}

function sameVersionMap(left: Record<string, string>, right: Record<string, string>): boolean {
  const leftKeys = Object.keys(left)
  const rightKeys = Object.keys(right)
  return sameStringSet(leftKeys, rightKeys) && leftKeys.every(key => left[key] === right[key])
}
