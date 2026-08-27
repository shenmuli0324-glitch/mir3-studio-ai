import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it } from 'vitest'

const clientSource = readFileSync(
  new URL('../src-tauri/resources/mir3-core-plugin/lib/client.js', import.meta.url),
  'utf8',
)

describe('mir3 Core Plugin public runtime contract', () => {
  it('normalizes the packaged Tauri referrer without allowing opaque origins', () => {
    const runtime = loadAdapter('tauri://localhost/workbench')
    const opaque = loadAdapter('file:///tmp/mir3-studio.html')
    runtime.plugin.apply(createHarnessContext({ calls: [], sessions: new Map() }))
    opaque.plugin.apply(createHarnessContext({ calls: [], sessions: new Map() }))

    expect(runtime.messages[0]).toMatchObject({
      postedOrigin: 'tauri://localhost',
      type: 'mir3/plugin.ready',
    })
    expect(opaque.messages).toHaveLength(0)
  })

  it('rejects Workspace and Session targets outside the activated project', async () => {
    const runtime = loadAdapter()
    const context = createHarnessContext({ calls: [], sessions: new Map() })
    runtime.plugin.apply(context)
    await runtime.send(request('mir3/bridge.describe', 1))

    await expect(
      context.workspaces.create({ path: '/tmp/foreign-project' }),
    ).rejects.toThrow('PROJECT_PATH_OUTSIDE_SCOPE')
    await expect(
      context.workspaces.create({ path: '/tmp/mir3-runtime/../foreign-project' }),
    ).rejects.toThrow('PROJECT_PATH_OUTSIDE_SCOPE')
    await expect(
      context.sessions.create({ cwd: '/tmp/foreign-project', sessionId: 'session-foreign' }),
    ).rejects.toThrow('PROJECT_PATH_OUTSIDE_SCOPE')
    await expect(
      context.workspaces.listDirectory('/tmp/foreign-project'),
    ).rejects.toThrow('PROJECT_PATH_OUTSIDE_SCOPE')
  })

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
          guiSession: true,
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

  it('runs an independent token-scoped GUI Session without a domain identity', async () => {
    const runtime = loadAdapter()
    const calls = []
    const sessions = new Map()
    runtime.plugin.apply(createHarnessContext({ calls, sessions }))
    const sessionId = 'mir3-gui-runtime'
    const token = 'a'.repeat(64)
    await runtime.send(request('mir3/guiSession.create', 1, {
      payload: {
        cwd: '/tmp/mir3-runtime',
        prompt: 'move the selected panel',
        workspaceId: 'gui-workspace-runtime',
        workspaceToken: token,
      },
      sessionId,
      systemId: '__studio_gui__',
      taskId: 'gui-task-runtime',
    }))
    expect(calls).toContainEqual(['archive', sessionId])
    const initialPrompt = calls.find(call => call[0] === 'prompt' && call[1] === sessionId)?.[2]
    expect(initialPrompt).toContain('mir3_gui_asset_query')
    expect(initialPrompt).toContain(`workspaceToken=${token}`)
    expect(initialPrompt).toContain('move the selected panel')

    const nextToken = 'b'.repeat(64)
    await runtime.send(request('mir3/guiSession.prompt', 2, {
      payload: {
        content: 'set x to 12',
        mode: 'queue',
        workspaceId: 'gui-workspace-runtime',
        workspaceToken: nextToken,
      },
      sessionId,
      systemId: '__studio_gui__',
      taskId: 'gui-task-runtime',
    }))
    const nextPrompt = calls.findLast(call => call[0] === 'prompt' && call[1] === sessionId)?.[2]
    expect(nextPrompt).toContain(`workspaceToken=${nextToken}`)
    expect(nextPrompt).toContain('set x to 12')

    sessions.get(sessionId).snapshot.nodes = [{
      type: 'tool-result',
      input: { workspaceToken: nextToken },
      result: {
        guiResult: {
          kind: 'operation',
          path: 'GUIExport/Test.lua',
          workspaceId: 'gui-workspace-runtime',
          workingRevision: 3,
          valid: true,
          diagnostics: [],
          source: 'must-not-be-projected',
        },
      },
    }]
    await runtime.send(request('mir3/guiSession.snapshot', 3, {
      sessionId,
      systemId: '__studio_gui__',
      taskId: 'gui-task-runtime',
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/guiSession.snapshot',
      payload: {
        guiResults: [{
          kind: 'operation',
          path: 'GUIExport/Test.lua',
          workspaceId: 'gui-workspace-runtime',
          workingRevision: 3,
          valid: true,
        }],
      },
    })
    expect(lastMessage(runtime.messages).payload.guiResults[0]).not.toHaveProperty('source')
    expect(lastMessage(runtime.messages).payload.nodes[0].input.workspaceToken).toBe('[redacted]')

    await runtime.send(request('mir3/guiSession.complete', 4, {
      sessionId,
      systemId: '__studio_gui__',
      taskId: 'gui-task-runtime',
    }))
    expect(lastMessage(runtime.messages).type).toBe('mir3/guiSession.completed')
  })

  it('releases the owner after Session creation fails so another task can retry safely', async () => {
    const runtime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    const create = context.sessions.create
    let createAttempts = 0
    context.sessions.create = async (options) => {
      createAttempts += 1
      if (createAttempts === 1)
        throw new Error('session create failed: fixture-create-failed: fixture create failed')
      return create(options)
    }
    runtime.plugin.apply(context)
    const sessionId = 'mir3-system-create-retry'

    await runtime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'must not run before archive' },
      sessionId,
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('SYSTEM_SESSION_CREATE_FAILED') },
    })
    expect(calls.some(call => call[0] === 'archive' || call[0] === 'open' || call[0] === 'prompt')).toBe(false)

    await runtime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'retry after create failure' },
      sessionId,
      taskId: 'task-create-retry',
    }))
    expect(createAttempts).toBe(2)
    expect(calls).toContainEqual(['archive', sessionId])
    expect(calls).toContainEqual(['open', sessionId])
    expect(calls).toContainEqual(['prompt', sessionId, 'retry after create failure', 'queue'])
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/systemSession.created',
      taskId: 'task-create-retry',
    })
  })

  it('reuses a partially created Session after archive fails and never opens it before archive succeeds', async () => {
    const runtime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    let archiveAttempts = 0
    context.workspaces.archiveSession = async (sessionId) => {
      archiveAttempts += 1
      calls.push(['archive', sessionId])
      if (archiveAttempts === 1)
        throw new Error('fixture archive failed')
    }
    runtime.plugin.apply(context)
    const sessionId = 'mir3-system-archive-retry'

    await runtime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'must stay blocked' },
      sessionId,
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('fixture archive failed') },
    })
    expect(calls.filter(call => call[0] === 'session-create')).toHaveLength(1)
    expect(calls.some(call => call[0] === 'open' || call[0] === 'prompt')).toBe(false)

    await runtime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'retry after archive failure' },
      sessionId,
      taskId: 'task-archive-retry',
    }))
    expect(calls.filter(call => call[0] === 'session-create')).toHaveLength(1)
    expect(calls.filter(call => call[0] === 'archive')).toHaveLength(2)
    const successfulArchive = calls.findLastIndex(call => call[0] === 'archive')
    const open = calls.findIndex(call => call[0] === 'open')
    const prompt = calls.findIndex(call => call[0] === 'prompt')
    expect(successfulArchive).toBeLessThan(open)
    expect(open).toBeLessThan(prompt)
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/systemSession.created',
      taskId: 'task-archive-retry',
    })
  })

  it('blocks concurrent Session commands until the archive operation has completed', async () => {
    const runtime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    let finishArchive
    context.workspaces.archiveSession = async (sessionId) => {
      calls.push(['archive', sessionId])
      await new Promise((resolve) => {
        finishArchive = resolve
      })
    }
    runtime.plugin.apply(context)
    const sessionId = 'mir3-system-archive-pending'

    await runtime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'run only after archive' },
      sessionId,
    }))
    await runtime.send(request('mir3/systemSession.prompt', 2, {
      payload: { content: 'concurrent prompt' },
      sessionId,
    }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('SYSTEM_SESSION_PREPARATION_IN_PROGRESS') },
    })
    await runtime.send(request('mir3/systemSession.resume', 3, { sessionId }))
    expect(lastMessage(runtime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('SYSTEM_SESSION_PREPARATION_IN_PROGRESS') },
    })
    expect(calls.some(call => call[0] === 'open' || call[0] === 'prompt')).toBe(false)

    finishArchive()
    await new Promise(resolve => setImmediate(resolve))
    expect(calls).toContainEqual(['open', sessionId])
    expect(calls).toContainEqual(['prompt', sessionId, 'run only after archive', 'queue'])
  })

  it('re-archives a residual Session after plugin reload and releases a failed resume owner', async () => {
    const firstRuntime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    let archiveAttempts = 0
    context.workspaces.archiveSession = async (sessionId) => {
      archiveAttempts += 1
      calls.push(['archive', sessionId])
      if (archiveAttempts <= 2)
        throw new Error(`fixture archive failed ${archiveAttempts}`)
    }
    const dispose = firstRuntime.plugin.apply(context)
    const sessionId = 'mir3-system-reload-residual'

    await firstRuntime.send(request('mir3/systemSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime', prompt: 'must stay blocked' },
      sessionId,
    }))
    expect(lastMessage(firstRuntime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('fixture archive failed 1') },
    })
    expect(calls.some(call => call[0] === 'open' || call[0] === 'prompt')).toBe(false)
    dispose()

    const resumedRuntime = loadAdapter()
    resumedRuntime.plugin.apply(context)
    await resumedRuntime.send(request('mir3/systemSession.resume', 1, { sessionId }))
    expect(lastMessage(resumedRuntime.messages)).toMatchObject({
      type: 'mir3/bridge.error',
      payload: { message: expect.stringContaining('fixture archive failed 2') },
    })
    expect(calls.some(call => call[0] === 'open' || call[0] === 'prompt')).toBe(false)

    await resumedRuntime.send(request('mir3/systemSession.resume', 2, {
      sessionId,
      taskId: 'task-resume-retry',
    }))
    expect(calls.filter(call => call[0] === 'session-create')).toHaveLength(1)
    expect(calls.filter(call => call[0] === 'archive')).toHaveLength(3)
    const successfulArchive = calls.findLastIndex(call => call[0] === 'archive')
    const open = calls.findIndex(call => call[0] === 'open')
    expect(successfulArchive).toBeLessThan(open)
    expect(lastMessage(resumedRuntime.messages)).toMatchObject({
      type: 'mir3/systemSession.resumed',
      taskId: 'task-resume-retry',
    })
  })

  it('rebuilds a global Session subscription after the plugin runtime reloads', async () => {
    const firstRuntime = loadAdapter()
    const calls = []
    const sessions = new Map()
    const context = createHarnessContext({ calls, sessions })
    const dispose = firstRuntime.plugin.apply(context)
    const sessionId = 'global-reload-runtime'

    await firstRuntime.send(request('mir3/globalSession.create', 1, {
      payload: { cwd: '/tmp/mir3-runtime' },
      sessionId,
      systemId: 'shop',
      taskId: 'global-task-runtime',
    }))
    dispose()

    const resumedRuntime = loadAdapter()
    resumedRuntime.plugin.apply(context)
    await resumedRuntime.send(request('mir3/globalSession.resume', 1, {
      sessionId,
      systemId: 'shop',
      taskId: 'global-task-runtime',
    }))

    expect(lastMessage(resumedRuntime.messages)).toMatchObject({
      type: 'mir3/globalSession.resumed',
      sessionId,
      taskId: 'global-task-runtime',
    })
    expect(calls.filter(call => call[0] === 'session-create' && call[1] === sessionId)).toHaveLength(1)
    expect(calls.filter(call => call[0] === 'open' && call[1] === sessionId)).toHaveLength(2)
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

function loadAdapter(referrer = 'https://studio.mir3.test/workbench') {
  const messages = []
  let descriptor
  let listener
  let removed = false
  let activated = false
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
    document: { referrer },
    window,
  })
  const module = { exports: {} }
  const plugin = descriptor.factory(module)
  return {
    listenerRemoved: () => removed,
    messages,
    plugin,
    async send(data, overrides = {}) {
      if (!activated
        && data.type !== 'mir3/project.activate'
        && (overrides.origin === undefined || overrides.origin === 'https://studio.mir3.test')
        && (overrides.source === undefined || overrides.source === parent)) {
        listener({
          data: request('mir3/project.activate', 1, {
            payload: {
              projectRoot: '/tmp/mir3-runtime',
              workspaceRoot: '/tmp/mir3-runtime',
              startSession: false,
            },
            sessionId: '',
            systemId: '__project__',
            taskId: 'project-activation',
          }),
          origin: 'https://studio.mir3.test',
          source: parent,
        })
        activated = true
        await new Promise(resolve => setImmediate(resolve))
      }
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
  const workspaces = []
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
        return sessionId
      },
      open(sessionId) {
        calls.push(['sessions-open', sessionId])
      },
    },
    workspaces: {
      list: {
        getSnapshot() {
          return { items: workspaces }
        },
      },
      async archiveSession(sessionId) {
        calls.push(['archive', sessionId])
      },
      async listDirectory(path) {
        calls.push(['workspace-list-directory', path])
        return { path, entries: [] }
      },
      async create({ path }) {
        workspaceSequence += 1
        calls.push(['workspace-create', path])
        const workspace = { path, workspaceId: `workspace-${workspaceSequence}` }
        workspaces.push(workspace)
        return workspace
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
