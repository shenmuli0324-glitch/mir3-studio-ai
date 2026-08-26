window.__ModuleLoader__.load({
  id: '@mir3-studio/dsh-mir3-core',
  factory(module) {
    'use strict'

    const name = 'mir3-core-plugin'
    const inject = ['workspaces', 'sessions']
    const PROTOCOL_VERSION = 2
    const SOURCE = 'mir3-core-plugin'
    const SYSTEM_SESSION_PREFIX = 'mir3-system-'
    const activeBindings = new Map()
    const parentOrigin = resolveParentOrigin()

    function resolveParentOrigin() {
      try {
        return new URL(document.referrer).origin
      }
      catch {
        return null
      }
    }

    function apply(ctx) {
      function post(type, request, payload) {
        if (!parentOrigin)
          return
        window.parent.postMessage({
          source: SOURCE,
          protocolVersion: PROTOCOL_VERSION,
          type,
          requestId: request.requestId,
          projectId: request.projectId,
          systemId: request.systemId,
          taskId: request.taskId,
          sessionId: request.sessionId,
          sequence: request.sequence || 0,
          payload,
        }, parentOrigin)
      }

      function postError(request, code, error) {
        post('mir3/bridge.error', request, { code, message: String(error) })
      }

      function validate(message) {
        return message
          && typeof message === 'object'
          && message.source === 'mir3-studio'
          && message.protocolVersion === PROTOCOL_VERSION
          && typeof message.requestId === 'string'
          && typeof message.projectId === 'string'
          && typeof message.systemId === 'string'
          && typeof message.taskId === 'string'
          && Number.isSafeInteger(message.sequence)
          && message.sequence >= 0
      }

      async function describe(request) {
        post('mir3/bridge.description', request, {
          protocolVersion: PROTOCOL_VERSION,
          capabilities: {
            sessions: typeof ctx.sessions?.create === 'function' && typeof ctx.sessions?.binding === 'function',
            workspaces: typeof ctx.workspaces?.create === 'function' && typeof ctx.workspaces?.startSession === 'function',
            archive: typeof ctx.workspaces?.archiveSession === 'function',
            snapshot: true,
            pendingInteraction: true,
            globalSession: typeof ctx.sessions?.open === 'function',
          },
        })
      }

      async function activateProject(request) {
        const payload = request.payload || {}
        if (typeof payload.workspaceRoot !== 'string')
          throw new Error('PROJECT_MESSAGE_INVALID: workspaceRoot is required')
        const workspace = await ctx.workspaces.create({ path: payload.workspaceRoot })
        if (payload.startSession !== false)
          ctx.workspaces.startSession(workspace.workspaceId)
        post('mir3/project.activated', request, {
          workspaceId: workspace.workspaceId,
          canonicalPath: workspace.path,
        })
      }

      async function createSystemSession(request) {
        const payload = request.payload || {}
        if (!ctx.sessions || typeof ctx.sessions.create !== 'function')
          throw new Error('SYSTEM_SESSION_UNSUPPORTED: sessions.create is unavailable')
        if (!isSystemSessionId(request.sessionId) || typeof payload.cwd !== 'string')
          throw new Error('SYSTEM_SESSION_INVALID: sessionId and cwd are required')
        const created = await ctx.sessions.create({ cwd: payload.cwd, sessionId: request.sessionId })
        requireResult(created, 'SYSTEM_SESSION_CREATE_FAILED')
        if (typeof ctx.workspaces?.archiveSession !== 'function')
          throw new Error('SYSTEM_SESSION_ARCHIVE_UNSUPPORTED: archiveSession is unavailable')
        await ctx.workspaces.archiveSession(request.sessionId)
        const session = requireSession(request.sessionId)
        await session.open()
        bindSession(request, session)
        post('mir3/systemSession.created', request, { archived: true, created: true })
        if (typeof payload.prompt === 'string' && payload.prompt.trim())
          requireResult(await session.prompt(textContent(payload.prompt), 'queue'), 'SYSTEM_SESSION_PROMPT_FAILED')
      }

      async function resumeSystemSession(request) {
        const session = requireSystemSession(request.sessionId)
        await session.open()
        bindSession(request, session)
        post('mir3/systemSession.resumed', request, projectSnapshot(session.getSnapshot()))
      }

      function bindSession(request, session) {
        const old = activeBindings.get(request.taskId)
        if (typeof old === 'function')
          old()
        let sequence = 0
        const dispose = session.subscribe(() => {
          sequence += 1
          post('mir3/systemSession.snapshot', { ...request, sequence }, projectSnapshot(session.getSnapshot()))
        })
        activeBindings.set(request.taskId, dispose)
      }

      async function promptSystemSession(request) {
        const session = requireSystemSession(request.sessionId)
        const content = String(request.payload?.content || '').trim()
        if (!content)
          throw new Error('SYSTEM_SESSION_PROMPT_INVALID: content is required')
        const result = await session.prompt(textContent(content), request.payload?.mode === 'steer' ? 'steer' : 'queue')
        requireResult(result, 'SYSTEM_SESSION_PROMPT_FAILED')
        post('mir3/systemSession.prompted', request, {})
      }

      async function cancelSystemSession(request) {
        const session = requireSystemSession(request.sessionId)
        requireResult(await session.cancel(), 'SYSTEM_SESSION_CANCEL_FAILED')
        post('mir3/systemSession.cancelled', request, {})
      }

      async function respondSystemSession(request) {
        const session = requireSystemSession(request.sessionId)
        const snapshot = session.getSnapshot()
        const pending = Array.isArray(snapshot?.pending) ? snapshot.pending : []
        const pendingKey = request.payload?.pendingKey
        const wait = pendingKey ? pending.find(item => item.key === pendingKey) : pending[0]
        if (!wait || typeof wait.respond !== 'function')
          throw new Error('SYSTEM_SESSION_NO_PENDING_INTERACTION: no pending interaction')
        const receipt = await wait.respond(encodePendingResponse(wait, request.payload?.response))
        if (!receipt?.accepted)
          throw new Error(`SYSTEM_SESSION_RESPONSE_REJECTED: ${receipt?.reason || 'unknown reason'}`)
        post('mir3/systemSession.responded', request, { pendingKey: wait.key })
      }

      async function snapshotSystemSession(request) {
        const session = requireSystemSession(request.sessionId)
        post('mir3/systemSession.snapshot', request, projectSnapshot(session.getSnapshot()))
      }

      async function completeSystemSession(request) {
        if (!isSystemSessionId(request.sessionId))
          throw new Error('SYSTEM_SESSION_SCOPE_UNVERIFIED: reserved Studio session id is required')
        const dispose = activeBindings.get(request.taskId)
        if (typeof dispose === 'function')
          dispose()
        activeBindings.delete(request.taskId)
        post('mir3/systemSession.completed', request, {})
      }

      async function createGlobalSession(request) {
        const payload = request.payload || {}
        if (typeof payload.cwd !== 'string' || !payload.cwd.trim())
          throw new Error('GLOBAL_SESSION_INVALID: cwd is required')
        const workspace = await ctx.workspaces.create({ path: payload.cwd })
        const globalSessionId = typeof payload.sessionId === 'string' && payload.sessionId
          ? payload.sessionId
          : `global-${request.requestId}`
        requireResult(
          await ctx.sessions.create({ workspaceId: workspace.workspaceId, sessionId: globalSessionId }),
          'GLOBAL_SESSION_CREATE_FAILED',
        )
        ctx.sessions.open(globalSessionId)
        const session = requireSession(globalSessionId)
        await session.open()
        if (typeof payload.prompt === 'string' && payload.prompt.trim())
          requireResult(await session.prompt(textContent(payload.prompt), 'queue'), 'GLOBAL_SESSION_PROMPT_FAILED')
        post('mir3/globalSession.created', { ...request, sessionId: globalSessionId }, {
          workspaceId: workspace.workspaceId,
          sessionId: globalSessionId,
        })
      }

      function requireSession(sessionId) {
        const session = ctx.sessions?.binding(sessionId)?.session
        if (!session)
          throw new Error(`SYSTEM_SESSION_NOT_FOUND: ${sessionId}`)
        return session
      }

      function isSystemSessionId(sessionId) {
        return typeof sessionId === 'string' && sessionId.startsWith(SYSTEM_SESSION_PREFIX)
      }

      function requireSystemSession(sessionId) {
        if (!isSystemSessionId(sessionId))
          throw new Error('SYSTEM_SESSION_SCOPE_UNVERIFIED: reserved Studio session id is required')
        return requireSession(sessionId)
      }

      async function dispatch(message) {
        switch (message.type) {
          case 'mir3/bridge.describe': return describe(message)
          case 'mir3/project.activate': return activateProject(message)
          case 'mir3/systemSession.create': return createSystemSession(message)
          case 'mir3/systemSession.resume': return resumeSystemSession(message)
          case 'mir3/systemSession.prompt': return promptSystemSession(message)
          case 'mir3/systemSession.cancel': return cancelSystemSession(message)
          case 'mir3/systemSession.respond': return respondSystemSession(message)
          case 'mir3/systemSession.snapshot': return snapshotSystemSession(message)
          case 'mir3/systemSession.complete': return completeSystemSession(message)
          case 'mir3/globalSession.create': return createGlobalSession(message)
          default: throw new Error(`BRIDGE_MESSAGE_UNKNOWN: ${message.type}`)
        }
      }

      function handleMessage(event) {
        if (event.source !== window.parent || !parentOrigin || event.origin !== parentOrigin)
          return
        if (!validate(event.data))
          return
        void dispatch(event.data).catch(error => postError(event.data, 'BRIDGE_REQUEST_FAILED', error))
      }

      window.addEventListener('message', handleMessage)
      post('mir3/plugin.ready', {
        requestId: `ready-${Date.now()}`,
        projectId: '',
        systemId: '',
        taskId: '',
        sessionId: '',
        sequence: 0,
      }, {
        protocolVersion: PROTOCOL_VERSION,
      })

      return () => {
        window.removeEventListener('message', handleMessage)
        for (const dispose of activeBindings.values()) {
          if (typeof dispose === 'function')
            dispose()
        }
        activeBindings.clear()
      }
    }

    function projectSnapshot(snapshot) {
      if (!snapshot || typeof snapshot !== 'object')
        return { nodes: [], runningCalls: [], running: false }
      return {
        sessionId: snapshot.sessionId || null,
        nodes: cloneable(Array.isArray(snapshot.nodes) ? snapshot.nodes : []),
        partial: cloneable(snapshot.partial || null),
        runningCalls: cloneable(Array.isArray(snapshot.runningCalls) ? snapshot.runningCalls : []),
        pending: projectPending(snapshot.pending),
        queue: cloneable(Array.isArray(snapshot.queue) ? snapshot.queue : []),
        running: Boolean(snapshot.running),
        composerPhase: snapshot.composerPhase || null,
        blank: Boolean(snapshot.blank),
        openState: snapshot.openState || null,
        openError: projectError(snapshot.openError),
        promptError: projectError(snapshot.promptError),
        lastAgentError: projectError(snapshot.lastAgentError),
      }
    }

    function projectPending(pending) {
      if (!Array.isArray(pending))
        return []
      return pending.map(item => ({
        key: item.key || null,
        kind: item.kind || 'interaction',
        payload: cloneable(item.payload || {}),
      }))
    }

    function textContent(text) {
      return [{ type: 'text', text }]
    }

    function requireResult(result, prefix) {
      if (!result?.ok)
        throw new Error(`${prefix}: ${result?.error?.code || 'unknown'}: ${result?.error?.message || 'operation failed'}`)
      return result.value
    }

    function encodePendingResponse(wait, response) {
      if (response?.cancelled) {
        return {
          ok: false,
          error: { code: 'cancelled', message: 'the user cancelled the interaction', details: {} },
        }
      }
      if (wait.kind === 'approval') {
        const outcome = response?.outcome
        if (outcome !== 'allowed-once' && outcome !== 'rejected')
          throw new Error('SYSTEM_SESSION_RESPONSE_INVALID: approval outcome is required')
        return {
          ok: true,
          value: { sessionId: wait.sessionId, approvalId: wait.payload.approvalId, outcome },
        }
      }
      if (wait.kind === 'question') {
        if (!response || !('answer' in response))
          throw new Error('SYSTEM_SESSION_RESPONSE_INVALID: question answer is required')
        return { ok: true, value: { sessionId: wait.sessionId, answer: response.answer } }
      }
      throw new Error(`SYSTEM_SESSION_RESPONSE_UNSUPPORTED: ${wait.kind}`)
    }

    function projectError(error) {
      if (!error)
        return null
      return cloneable(error)
    }

    function cloneable(value, depth = 0) {
      if (value === null || value === undefined)
        return value ?? null
      if (depth > 12)
        return '[max-depth]'
      if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean')
        return value
      if (typeof value === 'bigint')
        return value.toString()
      if (Array.isArray(value))
        return value.map(item => cloneable(item, depth + 1))
      if (value instanceof Map)
        return [...value.entries()].map(([key, item]) => [cloneable(key, depth + 1), cloneable(item, depth + 1)])
      if (typeof value === 'object') {
        const output = {}
        for (const [key, item] of Object.entries(value)) {
          if (typeof item !== 'function')
            output[key] = cloneable(item, depth + 1)
        }
        return output
      }
      return String(value)
    }

    module.exports = { apply, inject, name }
    return module.exports
  },
})
