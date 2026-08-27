// @vitest-environment happy-dom

import type { Mir3BridgeEnvelope } from '../src/features/projects/workspace-bridge'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { GLOBAL_WORKBENCH_EVENT, markGlobalTaskMcpDisabled, markGlobalTaskReviewPending, registeredGlobalTask, registeredGlobalTasks, registerGlobalTask, requestGlobalWorkbench, restoreGlobalTasks, unregisterGlobalTask } from '../src/features/system-ai/ai-handoff'
import { buildGlobalTaskHandoff } from '../src/features/system-ai/global-task-handoff'

const identity = {
  projectId: 'project-1',
  systemId: 'shop',
  taskId: 'global-task-1',
  sessionId: 'global-session-1',
  compositeId: 'composite-1',
  allowedSystems: ['shop', 'item'],
  allowedWriteSystems: ['shop'],
  draftIds: ['draft-1'],
  handoff: buildGlobalTaskHandoff({
    source: {
      projectId: 'project-1',
      systemId: 'shop',
      taskId: 'system-task-1',
      sessionId: 'system-session-1',
    },
    explicitSummary: { goal: 'Adjust shop prices', unfinishedSteps: ['Review composite Diff'] },
    references: { draftIds: ['draft-1'] },
    pluginVersions: { shop: '1.3.1', item: '1.3.1' },
    allowedReadSystems: ['shop', 'item'],
    allowedWriteSystems: ['shop'],
  }),
}

describe('global task recovery and workbench navigation', () => {
  beforeEach(() => {
    window.localStorage.clear()
    unregisterGlobalTask(identity)
  })

  afterEach(() => {
    unregisterGlobalTask(identity)
    window.localStorage.clear()
    vi.restoreAllMocks()
  })

  it('persists a non-sensitive task binding and restores it after the in-memory registry is cleared', () => {
    registerGlobalTask({ ...identity, updatedAt: 1_700_000_000_000 })
    const stored = window.localStorage.getItem('mir3-global-tasks:v1')
    expect(stored).toContain('composite-1')
    expect(stored).not.toContain('scopeToken')
    expect(stored).toContain('Adjust shop prices')

    unregisterGlobalTask(identity)
    window.localStorage.setItem('mir3-global-tasks:v1', stored!)
    expect(restoreGlobalTasks(1_700_000_001_000)).toHaveLength(1)
    expect(registeredGlobalTask(envelope())).toMatchObject({ compositeId: 'composite-1', draftIds: ['draft-1'], mcpStatus: 'disabled' })
    expect(registeredGlobalTask(envelope())?.handoff).toEqual(identity.handoff)
  })

  it('redacts a credential-bearing semantic field at the localStorage registration boundary', () => {
    registerGlobalTask({
      ...identity,
      scopeToken: 'unexpected-root-secret',
      handoff: {
        ...identity.handoff,
        goal: 'Continue with scopeToken=local-storage-secret',
      },
      updatedAt: 1_700_000_000_000,
    } as typeof identity & { scopeToken: string, updatedAt: number })
    const stored = window.localStorage.getItem('mir3-global-tasks:v1') ?? ''

    expect(stored).toContain('[REDACTED_CREDENTIAL]')
    expect(stored).not.toContain('local-storage-secret')
    expect(stored).not.toContain('unexpected-root-secret')
    expect(stored).not.toContain('scopeToken')
  })

  it('keeps a completed task registered until its deferred composite review is applied', () => {
    registerGlobalTask({ ...identity, updatedAt: 1_700_000_000_000 })
    expect(markGlobalTaskReviewPending(identity)?.reviewPending).toBe(true)
    const stored = window.localStorage.getItem('mir3-global-tasks:v1')
    unregisterGlobalTask(identity)
    window.localStorage.setItem('mir3-global-tasks:v1', stored!)

    expect(restoreGlobalTasks(1_700_000_001_000)).toMatchObject([{
      compositeId: 'composite-1',
      reviewPending: true,
    }])
    expect(registeredGlobalTasks()).toHaveLength(1)
  })

  it('discards expired and malformed stored task identities', () => {
    window.localStorage.setItem('mir3-global-tasks:v1', JSON.stringify({
      schemaVersion: 3,
      tasks: [
        { ...identity, mcpStatus: 'active', mcpError: null, updatedAt: 1 },
        { ...identity, taskId: '../../foreign', mcpStatus: 'active', mcpError: null, updatedAt: 1_700_000_000_000 },
      ],
    }))
    expect(restoreGlobalTasks(1_700_000_000_001)).toEqual([])
    expect(registeredGlobalTask(envelope())).toBeNull()
  })

  it('persists a disabled MCP state without discarding the structured recovery record', () => {
    registerGlobalTask({ ...identity, updatedAt: 1_700_000_000_000 })
    markGlobalTaskMcpDisabled(identity, 'GLOBAL_TASK_SCOPE_RECOVERY_FAILED: scopeToken=secret')
    const stored = window.localStorage.getItem('mir3-global-tasks:v1') ?? ''

    expect(registeredGlobalTask(envelope())).toMatchObject({
      compositeId: 'composite-1',
      mcpStatus: 'disabled',
      mcpError: 'GLOBAL_TASK_SCOPE_RECOVERY_FAILED: [REDACTED_CREDENTIAL]',
    })
    expect(stored).toContain('GLOBAL_TASK_SCOPE_RECOVERY_FAILED')
    expect(stored).not.toContain('secret')
    expect(stored).not.toContain('scopeToken')
  })

  it('emits an app-level request that makes the visible Harness workbench selectable', () => {
    const listener = vi.fn()
    window.addEventListener(GLOBAL_WORKBENCH_EVENT, listener)
    requestGlobalWorkbench(identity)
    expect(listener).toHaveBeenCalledTimes(1)
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({
      projectId: 'project-1',
      taskId: 'global-task-1',
      sessionId: 'global-session-1',
    })
    window.removeEventListener(GLOBAL_WORKBENCH_EVENT, listener)
  })
})

function envelope(): Mir3BridgeEnvelope {
  return {
    source: 'mir3-core-plugin',
    protocolVersion: 2,
    type: 'mir3/globalSession.snapshot',
    requestId: 'request-1',
    projectId: 'project-1',
    systemId: 'shop',
    taskId: 'global-task-1',
    sessionId: 'global-session-1',
    sequence: 2,
    payload: {},
  }
}
