// @vitest-environment happy-dom

import type { TaskScopeLease } from '../src/features/devtools/domain/types'
import type { RegisteredGlobalTask } from '../src/features/system-ai/ai-handoff'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { buildGlobalTaskHandoff } from '../src/features/system-ai/global-task-handoff'
import { recoverAndManageGlobalTaskScope, retireSourceTaskScope } from '../src/features/system-ai/global-task-recovery'
import { currentScopeLease, manageScopeLease, stopScopeLease } from '../src/features/system-ai/scope-lease-manager'

describe('global task scope recovery', () => {
  afterEach(async () => {
    vi.useRealTimers()
    await stopScopeLease(globalTask(), false)
    await stopScopeLease(sourceIdentity(), false)
    vi.restoreAllMocks()
  })

  it('reissues an expired restored task scope and restores its renewal timer', async () => {
    vi.useFakeTimers()
    const task = globalTask()
    const recover = vi.fn()
      .mockResolvedValueOnce(lease('fresh-token-1', 301_000))
      .mockResolvedValueOnce(lease('fresh-token-2', 900_000))
    const postPrompt = vi.fn(() => true)
    const active = vi.fn()

    await recoverAndManageGlobalTaskScope(task, {
      recover,
      revoke: vi.fn(async () => {}),
      postPrompt,
      now: () => 0,
      onActive: active,
    })

    expect(currentScopeLease(task)?.token).toBe('fresh-token-1')
    await vi.advanceTimersByTimeAsync(1_000)
    await vi.waitFor(() => expect(recover).toHaveBeenCalledTimes(2))
    expect(recover.mock.calls[1][1]?.token).toBe('fresh-token-1')
    expect(postPrompt).toHaveBeenCalledWith(task, expect.stringContaining('fresh-token-2'))
    expect(currentScopeLease(task)?.token).toBe('fresh-token-2')
    expect(active).toHaveBeenCalledTimes(1)
  })

  it('revokes and rejects a recovered lease whose pinned plugin versions differ', async () => {
    const revoke = vi.fn(async () => {})
    await expect(recoverAndManageGlobalTaskScope(globalTask(), {
      recover: async () => ({ ...lease('mismatched-token', Date.now() + 60_000), pluginVersions: { shop: '9.9.9' } }),
      revoke,
      postPrompt: () => true,
    })).rejects.toThrow('GLOBAL_TASK_SCOPE_RECOVERY_MISMATCH')

    expect(revoke).toHaveBeenCalledWith('project-1', 'mismatched-token')
    expect(currentScopeLease(globalTask())).toBeNull()
  })

  it('revokes every source-task lease before removing the local source lease', async () => {
    const source = sourceIdentity()
    const localRevoke = vi.fn(async () => {})
    manageScopeLease({
      identity: source,
      lease: { ...lease('source-token', Date.now() + 60_000), taskId: source.taskId },
      renew: async previous => previous,
      revoke: localRevoke,
    })
    const revokeTask = vi.fn(async () => {})

    await retireSourceTaskScope(source, { revokeTask })

    expect(revokeTask).toHaveBeenCalledWith(source.projectId, source.taskId)
    expect(localRevoke).not.toHaveBeenCalled()
    expect(currentScopeLease(source)).toBeNull()
  })

  it('keeps the source lease managed when task-wide revocation fails', async () => {
    const source = sourceIdentity()
    manageScopeLease({
      identity: source,
      lease: { ...lease('source-token', Date.now() + 60_000), taskId: source.taskId },
      renew: async previous => previous,
      revoke: async () => {},
    })

    await expect(retireSourceTaskScope(source, {
      revokeTask: async () => {
        throw new Error('TASK_SCOPES_REVOKE_FAILED')
      },
    })).rejects.toThrow('TASK_SCOPES_REVOKE_FAILED')
    expect(currentScopeLease(source)?.token).toBe('source-token')
  })
})

function globalTask(): RegisteredGlobalTask {
  return {
    projectId: 'project-1',
    systemId: 'shop',
    taskId: 'global-task-1',
    sessionId: 'global-session-1',
    compositeId: 'composite-1',
    allowedSystems: ['shop'],
    allowedWriteSystems: ['shop'],
    draftIds: ['draft-1'],
    handoff: buildGlobalTaskHandoff({
      source: { projectId: 'project-1', systemId: 'shop', taskId: 'system-task-1', sessionId: 'system-session-1' },
      explicitSummary: { goal: 'Update shop prices' },
      references: { draftIds: ['draft-1'] },
      pluginVersions: { shop: '1.3.1' },
      allowedReadSystems: ['shop'],
      allowedWriteSystems: ['shop'],
    }),
    mcpStatus: 'disabled',
    mcpError: null,
    reviewPending: false,
    updatedAt: 1,
  }
}

function sourceIdentity() {
  return { projectId: 'project-1', taskId: 'system-task-1', sessionId: 'system-session-1' }
}

function lease(token: string, expiresAt: number): TaskScopeLease {
  return {
    token,
    taskId: 'global-task-1',
    readSystems: ['shop'],
    writeSystems: ['shop'],
    draftIds: ['draft-1'],
    pluginVersions: { shop: '1.3.1' },
    expiresAt,
  }
}
