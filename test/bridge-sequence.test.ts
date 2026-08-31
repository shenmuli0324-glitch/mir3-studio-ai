import type { RefObject } from 'react'
import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it, vi } from 'vitest'
import { developmentWriteViolation, isGlobalSession, isGuiSession, isMir3ManagedSession, isProtectedTarget, isSystemSession, managedWriteViolation, sessionScopeViolation } from '../src-tauri/resources/mir3-core-plugin/lib/policy.js'
import { BridgeSequenceRegistry } from '../src/features/projects/bridge-sequence'
import { bootstrapHarnessBridge, connectHarnessBridge, ensureHarnessProjectActive, subscribeHarnessBridge } from '../src/features/projects/workspace-bridge'

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

  it('accepts Core responses only from the exact iframe origin/source with a complete v2 DTO', () => {
    let messageListener: ((event: MessageEvent) => void) | null = null
    const contentWindow = {}
    vi.stubGlobal('window', {
      addEventListener(type: string, listener: (event: MessageEvent) => void) {
        if (type === 'message')
          messageListener = listener
      },
      removeEventListener() {},
    })
    const received: string[] = []
    const iframeRef = {
      current: { src: 'http://127.0.0.1:3081/workbench', contentWindow },
    } as unknown as RefObject<HTMLIFrameElement | null>
    const disconnect = connectHarnessBridge(iframeRef)
    const unsubscribe = subscribeHarnessBridge(message => received.push(message.type))
    const valid = {
      source: 'mir3-core-plugin',
      protocolVersion: 2,
      type: 'mir3/systemSession.snapshot',
      requestId: 'response-1',
      projectId: 'project-studio-side',
      systemId: 'shop',
      taskId: 'task-studio-side',
      sessionId: 'mir3-system-studio-side',
      sequence: 2,
      payload: {},
    }
    messageListener?.({ origin: 'http://evil.local', source: contentWindow, data: valid } as MessageEvent)
    messageListener?.({ origin: 'http://127.0.0.1:3081', source: {}, data: valid } as MessageEvent)
    const { payload: _payload, ...missingPayload } = valid
    messageListener?.({ origin: 'http://127.0.0.1:3081', source: contentWindow, data: missingPayload } as MessageEvent)
    expect(received).toEqual([])

    messageListener?.({ origin: 'http://127.0.0.1:3081', source: contentWindow, data: valid } as MessageEvent)
    messageListener?.({ origin: 'http://127.0.0.1:3081', source: contentWindow, data: { ...valid, sequence: 1 } } as MessageEvent)
    messageListener?.({ origin: 'http://127.0.0.1:3081', source: contentWindow, data: { ...valid, sequence: 3 } } as MessageEvent)
    expect(received).toEqual(['mir3/systemSession.snapshot', 'mir3/systemSession.snapshot'])
    unsubscribe()
    disconnect()
    vi.unstubAllGlobals()
  })

  it('acknowledges the active project over the MessagePort before system AI starts', async () => {
    const requests: any[] = []
    let pluginPort: MessagePort | null = null
    vi.stubGlobal('window', {
      addEventListener() {},
      removeEventListener() {},
      setTimeout,
      clearTimeout,
    })
    const contentWindow = {
      postMessage(_message: unknown, _origin: string, ports: MessagePort[]) {
        pluginPort = ports[0]
        pluginPort.addEventListener('message', (event) => {
          const request = event.data
          requests.push(request)
          if (request.type !== 'mir3/project.activate')
            return
          pluginPort?.postMessage({
            source: 'mir3-core-plugin',
            protocolVersion: 2,
            type: 'mir3/project.activated',
            requestId: request.requestId,
            projectId: request.projectId,
            systemId: request.systemId,
            taskId: request.taskId,
            sessionId: request.sessionId,
            sequence: 1,
            payload: { workspaceId: 'workspace-project-1', canonicalPath: '/project' },
          })
        })
        pluginPort.start()
        pluginPort.postMessage({
          source: 'mir3-core-plugin',
          protocolVersion: 2,
          type: 'mir3/plugin.ready',
          requestId: 'ready-project-test',
          projectId: '',
          systemId: '',
          taskId: '',
          sessionId: '',
          sequence: 1,
          payload: { protocolVersion: 2 },
        })
      },
    }
    const iframeRef = {
      current: { src: 'http://127.0.0.1:3081/workbench', contentWindow },
    } as unknown as RefObject<HTMLIFrameElement | null>
    const project = {
      id: 'project-1',
      name: 'Project 1',
      root: '/project',
      clientRoot: '/project/客户端',
      engineRoot: '/project/引擎',
      activeWorkspaceRoot: '/project',
      status: 'valid' as const,
      warnings: [],
      createdAt: 1,
      updatedAt: 1,
    }
    const disconnect = connectHarnessBridge(iframeRef)
    expect(bootstrapHarnessBridge(iframeRef)).toBe(true)

    await ensureHarnessProjectActive(project)
    await ensureHarnessProjectActive(project)

    expect(requests.filter(request => request.type === 'mir3/project.activate')).toHaveLength(1)
    expect(requests.find(request => request.type === 'mir3/project.activate')).toMatchObject({
      projectId: 'project-1',
      payload: { projectRoot: '/project', workspaceRoot: '/project', startSession: false },
    })
    pluginPort?.close()
    disconnect()
    vi.unstubAllGlobals()
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

  it('creates an archived system session before opening and prompting without a Workspace', async () => {
    const harness = loadCoreClientHarness()
    harness.send(systemRequest('mir3/systemSession.create', 1, { prompt: 'inspect the current system' }))
    await flushTasks()

    expect(harness.calls).toEqual([
      'session.create:mir3-system-test:cwd',
      'workspace.archive:mir3-system-test',
      'session.open',
      'session.subscribe',
      'post:mir3/systemSession.created',
      'session.prompt:queue',
    ])
    expect(harness.calls.some(call => call.startsWith('workspace.create:'))).toBe(false)
  })

  it('creates and opens an ordinary Session without a managed marker before archiving cleanup', async () => {
    const harness = loadCoreClientHarness()
    harness.send(systemRequest('mir3/bridge.ordinarySessionCanary', 1, {
      cwd: '/project',
      sessionId: 'harness-canary-test',
    }))
    await flushTasks()

    expect(harness.calls).toEqual([
      'session.create:harness-canary-test:cwd',
      'session.open',
      'workspace.archive:harness-canary-test',
      'post:mir3/bridge.ordinarySessionCanary',
    ])
    expect(harness.posts.at(-1)?.payload).toMatchObject({
      sessionId: 'harness-canary-test',
      managed: false,
      archived: true,
    })
    expect(isMir3ManagedSession({ id: 'harness-canary-test' })).toBe(false)
  })

  it('rejects wrong origin, wrong source, missing DTO payload, old sequence, and cross-task session control', async () => {
    const harness = loadCoreClientHarness()
    const create = systemRequest('mir3/systemSession.create', 1, {})
    harness.send(create, { origin: 'http://evil.local' })
    harness.send(create, { source: {} })
    harness.send({ ...create, payload: undefined, sequence: 2, __omitPayload: true })
    await flushTasks()
    expect(harness.posts.filter(message => message.type !== 'mir3/plugin.ready' && message.type !== 'mir3/project.activated')).toEqual([])

    harness.send(create)
    await flushTasks()
    harness.send(systemRequest('mir3/systemSession.prompt', 2, { content: 'first' }))
    await flushTasks()
    const afterFirst = harness.posts.length
    harness.send(systemRequest('mir3/systemSession.prompt', 2, { content: 'replayed' }))
    await flushTasks()
    expect(harness.posts).toHaveLength(afterFirst)

    harness.send({ ...systemRequest('mir3/systemSession.prompt', 1, { content: 'cross task' }), taskId: 'task-other' })
    await flushTasks()
    expect(harness.posts.at(-1)?.type).toBe('mir3/bridge.error')
    expect(String(harness.posts.at(-1)?.payload.message)).toContain('SESSION_IDENTITY_MISMATCH')
  })

  it('resumes, cancels, and answers question/approval interactions through the bound session', async () => {
    const harness = loadCoreClientHarness()
    harness.send(systemRequest('mir3/systemSession.create', 1, {}))
    await flushTasks()

    harness.setPending('question', { answer: 'yes' })
    harness.send(systemRequest('mir3/systemSession.respond', 2, { pendingKey: 'pending-1', response: { answer: 'yes' } }))
    await flushTasks()
    expect(harness.pendingResponses.at(-1)).toMatchObject({ ok: true, value: { answer: 'yes' } })

    harness.setPending('approval', { outcome: 'allowed-once' })
    harness.send(systemRequest('mir3/systemSession.respond', 3, { pendingKey: 'pending-1', response: { outcome: 'allowed-once' } }))
    await flushTasks()
    expect(harness.pendingResponses.at(-1)).toMatchObject({ ok: true, value: { outcome: 'allowed-once' } })

    harness.send(systemRequest('mir3/systemSession.cancel', 4, {}))
    await flushTasks()
    expect(harness.posts.some(message => message.type === 'mir3/systemSession.cancelled')).toBe(true)
    harness.send(systemRequest('mir3/systemSession.resume', 5, {}))
    await flushTasks()
    expect(harness.posts.some(message => message.type === 'mir3/systemSession.resumed')).toBe(true)
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
  it('protects Studio system/GUI/global sessions and MIR3 project files without affecting ordinary Harness sessions', () => {
    expect(isSystemSession({ id: 'mir3-system-1' })).toBe(true)
    expect(isGuiSession({ id: 'mir3-gui-1' })).toBe(true)
    expect(isGlobalSession({ id: 'global-1' })).toBe(true)
    expect(isMir3ManagedSession({ id: 'global-1' })).toBe(true)
    expect(isMir3ManagedSession({ id: 'ordinary-harness-session' })).toBe(false)
    expect(isProtectedTarget('/project', { path: '/project/Data/maps/a.map' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/project/Data/config.json' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/project/Data/unknown.bin' })).toBe(true)
    expect(isProtectedTarget('/project', { path: '/outside/a.map' })).toBe(false)
    expect(managedWriteViolation('/project', { id: 'mir3-system-1', header: { cwd: '/project' } }, { path: '/project/Data/config.json' })).toBe('MIR3_SYSTEM_SESSION_DRAFT_REQUIRED')
    expect(managedWriteViolation('/project', { id: 'mir3-gui-1', header: { cwd: '/project' } }, { path: '/project/客户端/dev/GUIExport/Test.lua' })).toBe('MIR3_SYSTEM_SESSION_DRAFT_REQUIRED')
    expect(managedWriteViolation('/project', { id: 'global-1', header: { cwd: '/outside' } }, { path: '/tmp/scratch.txt' })).toBe('MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE')
    expect(managedWriteViolation('/project', { id: 'mir3-system-1', header: { cwd: '/project' } }, { path: '/tmp/scratch.txt' })).toBeNull()
    expect(managedWriteViolation('/project', { id: 'ordinary-harness-session', header: { cwd: '/project' } }, { path: '/project/Data/config.json' })).toBeNull()
    expect(sessionScopeViolation('/project', { id: 'ordinary-harness-session', header: { cwd: '/outside' } })).toBe('MIR3_PROJECT_SESSION_OUTSIDE_SCOPE')
    expect(developmentWriteViolation('/project', { id: 'ordinary-harness-session', header: { cwd: '/project' } }, { path: '/outside/file.txt' })).toBe('MIR3_PROJECT_WRITE_OUTSIDE_SCOPE')
    expect(developmentWriteViolation('/project', { id: 'ordinary-harness-session', header: { cwd: '/project' } }, { path: '/project/file.txt' })).toBeNull()
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
  const calls: string[] = []
  const pendingResponses: unknown[] = []
  let listener: ((event: { source: unknown, origin: string, data: unknown }) => void) | undefined
  let exported: { apply: (context: unknown) => () => void } | undefined
  let snapshotListener: (() => void) | undefined
  let snapshot: Record<string, unknown> = { nodes: [], runningCalls: [], running: true }
  const parent = {
    postMessage(message: PostedMessage) {
      posts.push(message)
      if (message.type !== 'mir3/project.activated')
        calls.push(`post:${message.type}`)
    },
  }
  const session = {
    async open() {
      calls.push('session.open')
    },
    async prompt(_content: unknown, mode: string) {
      calls.push(`session.prompt:${mode}`)
      return { ok: true, value: {} }
    },
    async cancel() {
      calls.push('session.cancel')
      return { ok: true, value: {} }
    },
    subscribe(callback: () => void) {
      calls.push('session.subscribe')
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
      async create(options: { sessionId: string, cwd?: string }) {
        calls.push(`session.create:${options.sessionId}:${options.cwd ? 'cwd' : 'workspace'}`)
        return options.sessionId
      },
      binding() {
        return { session }
      },
      open() {},
    },
    workspaces: {
      list: {
        getSnapshot() {
          return { items: [{ workspaceId: 'workspace-1', path: '/project' }] }
        },
      },
      async archiveSession(sessionId: string) {
        calls.push(`workspace.archive:${sessionId}`)
      },
      async create(options: { path: string }) {
        calls.push(`workspace.create:${options.path}`)
        return { workspaceId: 'workspace-1', path: '/project' }
      },
      async delete() {},
      startSession() {},
    },
  })
  if (!listener)
    throw new Error('CORE_CLIENT_TEST_LISTENER_FAILED')
  listener({
    source: parent,
    origin: 'http://studio.local',
    data: {
      source: 'mir3-studio',
      protocolVersion: 2,
      type: 'mir3/project.activate',
      requestId: 'activate-project-1',
      projectId: 'project-1',
      systemId: '__project__',
      taskId: 'project-activation',
      sessionId: '',
      sequence: 1,
      payload: { projectRoot: '/project', workspaceRoot: '/project', startSession: false },
    },
  })
  calls.length = 0
  return {
    calls,
    pendingResponses,
    posts,
    emitSnapshot() {
      snapshotListener?.()
    },
    setSnapshot(value: Record<string, unknown>) {
      snapshot = value
    },
    setPending(kind: 'approval' | 'question', expectedResponse: unknown) {
      snapshot = {
        ...snapshot,
        pending: [{
          key: 'pending-1',
          kind,
          sessionId: 'mir3-system-test',
          payload: kind === 'approval' ? { approvalId: 'approval-1' } : { question: 'Continue?' },
          async respond(response: unknown) {
            pendingResponses.push(response)
            expect(response).toMatchObject({ ok: true, value: expectedResponse } as object)
            return { accepted: true }
          },
        }],
      }
    },
    send(data: any, overrides: { origin?: string, source?: unknown } = {}) {
      if (data?.__omitPayload) {
        const { __omitPayload: _ignored, payload: _payload, ...withoutPayload } = data
        data = withoutPayload
      }
      listener?.({ source: overrides.source ?? parent, origin: overrides.origin ?? 'http://studio.local', data })
    },
  }
}

async function flushTasks() {
  await new Promise(resolve => setTimeout(resolve, 0))
}
