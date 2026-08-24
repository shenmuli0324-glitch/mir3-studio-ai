window.__ModuleLoader__.load({
  id: '@mir3-studio/dsh-mir3-core',
  factory(module) {
    'use strict'

    const name = 'mir3-core-plugin'
    const inject = ['workspaces']
    const PROJECT_MESSAGE = 'mir3/project.activate'
    const WORKSPACE_PICK = 'mir3/workspace.pick'
    const ADD_WORKSPACE_LABEL = /添加.*工作区|add workspace/i

    function apply(ctx) {
      let activeProject = null

      function post(message) {
        window.parent.postMessage({ source: 'mir3-core-plugin', version: 1, ...message }, '*')
      }

      async function activate(message) {
        const payload = message.payload
        if (!payload || typeof payload.projectId !== 'string' || typeof payload.projectRoot !== 'string' || typeof payload.workspaceRoot !== 'string') {
          post({
            type: 'mir3/project.error',
            requestId: message.requestId,
            payload: { code: 'PROJECT_MESSAGE_INVALID', message: 'Invalid MIR3 project activation message.' },
          })
          return
        }
        try {
          const workspace = await ctx.workspaces.create({ path: payload.workspaceRoot })
          activeProject = payload
          ctx.workspaces.startSession(workspace.workspaceId)
          post({
            type: 'mir3/project.activated',
            requestId: message.requestId,
            payload: { workspaceId: workspace.workspaceId, canonicalPath: workspace.path },
          })
        }
        catch (error) {
          post({
            type: 'mir3/project.error',
            requestId: message.requestId,
            payload: { code: 'WORKSPACE_ACTIVATE_FAILED', message: String(error) },
          })
        }
      }

      function handleMessage(event) {
        if (event.source !== window.parent)
          return
        const message = event.data
        if (!message || typeof message !== 'object' || message.source !== 'mir3-studio' || message.version !== 1)
          return
        if (message.type === PROJECT_MESSAGE)
          void activate(message)
      }

      function interceptWorkspacePicker(event) {
        const target = event.target instanceof Element ? event.target.closest('button,[role="button"]') : null
        const label = target?.getAttribute('aria-label') || target?.textContent || ''
        if (!target || !ADD_WORKSPACE_LABEL.test(label))
          return
        event.preventDefault()
        event.stopPropagation()
        event.stopImmediatePropagation()
        post({
          type: WORKSPACE_PICK,
          requestId: `workspace-${Date.now()}`,
          payload: activeProject ? { projectId: activeProject.projectId } : {},
        })
      }

      window.addEventListener('message', handleMessage)
      document.addEventListener('click', interceptWorkspacePicker, true)
      post({ type: 'mir3/plugin.ready', requestId: `ready-${Date.now()}`, payload: {} })

      return () => {
        window.removeEventListener('message', handleMessage)
        document.removeEventListener('click', interceptWorkspacePicker, true)
      }
    }

    module.exports = { apply, inject, name }
    // Harness 的浏览器 ModuleLoader 使用 factory 的返回值作为插件导出。
    // 仅赋值 module.exports 而不返回会使 Cordis 收到 undefined。
    return module.exports
  },
})
