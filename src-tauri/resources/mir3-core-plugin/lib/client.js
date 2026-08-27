window.__ModuleLoader__.load({
  id: '@mir3-studio/dsh-mir3-core',
  factory(module) {
    'use strict'

    const name = 'mir3-core-plugin'
    const inject = ['workspaces', 'sessions']
    const PROTOCOL_VERSION = 2
    const SOURCE = 'mir3-core-plugin'
    const SYSTEM_SESSION_PREFIX = 'mir3-system-'
    const GLOBAL_SESSION_PREFIX = 'global-'
    const activeBindings = new Map()
    const bindingActivity = new Map()
    const returnTargets = new Map()
    const sessionOwners = new Map()
    const sessionPreparations = new Set()
    const recoverableSystemSessions = new Set()
    const inboundSequences = new Map()
    const outboundSequences = new Map()
    const parentOrigin = resolveParentOrigin()

    function sequenceKey(message) {
      return `${message.projectId}\u241F${message.taskId}\u241F${message.sessionId}`
    }

    function bindingKey(message) {
      return sequenceKey(message)
    }

    function acceptInboundSequence(message) {
      const key = sequenceKey(message)
      const previous = inboundSequences.get(key) || 0
      if (message.sequence <= previous)
        return false
      inboundSequences.set(key, message.sequence)
      return true
    }

    function nextOutboundSequence(message) {
      const key = sequenceKey(message)
      const sequence = (outboundSequences.get(key) || 0) + 1
      outboundSequences.set(key, sequence)
      return sequence
    }

    function resolveParentOrigin() {
      try {
        const referrer = new URL(document.referrer)
        if (referrer.origin !== 'null')
          return referrer.origin
        if (referrer.protocol === 'tauri:' && referrer.host)
          return `${referrer.protocol}//${referrer.host}`
        return null
      }
      catch {
        return null
      }
    }

    function apply(ctx) {
      let bridgePort = null
      let activeProject = null
      const originalWorkspaceMethods = wrapWorkspaceBoundary()
      const originalSessionCreate = ctx.sessions.create.bind(ctx.sessions)
      ctx.sessions.create = async (options = {}) => {
        requireActiveSessionTarget(options)
        return originalSessionCreate(options)
      }

      function wrapWorkspaceBoundary() {
        const methods = {}
        for (const method of ['create', 'pickDirectory', 'listDirectory', 'createDirectory', 'openPath']) {
          if (typeof ctx.workspaces?.[method] === 'function')
            methods[method] = ctx.workspaces[method].bind(ctx.workspaces)
        }
        if (methods.create) {
          ctx.workspaces.create = async (input) => {
            requireProjectPath(input?.path)
            return methods.create(input)
          }
        }
        if (methods.pickDirectory) {
          ctx.workspaces.pickDirectory = async () => {
            requireActiveProject()
            const selected = await methods.pickDirectory()
            if (selected != null)
              requireProjectPath(selected)
            return selected
          }
        }
        if (methods.listDirectory) {
          ctx.workspaces.listDirectory = async (path, signal) => {
            const target = path ?? requireActiveProject().projectRoot
            requireProjectPath(target)
            return methods.listDirectory(target, signal)
          }
        }
        if (methods.createDirectory) {
          ctx.workspaces.createDirectory = async (path, name) => {
            requireProjectPath(path)
            return methods.createDirectory(path, name)
          }
        }
        if (methods.openPath) {
          ctx.workspaces.openPath = async (path) => {
            requireProjectPath(path)
            return methods.openPath(path)
          }
        }
        return methods
      }

      function requireActiveProject(request) {
        if (!activeProject)
          throw new Error('PROJECT_SCOPE_UNAVAILABLE: activate a MIR3 project before starting development')
        if (request?.projectId && request.projectId !== activeProject.projectId)
          throw new Error('PROJECT_SCOPE_MISMATCH: request belongs to another MIR3 project')
        return activeProject
      }

      function requireProjectPath(path, request) {
        const project = requireActiveProject(request)
        if (typeof path !== 'string' || !isWithinPath(project.projectRoot, path))
          throw new Error('PROJECT_PATH_OUTSIDE_SCOPE: development paths must stay inside the active MIR3 project')
        return path
      }

      function requireActiveSessionTarget(options, request) {
        const project = requireActiveProject(request)
        if (typeof options.cwd === 'string') {
          requireProjectPath(options.cwd, request)
          return
        }
        if (typeof options.workspaceId !== 'string')
          throw new Error('SESSION_SCOPE_UNAVAILABLE: cwd or a project Workspace is required')
        const workspace = ctx.workspaces
          ?.list
          ?.getSnapshot?.()
          .items
          ?.find(item => item.workspaceId === options.workspaceId)
        if (!workspace || !isWithinPath(project.projectRoot, workspace.path))
          throw new Error('SESSION_WORKSPACE_OUTSIDE_SCOPE: Session Workspace must belong to the active MIR3 project')
      }

      function post(type, request, payload) {
        const message = {
          source: SOURCE,
          protocolVersion: PROTOCOL_VERSION,
          type,
          requestId: request.requestId,
          projectId: request.projectId,
          systemId: request.systemId,
          taskId: request.taskId,
          sessionId: request.sessionId,
          sequence: nextOutboundSequence(request),
          payload,
        }
        if (bridgePort) {
          bridgePort.postMessage(message)
          return
        }
        if (!parentOrigin)
          return
        try {
          window.parent.postMessage(message, parentOrigin)
        }
        catch {
          // macOS 的 Tauri 自定义协议是 opaque origin，等待宿主下发 MessagePort。
        }
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
          && typeof message.sessionId === 'string'
          && Number.isSafeInteger(message.sequence)
          && message.sequence > 0
          && Object.hasOwn(message, 'payload')
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
            ordinarySessionCanary: typeof ctx.sessions?.create === 'function' && typeof ctx.workspaces?.archiveSession === 'function',
            projectScope: true,
          },
        })
      }

      async function activateProject(request) {
        const payload = request.payload || {}
        if (typeof payload.projectRoot !== 'string' || typeof payload.workspaceRoot !== 'string')
          throw new Error('PROJECT_MESSAGE_INVALID: projectRoot and workspaceRoot are required')
        if (!isWithinPath(payload.projectRoot, payload.workspaceRoot))
          throw new Error('PROJECT_WORKSPACE_OUTSIDE_SCOPE: workspaceRoot must be inside projectRoot')
        activeProject = {
          projectId: request.projectId,
          projectRoot: normalizePath(payload.projectRoot),
          workspaceRoot: normalizePath(payload.workspaceRoot),
        }
        const workspace = await ctx.workspaces.create({ path: payload.workspaceRoot })
        if (payload.startSession !== false)
          ctx.workspaces.startSession(workspace.workspaceId)
        post('mir3/project.activated', request, {
          workspaceId: workspace.workspaceId,
          canonicalPath: workspace.path,
        })
      }

      async function canaryOrdinarySession(request) {
        const payload = request.payload || {}
        const sessionId = String(payload.sessionId || '')
        if (!sessionId.startsWith('harness-canary-') || isSystemSessionId(sessionId) || isGlobalSessionId(sessionId) || typeof payload.cwd !== 'string')
          throw new Error('ORDINARY_SESSION_CANARY_INVALID: an unreserved canary sessionId and cwd are required')
        requireActiveProject(request)
        requireResult(await ctx.sessions.create({ cwd: payload.cwd, sessionId }), 'ORDINARY_SESSION_CANARY_CREATE_FAILED')
        let openError = null
        try {
          const session = requireSession(sessionId)
          await session.open()
        }
        catch (error) {
          openError = error
        }
        await ctx.workspaces.archiveSession(sessionId)
        if (openError)
          throw openError
        post('mir3/bridge.ordinarySessionCanary', { ...request, sessionId }, {
          sessionId,
          managed: false,
          archived: true,
        })
      }

      async function createSystemSession(request) {
        const payload = request.payload || {}
        if (!ctx.sessions || typeof ctx.sessions.create !== 'function')
          throw new Error('SYSTEM_SESSION_UNSUPPORTED: sessions.create is unavailable')
        if (!isSystemSessionId(request.sessionId) || typeof payload.cwd !== 'string')
          throw new Error('SYSTEM_SESSION_INVALID: sessionId and cwd are required')
        requireScopedIdentity(request)
        requireProjectPath(payload.cwd, request)
        if (sessionPreparations.has(request.sessionId))
          throw new Error('SYSTEM_SESSION_CREATE_IN_PROGRESS: session is already being prepared')
        sessionPreparations.add(request.sessionId)
        let session
        try {
          claimSessionOwner(request)
          if (!recoverableSystemSessions.has(request.sessionId)) {
            const created = await ctx.sessions.create({ cwd: payload.cwd, sessionId: request.sessionId })
            requireResult(created, 'SYSTEM_SESSION_CREATE_FAILED')
            recoverableSystemSessions.add(request.sessionId)
          }
          session = requireSession(request.sessionId)
          if (typeof ctx.workspaces?.archiveSession !== 'function')
            throw new Error('SYSTEM_SESSION_ARCHIVE_UNSUPPORTED: archiveSession is unavailable')
          await ctx.workspaces.archiveSession(request.sessionId)
          await session.open()
          bindSession(request, session, 'mir3/systemSession')
          recoverableSystemSessions.delete(request.sessionId)
        }
        catch (error) {
          releaseSessionOwner(request)
          throw error
        }
        finally {
          sessionPreparations.delete(request.sessionId)
        }
        post('mir3/systemSession.created', request, { archived: true, created: true })
        if (typeof payload.prompt === 'string' && payload.prompt.trim())
          requireResult(await session.prompt(textContent(payload.prompt), 'queue'), 'SYSTEM_SESSION_PROMPT_FAILED')
      }

      async function resumeSystemSession(request) {
        requireScopedIdentity(request)
        requireActiveProject(request)
        if (sessionPreparations.has(request.sessionId))
          throw new Error('SYSTEM_SESSION_PREPARATION_IN_PROGRESS: session is not archived and ready yet')
        sessionPreparations.add(request.sessionId)
        try {
          claimSessionOwner(request)
          const session = requireSystemSession(request.sessionId)
          if (typeof ctx.workspaces?.archiveSession !== 'function')
            throw new Error('SYSTEM_SESSION_ARCHIVE_UNSUPPORTED: archiveSession is unavailable')
          await ctx.workspaces.archiveSession(request.sessionId)
          await session.open()
          bindSession(request, session, 'mir3/systemSession')
          recoverableSystemSessions.delete(request.sessionId)
          post('mir3/systemSession.resumed', request, projectSnapshot(session.getSnapshot(), request))
        }
        catch (error) {
          releaseSessionOwner(request)
          throw error
        }
        finally {
          sessionPreparations.delete(request.sessionId)
        }
      }

      function bindSession(request, session, eventPrefix) {
        const key = bindingKey(request)
        const old = activeBindings.get(key)
        if (typeof old === 'function')
          old()
        bindingActivity.set(key, Boolean(session.getSnapshot()?.running))
        const dispose = session.subscribe(() => {
          const snapshot = session.getSnapshot()
          const wasRunning = bindingActivity.get(key) === true
          const running = Boolean(snapshot?.running)
          bindingActivity.set(key, running)
          post(`${eventPrefix}.snapshot`, request, projectSnapshot(snapshot, request))
          if (wasRunning && !running)
            post(`${eventPrefix}.completed`, request, projectSnapshot(snapshot, request))
        })
        activeBindings.set(key, dispose)
      }

      async function promptSystemSession(request) {
        requireSessionOwner(request)
        const session = requireSystemSession(request.sessionId)
        const content = String(request.payload?.content || '').trim()
        if (!content)
          throw new Error('SYSTEM_SESSION_PROMPT_INVALID: content is required')
        const result = await session.prompt(textContent(content), request.payload?.mode === 'steer' ? 'steer' : 'queue')
        requireResult(result, 'SYSTEM_SESSION_PROMPT_FAILED')
        post('mir3/systemSession.prompted', request, {})
      }

      async function cancelSystemSession(request) {
        requireSessionOwner(request)
        const session = requireSystemSession(request.sessionId)
        requireResult(await session.cancel(), 'SYSTEM_SESSION_CANCEL_FAILED')
        post('mir3/systemSession.cancelled', request, {})
      }

      async function respondSystemSession(request) {
        requireSessionOwner(request)
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
        requireSessionOwner(request)
        const session = requireSystemSession(request.sessionId)
        post('mir3/systemSession.snapshot', request, projectSnapshot(session.getSnapshot(), request))
      }

      async function completeSystemSession(request) {
        if (!isSystemSessionId(request.sessionId))
          throw new Error('SYSTEM_SESSION_SCOPE_UNVERIFIED: reserved Studio session id is required')
        requireSessionOwner(request)
        const session = requireSystemSession(request.sessionId)
        const snapshot = projectSnapshot(session.getSnapshot(), request)
        const key = bindingKey(request)
        const dispose = activeBindings.get(key)
        if (typeof dispose === 'function')
          dispose()
        activeBindings.delete(key)
        bindingActivity.delete(key)
        returnTargets.delete(key)
        sessionOwners.delete(request.sessionId)
        post('mir3/systemSession.completed', request, snapshot)
      }

      async function createGlobalSession(request) {
        const payload = request.payload || {}
        if (typeof payload.cwd !== 'string' || !payload.cwd.trim())
          throw new Error('GLOBAL_SESSION_INVALID: cwd is required')
        if (!isGlobalSessionId(request.sessionId))
          throw new Error('GLOBAL_SESSION_SCOPE_UNVERIFIED: reserved Studio global session id is required')
        requireScopedIdentity(request)
        requireProjectPath(payload.cwd, request)
        claimSessionOwner(request)
        const workspace = await ctx.workspaces.create({ path: payload.cwd })
        const globalSessionId = request.sessionId
        requireResult(
          await ctx.sessions.create({ workspaceId: workspace.workspaceId, sessionId: globalSessionId }),
          'GLOBAL_SESSION_CREATE_FAILED',
        )
        ctx.sessions.open(globalSessionId)
        const session = requireSession(globalSessionId)
        await session.open()
        returnTargets.set(bindingKey(request), cloneable(payload.structuredContext?.returnTo || null))
        bindSession(request, session, 'mir3/globalSession')
        post('mir3/globalSession.created', { ...request, sessionId: globalSessionId }, {
          workspaceId: workspace.workspaceId,
          sessionId: globalSessionId,
        })
        if (typeof payload.prompt === 'string' && payload.prompt.trim())
          requireResult(await session.prompt(textContent(payload.prompt), 'queue'), 'GLOBAL_SESSION_PROMPT_FAILED')
      }

      async function resumeGlobalSession(request) {
        if (!isGlobalSessionId(request.sessionId))
          throw new Error('GLOBAL_SESSION_SCOPE_UNVERIFIED: reserved Studio global session id is required')
        requireScopedIdentity(request)
        requireActiveProject(request)
        try {
          claimSessionOwner(request)
          const session = requireSession(request.sessionId)
          await session.open()
          bindSession(request, session, 'mir3/globalSession')
          post('mir3/globalSession.resumed', request, projectSnapshot(session.getSnapshot(), request))
        }
        catch (error) {
          releaseSessionOwner(request)
          throw error
        }
      }

      async function promptGlobalSession(request) {
        if (!isGlobalSessionId(request.sessionId))
          throw new Error('GLOBAL_SESSION_SCOPE_UNVERIFIED: reserved Studio global session id is required')
        requireSessionOwner(request)
        const session = requireSession(request.sessionId)
        const content = String(request.payload?.content || '').trim()
        if (!content)
          throw new Error('GLOBAL_SESSION_PROMPT_INVALID: content is required')
        requireResult(await session.prompt(textContent(content), request.payload?.mode === 'steer' ? 'steer' : 'queue'), 'GLOBAL_SESSION_PROMPT_FAILED')
        post('mir3/globalSession.prompted', request, {})
      }

      async function cancelGlobalSession(request) {
        if (!isGlobalSessionId(request.sessionId))
          throw new Error('GLOBAL_SESSION_SCOPE_UNVERIFIED: reserved Studio global session id is required')
        requireSessionOwner(request)
        const session = requireSession(request.sessionId)
        requireResult(await session.cancel(), 'GLOBAL_SESSION_CANCEL_FAILED')
        post('mir3/globalSession.cancelled', request, projectSnapshot(session.getSnapshot(), request))
      }

      async function completeGlobalSession(request) {
        if (!isGlobalSessionId(request.sessionId))
          throw new Error('GLOBAL_SESSION_SCOPE_UNVERIFIED: reserved Studio global session id is required')
        requireSessionOwner(request)
        const session = requireSession(request.sessionId)
        const snapshot = projectSnapshot(session.getSnapshot(), request)
        const key = bindingKey(request)
        const dispose = activeBindings.get(key)
        if (typeof dispose === 'function')
          dispose()
        activeBindings.delete(key)
        bindingActivity.delete(key)
        returnTargets.delete(key)
        sessionOwners.delete(request.sessionId)
        post('mir3/globalSession.completed', request, snapshot)
      }

      function requireSession(sessionId) {
        const session = ctx.sessions?.binding(sessionId)?.session
        if (!session)
          throw new Error(`SYSTEM_SESSION_NOT_FOUND: ${sessionId}`)
        return session
      }

      function isSystemSessionId(sessionId) {
        return typeof sessionId === 'string' && sessionId.startsWith(SYSTEM_SESSION_PREFIX) && sessionId.length > SYSTEM_SESSION_PREFIX.length
      }

      function isGlobalSessionId(sessionId) {
        return typeof sessionId === 'string' && sessionId.startsWith(GLOBAL_SESSION_PREFIX) && sessionId.length > GLOBAL_SESSION_PREFIX.length
      }

      function requireScopedIdentity(request) {
        if (!request.projectId || !request.systemId || !request.taskId || !request.sessionId)
          throw new Error('SESSION_IDENTITY_INVALID: projectId, systemId, taskId, and sessionId are required')
      }

      function claimSessionOwner(request) {
        const owner = sessionOwner(request)
        const previous = sessionOwners.get(request.sessionId)
        if (previous && previous !== owner)
          throw new Error('SESSION_IDENTITY_MISMATCH: session is bound to another task')
        sessionOwners.set(request.sessionId, owner)
      }

      function releaseSessionOwner(request) {
        if (sessionOwners.get(request.sessionId) === sessionOwner(request))
          sessionOwners.delete(request.sessionId)
      }

      function sessionOwner(request) {
        return `${request.projectId}\u241F${request.systemId}\u241F${request.taskId}`
      }

      function requireSessionOwner(request) {
        requireScopedIdentity(request)
        if (sessionPreparations.has(request.sessionId))
          throw new Error('SYSTEM_SESSION_PREPARATION_IN_PROGRESS: session is not archived and ready yet')
        const owner = sessionOwner(request)
        if (sessionOwners.get(request.sessionId) !== owner)
          throw new Error('SESSION_IDENTITY_MISMATCH: session is not bound to this task')
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
          case 'mir3/bridge.ordinarySessionCanary': return canaryOrdinarySession(message)
          case 'mir3/systemSession.create': return createSystemSession(message)
          case 'mir3/systemSession.resume': return resumeSystemSession(message)
          case 'mir3/systemSession.prompt': return promptSystemSession(message)
          case 'mir3/systemSession.cancel': return cancelSystemSession(message)
          case 'mir3/systemSession.respond': return respondSystemSession(message)
          case 'mir3/systemSession.snapshot': return snapshotSystemSession(message)
          case 'mir3/systemSession.complete': return completeSystemSession(message)
          case 'mir3/globalSession.create': return createGlobalSession(message)
          case 'mir3/globalSession.resume': return resumeGlobalSession(message)
          case 'mir3/globalSession.prompt': return promptGlobalSession(message)
          case 'mir3/globalSession.cancel': return cancelGlobalSession(message)
          case 'mir3/globalSession.complete': return completeGlobalSession(message)
          default: throw new Error(`BRIDGE_MESSAGE_UNKNOWN: ${message.type}`)
        }
      }

      function handleMessage(event) {
        if (event.source === window.parent
          && event.data?.source === 'mir3-studio'
          && event.data?.protocolVersion === PROTOCOL_VERSION
          && event.data?.type === 'mir3/bridge.port'
          && event.ports?.[0]) {
          bridgePort?.close()
          bridgePort = event.ports[0]
          inboundSequences.clear()
          outboundSequences.clear()
          bridgePort.addEventListener('message', handlePortMessage)
          bridgePort.start()
          postReady()
          return
        }
        if (event.source !== window.parent || !parentOrigin || event.origin !== parentOrigin)
          return
        dispatchMessage(event.data)
      }

      function handlePortMessage(event) {
        dispatchMessage(event.data)
      }

      function dispatchMessage(message) {
        if (!validate(message))
          return
        if (!acceptInboundSequence(message))
          return
        void dispatch(message).catch(error => postError(message, 'BRIDGE_REQUEST_FAILED', error))
      }

      function postReady() {
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
      }

      window.addEventListener('message', handleMessage)
      postReady()

      return () => {
        window.removeEventListener('message', handleMessage)
        bridgePort?.close()
        bridgePort = null
        for (const dispose of activeBindings.values()) {
          if (typeof dispose === 'function')
            dispose()
        }
        activeBindings.clear()
        bindingActivity.clear()
        returnTargets.clear()
        sessionOwners.clear()
        sessionPreparations.clear()
        recoverableSystemSessions.clear()
        inboundSequences.clear()
        outboundSequences.clear()
        ctx.sessions.create = originalSessionCreate
        for (const [method, original] of Object.entries(originalWorkspaceMethods))
          ctx.workspaces[method] = original
        activeProject = null
      }
    }

    function normalizePath(value) {
      const source = String(value).replace(/\\/g, '/')
      const drive = source.match(/^([a-z]:)\//i)?.[1] ?? null
      const unc = !drive && source.startsWith('//')
      if (!drive && !unc && !source.startsWith('/'))
        return null
      const prefix = drive ? `${drive}/` : unc ? '//' : '/'
      const offset = drive ? drive.length + 1 : unc ? 2 : 1
      const parts = []
      for (const part of source.slice(offset).split('/')) {
        if (!part || part === '.')
          continue
        if (part === '..') {
          if (parts.length === 0)
            return null
          parts.pop()
          continue
        }
        parts.push(part)
      }
      const normalized = `${prefix}${parts.join('/')}`.replace(/\/$/, '') || prefix
      return drive || unc ? normalized.toLowerCase() : normalized
    }

    function isWithinPath(root, candidate) {
      const normalizedRoot = normalizePath(root)
      const normalizedCandidate = normalizePath(candidate)
      const descendantPrefix = normalizedRoot === '/' ? '/' : `${normalizedRoot}/`
      return Boolean(normalizedRoot && normalizedCandidate
        && (normalizedCandidate === normalizedRoot || normalizedCandidate.startsWith(descendantPrefix)))
    }

    function projectSnapshot(snapshot, request) {
      if (!snapshot || typeof snapshot !== 'object')
        return { nodes: [], runningCalls: [], running: false, domainResults: [], returnTo: cloneable(returnTargets.get(request ? bindingKey(request) : '') || null) }
      const nodes = cloneable(Array.isArray(snapshot.nodes) ? snapshot.nodes : [])
      return {
        sessionId: snapshot.sessionId || null,
        nodes,
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
        domainResults: projectDomainResults(nodes, request?.systemId),
        returnTo: projectReturnTarget(nodes) || cloneable(returnTargets.get(request ? bindingKey(request) : '') || null),
      }
    }

    function projectReturnTarget(nodes) {
      if (!Array.isArray(nodes))
        return null
      for (const node of nodes) {
        if (!isToolNode(node))
          continue
        const candidates = []
        collectStructuredValues(node, candidates, new Set(), 0)
        for (const candidate of candidates) {
          const target = candidate?.returnTo
          if (target && typeof target === 'object' && target.view === 'devtools')
            return cloneable(target)
        }
      }
      return null
    }

    function projectDomainResults(nodes, fallbackSystemId) {
      if (!Array.isArray(nodes))
        return []
      const results = new Map()
      for (const node of nodes) {
        if (!isToolNode(node))
          continue
        const candidates = []
        collectStructuredValues(node, candidates, new Set(), 0)
        const nodeSystemId = findStringField(node, 'systemId') || fallbackSystemId
        for (const candidate of candidates) {
          const projected = projectDomainResult(candidate, nodeSystemId)
          if (!projected)
            continue
          results.set(`${projected.systemId}\u241F${projected.draftId}`, projected)
        }
      }
      return [...results.values()]
    }

    function projectDomainResult(value, fallbackSystemId) {
      if (!value || typeof value !== 'object' || Array.isArray(value))
        return null
      const draft = value.draft && typeof value.draft === 'object' ? value.draft : null
      const previewDraft = value.preview?.draft && typeof value.preview.draft === 'object' ? value.preview.draft : null
      const draftId = value.draftId || draft?.id || previewDraft?.id
      const revision = value.revision ?? draft?.revision ?? previewDraft?.revision
      const systemId = value.systemId || fallbackSystemId
      if (typeof draftId !== 'string' || typeof systemId !== 'string' || !Number.isSafeInteger(revision) || revision < 0)
        return null
      const validation = projectValidation(value.validation || value.draftValidation || value.report)
      const changedResources = uniqueStrings([
        ...stringArray(value.changedResources),
        ...stringArray(value.resourceIds),
        typeof value.resourceId === 'string' ? value.resourceId : null,
      ].filter(Boolean))
      return {
        draftId,
        revision,
        systemId,
        validation,
        changedResources,
        resourceId: typeof value.resourceId === 'string' ? value.resourceId : null,
      }
    }

    function projectValidation(value) {
      if (!value || typeof value !== 'object' || typeof value.valid !== 'boolean')
        return null
      return {
        valid: value.valid,
        diagnostics: stringArray(value.diagnostics).slice(0, 500),
      }
    }

    function collectStructuredValues(value, output, visited, depth) {
      if (depth > 12 || value === null || value === undefined)
        return
      if (typeof value === 'string') {
        const trimmed = value.trim()
        if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
          try {
            collectStructuredValues(JSON.parse(trimmed), output, visited, depth + 1)
          }
          catch {}
        }
        return
      }
      if (typeof value !== 'object' || visited.has(value))
        return
      visited.add(value)
      if (!Array.isArray(value))
        output.push(value)
      for (const item of Array.isArray(value) ? value : Object.values(value))
        collectStructuredValues(item, output, visited, depth + 1)
    }

    function findStringField(value, field, visited = new Set(), depth = 0) {
      if (!value || typeof value !== 'object' || visited.has(value) || depth > 10)
        return null
      visited.add(value)
      if (!Array.isArray(value) && typeof value[field] === 'string')
        return value[field]
      for (const item of Array.isArray(value) ? value : Object.values(value)) {
        const found = findStringField(item, field, visited, depth + 1)
        if (found)
          return found
      }
      return null
    }

    function isToolNode(value) {
      if (!value || typeof value !== 'object' || Array.isArray(value))
        return false
      const type = String(value.type || value.kind || '').toLowerCase()
      return type.includes('tool') || typeof value.tool === 'string' || typeof value.toolName === 'string'
    }

    function stringArray(value) {
      if (!Array.isArray(value))
        return []
      return value.filter(item => typeof item === 'string')
    }

    function uniqueStrings(values) {
      return [...new Set(values)]
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
