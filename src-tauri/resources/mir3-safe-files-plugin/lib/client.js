window.__ModuleLoader__.load({
  id: '@mir3-studio/dsh-mir3-safe-files',
  factory(require) {
    'use strict'

    const module = { exports: {} }
    const React = require('react')
    const inject = ['betterSidebar']
    const name = 'mir3-safe-files-client'
    const pending = new Map()
    let activeProject = null

    function post(type, payload, requestId) {
      window.parent.postMessage({
        source: 'mir3-safe-files-plugin',
        type,
        version: 1,
        requestId: requestId || `safe-${Date.now()}-${Math.random().toString(16).slice(2)}`,
        payload: payload || {},
      }, '*')
    }

    function request(command, payload) {
      const requestId = `safe-${Date.now()}-${Math.random().toString(16).slice(2)}`
      return new Promise((resolve, reject) => {
        const timer = window.setTimeout(() => {
          pending.delete(requestId)
          reject(new Error(`MIR3 Safe Files request timed out: ${command}`))
        }, 30000)
        pending.set(requestId, { resolve, reject, timer })
        post('mir3/files.request', { command, ...payload }, requestId)
      })
    }

    function relativePath(path) {
      if (!activeProject)
        throw new Error('尚未绑定 MIR3 项目')
      const root = activeProject.projectRoot.replace(/\\/g, '/').replace(/\/$/, '')
      const normalized = path.replace(/\\/g, '/')
      const insensitive = /^[A-Z]:\//i.test(root)
      const matches = insensitive
        ? normalized.toLowerCase().startsWith(`${root.toLowerCase()}/`)
        : normalized.startsWith(`${root}/`)
      if (!matches)
        throw new Error('文件不在当前 MIR3 项目内')
      return normalized.slice(root.length + 1)
    }

    function badge(text) {
      return React.createElement('span', {
        style: { border: '1px solid #555', borderRadius: 12, padding: '2px 8px', color: '#bbb', fontSize: 12 },
      }, text)
    }

    function SafeTextViewer(props) {
      const opened = props.customData
      const [value, setValue] = React.useState(opened.content)
      const [savedValue, setSavedValue] = React.useState(opened.content)
      const [draftId, setDraftId] = React.useState(opened.draftId || null)
      const [revision, setRevision] = React.useState(opened.revision || 0)
      const [state, setState] = React.useState('idle')
      const dirty = value !== savedValue

      React.useEffect(() => {
        post('mir3/files.dirty', { dirty, relativePath: opened.relativePath })
        return () => post('mir3/files.dirty', { dirty: false, relativePath: opened.relativePath })
      }, [dirty, opened.relativePath])

      async function save() {
        if (!dirty || state === 'saving')
          return
        setState('saving')
        try {
          const command = opened.relativePath.toLowerCase().endsWith('.lua') ? 'safe_lua_patch' : 'safe_text_patch'
          const result = await request(command, {
            projectId: opened.projectId,
            operation: {
              relativePath: opened.relativePath,
              draftId,
              expectedRevision: revision,
              expectedSha256: opened.sha256,
              originalContent: savedValue,
              newContent: value,
              newline: opened.mixedNewlines ? opened.newline : null,
            },
          })
          setDraftId(result.draftId)
          setRevision(result.revision)
          setSavedValue(value)
          setState('saved')
          post('mir3/files.mode', { mode: 'draft', draftId: result.draftId })
        }
        catch (error) {
          setState(`error: ${String(error)}`)
          post('mir3/files.error', { message: String(error) })
        }
      }

      return React.createElement('div', { style: { height: '100%', display: 'flex', flexDirection: 'column', background: '#151515', color: '#eee' } }, React.createElement('div', { style: { display: 'flex', gap: 8, alignItems: 'center', padding: '8px 12px', borderBottom: '1px solid #333' } }, badge('安全编辑：已启用'), badge(`编码：${opened.encoding}`), badge(`换行：${opened.mixedNewlines ? '混合（只允许无换行编辑）' : opened.newline || '无'}`), React.createElement('span', { style: { flex: 1 } }), React.createElement('span', { style: { color: state.startsWith('error') ? '#ff7b72' : '#999', fontSize: 12 } }, state === 'saved' ? '已保存到 Draft' : state), React.createElement('button', { type: 'button', disabled: !dirty || state === 'saving', onClick: save, style: { padding: '6px 14px', borderRadius: 6, border: '1px solid #666', background: dirty ? '#b8860b' : '#333', color: '#fff' } }, '保存到 Draft')), React.createElement('textarea', {
        value,
        onChange: event => setValue(event.target.value),
        spellCheck: false,
        style: { flex: 1, resize: 'none', border: 0, outline: 0, padding: 14, background: '#151515', color: '#eee', fontFamily: 'ui-monospace, SFMono-Regular, Consolas, monospace', fontSize: 14, lineHeight: 1.6 },
      }))
    }

    function SafeXlsGrid(props) {
      const data = props.data
      const viewportRef = React.useRef(null)
      const [scrollTop, setScrollTop] = React.useState(0)
      const [viewportHeight, setViewportHeight] = React.useState(600)
      const rowHeight = 32
      const rowNumberWidth = 56
      const columnWidth = 140
      const overscan = 10
      const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan)
      const visibleCount = Math.ceil(viewportHeight / rowHeight) + overscan * 2
      const end = Math.min(data.rowCount, start + visibleCount)
      const visibleRows = data.rows.slice(start, end)

      React.useEffect(() => {
        const viewport = viewportRef.current
        if (!viewport)
          return undefined
        function measure() {
          setViewportHeight(viewport.clientHeight || 600)
        }
        const observer = new ResizeObserver(measure)
        observer.observe(viewport)
        return () => observer.disconnect()
      }, [])

      return React.createElement('div', {
        ref: viewportRef,
        onScroll: event => setScrollTop(event.currentTarget.scrollTop),
        style: { flex: 1, overflow: 'auto', position: 'relative' },
      }, React.createElement('div', {
        style: {
          height: data.rowCount * rowHeight,
          minWidth: rowNumberWidth + data.columnCount * columnWidth,
          position: 'relative',
        },
      }, visibleRows.map((row, visibleIndex) => {
        const rowIndex = start + visibleIndex
        const cells = row.map((cell, columnIndex) => ({
          cell,
          key: `${data.sheet}:row:${rowIndex}:column:${columnIndex}`,
        }))
        return React.createElement('div', {
          key: `${data.sheet}:row:${rowIndex}`,
          style: {
            display: 'flex',
            height: rowHeight,
            left: 0,
            position: 'absolute',
            right: 0,
            top: rowIndex * rowHeight,
          },
        }, React.createElement('div', {
          style: {
            alignItems: 'center',
            background: '#222',
            borderBottom: '1px solid #333',
            borderRight: '1px solid #3b3b3b',
            color: '#999',
            display: 'flex',
            flex: `0 0 ${rowNumberWidth}px`,
            justifyContent: 'center',
            left: 0,
            position: 'sticky',
            zIndex: 1,
          },
        }, String(rowIndex + 1)), cells.map(item => React.createElement('div', {
          key: item.key,
          title: item.cell,
          style: {
            alignItems: 'center',
            borderBottom: '1px solid #333',
            borderRight: '1px solid #333',
            display: 'flex',
            flex: `0 0 ${columnWidth}px`,
            overflow: 'hidden',
            padding: '0 8px',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          },
        }, item.cell)))
      })))
    }

    function SafeXlsViewer(props) {
      const workbook = props.customData
      const [sheet, setSheet] = React.useState(workbook.sheets[0]?.name || '')
      const [sheetData, setSheetData] = React.useState(null)
      const [error, setError] = React.useState('')

      function handleSheetChange(event) {
        setSheet(event.target.value)
        setSheetData(null)
        setError('')
      }

      React.useEffect(() => {
        let active = true
        if (!sheet)
          return undefined
        request('safe_xls_sheet_read', {
          projectId: activeProject.projectId,
          relativePath: workbook.relativePath,
          sheet,
          expectedSha256: workbook.sha256,
        })
          .then(result => active && setSheetData(result))
          .catch(reason => active && setError(String(reason)))
        return () => {
          active = false
        }
      }, [sheet, workbook.relativePath, workbook.sha256])

      return React.createElement('div', {
        style: { height: '100%', display: 'flex', flexDirection: 'column', background: '#151515', color: '#eee' },
      }, React.createElement('div', {
        style: { display: 'flex', gap: 8, alignItems: 'center', padding: 10, background: '#1d1d1d', borderBottom: '1px solid #333' },
      }, badge('BIFF XLS 只读'), badge('完整工作表'), React.createElement('select', {
        value: sheet,
        onChange: handleSheetChange,
        style: { background: '#222', color: '#eee', border: '1px solid #555', padding: 6 },
      }, workbook.sheets.map(value => React.createElement('option', {
        key: value.name,
        value: value.name,
      }, `${value.name} · ${value.rowCount} 行 × ${value.columnCount} 列`))), React.createElement('span', { style: { flex: 1 } }), sheetData
        ? badge(`${sheetData.rowCount} 行 × ${sheetData.columnCount} 列`)
        : null), error
        ? React.createElement('div', { style: { padding: 20, color: '#ff7b72' } }, error)
        : null, sheetData
        ? React.createElement(SafeXlsGrid, { data: sheetData })
        : React.createElement('div', { style: { padding: 20, color: '#999' } }, '正在读取完整工作表…'))
    }

    function handleMessage(event) {
      if (event.source !== window.parent)
        return
      const message = event.data
      if (!message || message.source !== 'mir3-studio' || message.version !== 1)
        return
      if (message.type === 'mir3/project.activate') {
        activeProject = message.payload
        return
      }
      if (message.type !== 'mir3/files.response' || !message.requestId)
        return
      const request = pending.get(message.requestId)
      if (!request)
        return
      window.clearTimeout(request.timer)
      pending.delete(message.requestId)
      if (message.payload?.error)
        request.reject(new Error(message.payload.error))
      else
        request.resolve(message.payload?.result)
    }

    function apply(ctx) {
      if (typeof window.__MIR3_SAFE_FILES_DISPOSE__ === 'function')
        window.__MIR3_SAFE_FILES_DISPOSE__()
      window.addEventListener('message', handleMessage)
      const disposers = []
      try {
        disposers.push(ctx.betterSidebar.registerFileViewer({
          id: 'mir3-safe-text',
          title: 'MIR3 Safe TXT',
          exts: ['txt'],
          priority: 200,
          fetchStrategy: 'custom',
          load: path => request('safe_file_open', { projectId: activeProject?.projectId, relativePath: relativePath(path), draftId: null }),
          component: SafeTextViewer,
        }))
        disposers.push(ctx.betterSidebar.registerFileViewer({
          id: 'mir3-safe-lua',
          title: 'MIR3 Safe Lua',
          exts: ['lua'],
          priority: 200,
          fetchStrategy: 'custom',
          load: path => request('safe_file_open', { projectId: activeProject?.projectId, relativePath: relativePath(path), draftId: null }),
          component: SafeTextViewer,
        }))
        disposers.push(ctx.betterSidebar.registerFileViewer({
          id: 'mir3-safe-xls',
          title: 'MIR3 BIFF XLS',
          exts: ['xls'],
          priority: 200,
          fetchStrategy: 'custom',
          load: path => request('safe_xls_open', { projectId: activeProject?.projectId, relativePath: relativePath(path) }),
          component: SafeXlsViewer,
        }))
      }
      catch (error) {
        for (const dispose of disposers)
          dispose()
        window.removeEventListener('message', handleMessage)
        post('mir3/files.error', { message: String(error), mode: 'native' })
        return () => {}
      }
      post('mir3/files.ready', { mode: 'draft' })
      function disposePlugin() {
        window.removeEventListener('message', handleMessage)
        for (const dispose of disposers)
          dispose()
        for (const item of pending.values()) {
          window.clearTimeout(item.timer)
          item.reject(new Error('MIR3 Safe Files was disabled'))
        }
        pending.clear()
        if (window.__MIR3_SAFE_FILES_DISPOSE__ === disposePlugin)
          delete window.__MIR3_SAFE_FILES_DISPOSE__
      }
      window.__MIR3_SAFE_FILES_DISPOSE__ = disposePlugin
      return disposePlugin
    }

    module.exports = { apply, inject, name }
    return module.exports
  },
})
