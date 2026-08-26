import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it } from 'vitest'

const clientSource = readFileSync(
  new URL('../src-tauri/resources/mir3-core-plugin/lib/client.js', import.meta.url),
  'utf8',
)

describe('mir3 Core Plugin public runtime contract', () => {
  it('runs ordinary, system, and global Session flows through the real adapter', async () => {
    const runtime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    const dispose = runtime.plugin.apply(context)

    expect(runtime.messages[0]).toMatchObject({
      protocolVersion: 2,
      source: 'mir3-core-plugin',
      type: 'mir3/plugin.ready',
    })

    await runtime.send(request('mir3/bridge.describe', 1))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.description',
      payload: {
        protocolVersion: 2,
        capabilities: {
          archive: true,
          globalSession: true,
          ordinarySessionCanary: true,
          pendingInteraction: true,
          sessions: true,
          snapshot: true,
          workspaces: true,
        },
      },
    })

    await runtime.send(request('mir3/bridge.ordinarySessionCanary', 2, {
      payload: { cwd: '/tmp/mir3-runtime', sessionId: 'harness-canary-runtime' },
      sessionId: '',
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.ordinarySessionCanary',
      payload: { archived: true, managed: false, sessionId: 'harness-canary-runtime' },
    })

    const systemId = 'mir3-system-runtime'
    await runtime.send(request('mir3/systemSession.create', 3, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'update the current map resource' },
      sessionId: systemId,
    }))
    expect(calls).toContainEqual(['archive', systemId])
    expect(calls).toContainEqual(['open', systemId])
    expect(calls).toContainEqual(['prompt', systemId, 'update the current map resource', 'queue'])
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/systemSession.created',
      payload: { archived: true, created: true },
    })

    sessions.get(systemId).snapshot.nodes = [{
      type: 'tool-result',
      result: {
        changedResources: ['map:main'],
        draftId: 'draft-map-runtime',
        revision: 2,
        systemId: 'map',
        validation: { diagnostics: [], valid: true },
      },
    }]
    await runtime.send(request('mir3/systemSession.snapshot', 4, { sessionId: systemId }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/systemSession.snapshot',
      payload: {
        domainResults: [{
          changedResources: ['map:main'],
          draftId: 'draft-map-runtime',
          revision: 2,
          systemId: 'map',
          validation: { diagnostics: [], valid: true },
        }],
      },
    })
    await runtime.send(request('mir3/systemSession.complete', 5, { sessionId: systemId }))
    expect(lastMessage(runtime.messages).type).toBe('mir3/systemSession.completed')

    const globalId = 'global-runtime'
    const returnTo = { projectId: 'project-runtime', systemId: 'map', view: 'devtools' }
    await runtime.send(request('mir3/globalSession.create', 6, {
      payload: {
        cwd: '/tmp/mir3-runtime',
        prompt: 'coordinate map and quest changes',
        structuredContext: { returnTo },
      },
      sessionId: globalId,
      systemId: '__global__',
    }))
    expect(calls).toContainEqual(['workspace-create', '/tmp/mir3-runtime'])
    expect(calls).toContainEqual(['prompt', globalId, 'coordinate map and quest changes', 'queue'])
    await runtime.send(request('mir3/globalSession.complete', 7, {
      sessionId: globalId,
      systemId: '__global__',
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/globalSession.completed',
      payload: { returnTo },
    })

    dispose()
    expect(runtime.listenerRemoved()).toBe(true)
  })

  it('rejects foreign origins, sources, replayed sequences, and task-owner changes', async () => {
    const runtime = loadAdapter()
    const sessions = new Map()
    runtime.plugin.apply(createHarnessContext({ calls: [], sessions }))
    const initialCount = runtime.messages.length
    const create = request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime' },
      sessionId: 'mir3-system-owner',
    })

    await runtime.send(create, { origin: 'https://foreign.example' })
    await runtime.send(create, { source: {} })
    expect(runtime.messages).toHaveLength(initialCount)

    await runtime.send(create)
    expect(lastMessage(runtime.messages).type).toBe('mir3/systemSession.created')
    const acceptedCount = runtime.messages.length
    await runtime.send(create)
    expect(runtime.messages).toHaveLength(acceptedCount)

    await runtime.send(request('mir3/systemSession.prompt', 2, {
      payload: { content: 'foreign task write' },
      sessionId: 'mir3-system-owner',
      taskId: 'another-task',
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: {
        code: 'BRIDGE_REQUEST_FAILED',
        message: expect.stringContaining('SESSION_IDENTITY_MISMATCH'),
      },
    })
  })
})

function loadAdapter() {
  const messages = []
  let descriptor
  let listener
  let removed = false
  const parent = {
    postMessage(message, origin) {
      messages.push({ ...message, postedOrigin: origin })
    },
  }
  const window = {
    __ModuleLoader__: {
      load(value) {
        descriptor = value
      },
    },
    addEventListener(type, callback) {
      if (type === 'message')
        listener = callback
    },
    parent,
    removeEventListener(type, callback) {
      if (type === 'message' && callback === listener)
        removed = true
    },
  }
  vm.runInNewContext(clientSource, {
    Date,
    Map,
    Number,
    Object,
    Set,
    String,
    URL,
    document: { referrer: 'https://studio.mir3.test/workbench' },
    window,
  })
  const module = { exports: {} }
  const plugin = descriptor.factory(module)
  return {
    listenerRemoved: () => removed,
    messages,
    plugin,
    async send(data, overrides = {}) {
      listener({
        data,
        origin: overrides.origin ?? 'https://studio.mir3.test',
        source: overrides.source ?? parent,
      })
      await new Promise(resolve => setImmediate(resolve))
    },
  }
}

function createHarnessContext({ calls, sessions }) {
  let workspaceSequence = 0
  return {
    sessions: {
      binding(sessionId) {
        const session = sessions.get(sessionId)
        return session ? { session } : null
      },
      async create(options) {
        const sessionId = options.sessionId
        sessions.set(sessionId, createSession(sessionId, calls))
        calls.push(['session-create', sessionId, options.cwd ?? null, options.workspaceId ?? null])
        return { ok: true, value: { sessionId } }
      },
      open(sessionId) {
        calls.push(['sessions-open', sessionId])
      },
    },
    workspaces: {
      async archiveSession(sessionId) {
        calls.push(['archive', sessionId])
      },
      async create({ path }) {
        workspaceSequence += 1
        calls.push(['workspace-create', path])
        return { path, workspaceId: `workspace-${workspaceSequence}` }
      },
      startSession(workspaceId) {
        calls.push(['workspace-start', workspaceId])
      },
    },
  }
}

function createSession(sessionId, calls) {
  const snapshot = {
    blank: false,
    nodes: [],
    pending: [],
    queue: [],
    running: false,
    runningCalls: [],
    sessionId,
  }
  return {
    snapshot,
    async cancel() {
      calls.push(['cancel', sessionId])
      return { ok: true, value: {} }
    },
    getSnapshot() {
      return snapshot
    },
    async open() {
      calls.push(['open', sessionId])
    },
    async prompt(content, mode) {
      calls.push(['prompt', sessionId, content[0]?.text, mode])
      return { ok: true, value: {} }
    },
    subscribe() {
      return () => calls.push(['unsubscribe', sessionId])
    },
  }
}

function request(type, sequence, overrides = {}) {
  return {
    payload: {},
    projectId: 'project-runtime',
    protocolVersion: 2,
    requestId: `request-${sequence}-${type}`,
    sequence,
    sessionId: 'session-runtime',
    source: 'mir3-studio',
    systemId: 'map',
    taskId: 'task-runtime',
    type,
    ...overrides,
  }
}

function lastMessage(messages) {
  return messages.at(-1)
}
