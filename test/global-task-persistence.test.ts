// @vitest-environment happy-dom

import type { Mir3BridgeEnvelope } from '../src/features/projects/workspace-bridge'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { GLOBAL_WORKBENCH_EVENT, markGlobalTaskReviewPending, registeredGlobalTask, registeredGlobalTasks, registerGlobalTask, requestGlobalWorkbench, restoreGlobalTasks, unregisterGlobalTask } from '../src/features/system-ai/ai-handoff'

const identity = {
  projectId: 'project-1',
  systemId: 'shop',
  taskId: 'global-task-1',
  sessionId: 'global-session-1',
  compositeId: 'composite-1',
  allowedSystems: ['shop', 'item'],
  allowedWriteSystems: ['shop'],
  draftIds: ['draft-1'],
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

    unregisterGlobalTask(identity)
    window.localStorage.setItem('mir3-global-tasks:v1', stored!)
    expect(restoreGlobalTasks(1_700_000_001_000)).toHaveLength(1)
    expect(registeredGlobalTask(envelope())).toMatchObject({ compositeId: 'composite-1', draftIds: ['draft-1'] })
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
      schemaVersion: 1,
      tasks: [
        { ...identity, updatedAt: 1 },
        { ...identity, taskId: '../../foreign', updatedAt: 1_700_000_000_000 },
      ],
    }))
    expect(restoreGlobalTasks(1_700_000_000_001)).toEqual([])
    expect(registeredGlobalTask(envelope())).toBeNull()
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
