import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it } from 'vitest'
import { isGlobalSession, isMir3ManagedSession, isProtectedTarget, isSystemSession } from '../src-tauri/resources/mir3-core-plugin/lib/policy.js'
import { BridgeSequenceRegistry } from '../src/features/projects/bridge-sequence'

describe('bridge protocol v2 sequence contract', () => {
  it('keeps independent request and response sequences strictly monotonic per session', () => {
    const requests = new BridgeSequenceRegistry()
    const responses = new BridgeSequenceRegistry()
    const identity = { projectId: 'project-1', taskId: 'task-1', sessionId: 'mir3-system-1' }

    expect(requests.next(identity)).toBe(1)
    expect(requests.next(identity)).toBe(2)
    expect(responses.accept(identity, 1)).toBe(true)
    expect(responses.accept(identity, 2)).toBe(true)
    expect(responses.accept(identity, 2)).toBe(false)
    expect(responses.accept(identity, 1)).toBe(false)
    expect(responses.accept(identity, 3)).toBe(true)
  })

  it('emits cancel and errors after snapshots with increasing Core response sequences', async () => {
    const harness = loadCoreClientHarness()
    harness.send(systemRequest('mir3/systemSession.create', 1, {}))
    await flushTasks()

    harness.emitSnapshot()
    harness.send(systemRequest('mir3/systemSession.cancel', 2, {}))
    await flushTasks()
    harness.send(systemRequest('mir3/unknown', 3, {}))
    await flushTasks()

    const sessionMessages = harness.posts.filter(message => message.sessionId === 'mir3-system-test')
    expect(sessionMessages.map(message => [message.type, message.sequence])).toEqual([
      ['mir3/systemSession.created', 1],
      ['mir3/systemSession.snapshot', 2],
      ['mir3/systemSession.cancelled', 3],
      ['mir3/bridge.error', 4],
    ])

    const count = harness.posts.length
    harness.send({ ...systemRequest('mir3/systemSession.prompt', 4, {}), sessionId: undefined })
    await flushTasks()
    expect(harness.posts).toHaveLength(count)
  })

  it('projects only structured tool results into snapshot and complete handoffs', async () => {
    const harness = loadCoreClientHarness()
    harness.send(systemRequest('mir3/systemSession.create', 1, {}))
    await flushTasks()
    const toolNode = {
      type: 'tool-result',
      toolName: 'mir3_domain_operate',
      input: { systemId: 'shop' },
      result: JSON.stringify({
        draftId: 'draft-1',
        revision: 7,
        validation: { valid: true, diagnostics: [] },
        changedResources: ['shop:item:1'],
      }),
    }
    harness.setSnapshot({
      nodes: [toolNode],
      runningCalls: [],
      running: true,
    })
    harness.emitSnapshot()
    harness.setSnapshot({
      nodes: [toolNode, { type: 'assistant', content: JSON.stringify({ draftId: 'forged', revision: 99, systemId: 'shop' }) }],
      runningCalls: [],
      running: false,
    })
    harness.emitSnapshot()

    const structured = harness.posts.find(message => message.type === 'mir3/systemSession.snapshot' && message.payload?.domainResults?.length > 0)
    expect(structured?.payload.domainResults).toEqual([{
      draftId: 'draft-1',
      revision: 7,
      systemId: 'shop',
      validation: { valid: true, diagnostics: [] },
      changedResources: ['shop:item:1'],
      resourceId: null,
    }])
    const completed = harness.posts.find(message => message.type === 'mir3/systemSession.completed')
    expect(completed?.payload.domainResults).toEqual(structured?.payload.domainResults)
  })

  it('streams global snapshots with a validated return target and supports renewal prompts', async () => {
    const harness = loadCoreClientHarness()
    harness.send(globalRequest('mir3/globalSession.create', 1, {
      cwd: '/project',
      prompt: 'structured summary only',
      structuredContext: { returnTo: { view: 'devtools', projectId: 'project-1', systemId: 'shop', draftId: 'draft-1' } },
    }))
    await flushTasks()
    harness.setSnapshot({
      nodes: [{ type: 'tool-result', toolName: 'mir3_validate', result: { systemId: 'shop', draftId: 'draft-1', revision: 2, validation: { valid: true, diagnostics: [] } } }],
      runningCalls: [],
      running: false,
    })
    harness.emitSnapshot()
    await flushTasks()

    const completed = harness.posts.find(message => message.type === 'mir3/globalSession.completed')
    expect(completed?.payload.returnTo).toEqual({ view: 'devtools', projectId: 'project-1', systemId: 'shop', draftId: 'draft-1' })
    expect(completed?.payload.domainResults[0]).toMatchObject({ systemId: 'shop', draftId: 'draft-1', revision: 2 })

    harness.send(globalRequest('mir3/globalSession.prompt', 2, { content: '[MIR3 Scope Renewal] token=new', mode: 'steer' }))
    await flushTasks()
    expect(harness.posts.some(message => message.type === 'mir3/globalSession.prompted')).toBe(true)
  })
})

describe('mir3 managed-session policy', () => {
  it('protects Studio system/global sessions and MIR3 map files without affecting ordinary Harness sessions', () => {
    expect(isSystemSession({ id: 'mir3-system-1' })).toBe(true)
    expect(isGlobalSession({ id: 'global-1' })).toBe(true)
    expect(isMir3ManagedSession({ id: 'global-1' })).toBe(true)
    expect(isMir3ManagedSession({ id: 'ordinary-harness-session' })).toBe(false)
    expect(isProtectedTarget('/project', { path: '/project/Data/maps/a.map' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/project/Data/config.json' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/project/Data/unknown.bin' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/outside/a.map' })).toBe(false)
  })
})

interface PostedMessage {
  type: string
  sessionId: string
  sequence: number
  payload: any
}

function systemRequest(type: string, sequence: number, payload: unknown) {
  return {
    source: 'mir3-studio',
    protocolVersion: 2,
    type,
    requestId: `request-${sequence}`,
    projectId: 'project-1',
    systemId: 'map',
    taskId: 'task-1',
    sessionId: 'mir3-system-test',
    sequence,
    payload: type === 'mir3/systemSession.create' ? { cwd: '/project', ...(payload as object) } : payload,
  }
}

function globalRequest(type: string, sequence: number, payload: unknown) {
  return {
    ...systemRequest(type, sequence, payload),
    systemId: 'shop',
    taskId: 'global-task-1',
    sessionId: 'global-session-1',
  }
}

function loadCoreClientHarness() {
  const source = readFileSync(new URL('../src-tauri/resources/mir3-core-plugin/lib/client.js', import.meta.url), 'utf8')
  const posts: PostedMessage[] = []
  let listener: ((event: { source: unknown, origin: string, data: unknown }) => void) | undefined
  let exported: { apply: (context: unknown) => () => void } | undefined
  let snapshotListener: (() => void) | undefined
  let snapshot: Record<string, unknown> = { nodes: [], runningCalls: [], running: true }
  const parent = {
    postMessage(message: PostedMessage) {
      posts.push(message)
    },
  }
  const session = {
    async open() {},
    async prompt() {
      return { ok: true, value: {} }
    },
    async cancel() {
      return { ok: true, value: {} }
    },
    subscribe(callback: () => void) {
      snapshotListener = callback
      return function dispose() {}
    },
    getSnapshot() {
      return snapshot
    },
  }
  const windowObject = {
    parent,
    __ModuleLoader__: {
      load(definition: { factory: (module: { exports: unknown }) => void }) {
        const module = { exports: {} }
        definition.factory(module)
        exported = module.exports as typeof exported
      },
    },
    addEventListener(type: string, callback: typeof listener) {
      if (type === 'message')
        listener = callback
    },
    removeEventListener() {},
  }
  vm.runInNewContext(source, {
    URL,
    document: { referrer: 'http://studio.local/app' },
    window: windowObject,
  })
  if (!exported)
    throw new Error('CORE_CLIENT_TEST_LOAD_FAILED')
  exported.apply({
    sessions: {
      async create() {
        return { ok: true, value: {} }
      },
      binding() {
        return { session }
      },
      open() {},
    },
    workspaces: {
      async archiveSession() {},
      async create() {
        return { workspaceId: 'workspace-1', path: '/project' }
      },
      startSession() {},
    },
  })
  if (!listener)
    throw new Error('CORE_CLIENT_TEST_LISTENER_FAILED')
  return {
    posts,
    emitSnapshot() {
      snapshotListener?.()
    },
    setSnapshot(value: Record<string, unknown>) {
      snapshot = value
    },
    send(data: unknown) {
      listener?.({ source: parent, origin: 'http://studio.local', data })
    },
  }
}

async function flushTasks() {
  await new Promise(resolve => setTimeout(resolve, 0))
}
