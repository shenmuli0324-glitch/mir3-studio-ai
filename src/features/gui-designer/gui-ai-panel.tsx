import type { Mir3BridgeEnvelope } from '@/features/projects/workspace-bridge'
import type { AiConversationMessage, AiPendingInteraction } from '@/features/system-ai/ai-conversation-panel'
import { ChevronLeft, ChevronRight, Sparkles } from '@gravity-ui/icons'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ensureHarnessProjectActive, postHarnessBridge, subscribeHarnessBridge } from '@/features/projects/workspace-bridge'
import { AiConversationPanel } from '@/features/system-ai/ai-conversation-panel'
import { projectTaskMessages } from '@/features/system-ai/global-task-handoff'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'

const GUI_SYSTEM_ID = '__studio_gui__'
const DEFAULT_PANEL_WIDTH = 360
const MIN_PANEL_WIDTH = 300
const MAX_PANEL_WIDTH = 640

interface GuiSessionSnapshot {
  nodes?: unknown[]
  partial?: unknown
  pending?: AiPendingInteraction[]
  running?: boolean
  openError?: string | null
  promptError?: string | null
}

export function GuiAiPanel() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const project = scope.activeProject
  const [collapsed, setCollapsed] = useState(false)
  const [width, setWidth] = useState(readPanelWidth)
  const [taskId] = useState(() => activeGuiTaskId(project?.id ?? 'unknown'))
  const [sessionId, setSessionId] = useState(() => activeGuiSessionId(project?.id ?? 'unknown'))
  const [connected, setConnected] = useState(false)
  const [sessionReady, setSessionReady] = useState(false)
  const [running, setRunning] = useState(false)
  const [messages, setMessages] = useState<AiConversationMessage[]>([])
  const [pending, setPending] = useState<AiPendingInteraction[]>([])
  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)
  const expectedSessionRef = useRef(sessionId)
  const resumedSessionRef = useRef('')
  const aiTurnRef = useRef<{ path: string, workingRevision: number } | null>(null)
  const scopeRef = useRef(scope)
  scopeRef.current = scope

  useEffect(() => {
    if (!project)
      return
    const unsubscribe = subscribeHarnessBridge((message) => {
      if (message.type === 'mir3/plugin.ready')
        setConnected(true)
      if (message.type === 'mir3/bridge.description' && guiSessionAvailable(message.payload))
        setConnected(true)
      if (!matchesGuiSession(message, project.id, taskId, expectedSessionRef.current))
        return
      if (message.type === 'mir3/bridge.error') {
        const nextError = bridgeError(message)
        if (isMissingGuiSession(nextError)) {
          forgetGuiSessionId(project.id)
          expectedSessionRef.current = ''
          resumedSessionRef.current = ''
          setSessionId('')
          setSessionReady(false)
          setPending([])
        }
        setError(nextError)
        aiTurnRef.current = null
        setRunning(false)
        return
      }
      if (message.type === 'mir3/guiSession.created') {
        setConnected(true)
        setSessionReady(false)
        postGuiSession('mir3/guiSession.snapshot', project.id, taskId, message.sessionId, {})
        return
      }
      if (message.type === 'mir3/guiSession.resumed' || message.type === 'mir3/guiSession.snapshot') {
        setSessionReady(true)
        applySessionSnapshot(message.payload as GuiSessionSnapshot, setMessages, setPending, setRunning, setError)
        return
      }
      if (message.type === 'mir3/guiSession.cancelled') {
        setRunning(false)
        aiTurnRef.current = null
        return
      }
      if (message.type === 'mir3/guiSession.completed') {
        setRunning(false)
        const turn = aiTurnRef.current
        aiTurnRef.current = null
        if (turn) {
          void scopeRef.current.completeAiTurn(turn.path, turn.workingRevision).then((applied) => {
            if (!applied)
              setError(t('studio.gui.ai.revision_conflict'))
          }).catch(reason => setError(String(reason)))
        }
      }
    })
    postHarnessBridge({
      type: 'mir3/bridge.describe',
      projectId: project.id,
      systemId: GUI_SYSTEM_ID,
      taskId,
      sessionId: '',
      payload: {},
    })
    return unsubscribe
  }, [project, t, taskId])

  useEffect(() => {
    if (!project || !connected || !sessionId || resumedSessionRef.current === sessionId)
      return
    let cancelled = false
    resumedSessionRef.current = sessionId
    void ensureHarnessProjectActive(project).then(() => {
      if (cancelled)
        return
      const posted = postGuiSession('mir3/guiSession.resume', project.id, taskId, sessionId, {})
      if (!posted) {
        resumedSessionRef.current = ''
        setError(t('studio.gui.ai.unavailable'))
      }
    }).catch((reason) => {
      if (!cancelled) {
        resumedSessionRef.current = ''
        setError(String(reason))
      }
    })
    return () => {
      cancelled = true
    }
  }, [connected, project, sessionId, t, taskId])

  async function sendPrompt() {
    const content = input.trim()
    if (!project || !scope.currentPath || !content)
      return
    if (!connected) {
      setError(t('studio.gui.ai.unavailable'))
      return
    }
    if (sessionId && !sessionReady) {
      setError(t('studio.devtools.ai.resuming'))
      return
    }
    try {
      await ensureHarnessProjectActive(project)
      const turn = await scope.beginAiTurn()
      aiTurnRef.current = { path: scope.currentPath, workingRevision: turn.workingRevision }
      setInput('')
      setError(null)
      setMessages(value => [...value, { id: guiRequestId(), role: 'user', content }])
      setRunning(true)
      if (!sessionId) {
        const nextSessionId = `mir3-gui-${guiRequestId()}`
        rememberGuiSessionId(project.id, nextSessionId)
        expectedSessionRef.current = nextSessionId
        resumedSessionRef.current = nextSessionId
        setSessionId(nextSessionId)
        const posted = postGuiSession('mir3/guiSession.create', project.id, taskId, nextSessionId, {
          cwd: project.activeWorkspaceRoot,
          prompt: guiScopedPrompt(content, turn.workspace.path, turn.workspace.workingRevision),
          workspaceId: turn.workspace.workspaceId,
          workspaceToken: turn.workspace.workspaceToken,
        })
        if (!posted)
          handleUnavailable()
        return
      }
      const posted = postGuiSession('mir3/guiSession.prompt', project.id, taskId, sessionId, {
        content: guiScopedPrompt(content, turn.workspace.path, turn.workspace.workingRevision),
        mode: 'queue',
        workspaceId: turn.workspace.workspaceId,
        workspaceToken: turn.workspace.workspaceToken,
      })
      if (!posted)
        handleUnavailable()
    }
    catch (reason) {
      aiTurnRef.current = null
      setRunning(false)
      setError(String(reason))
    }
  }

  function handleUnavailable() {
    aiTurnRef.current = null
    setRunning(false)
    setError(t('studio.gui.ai.unavailable'))
  }

  function cancel() {
    if (project && sessionId)
      postGuiSession('mir3/guiSession.cancel', project.id, taskId, sessionId, {})
  }

  function respond(pendingKey: string, response: unknown) {
    if (project && sessionId)
      postGuiSession('mir3/guiSession.respond', project.id, taskId, sessionId, { pendingKey, response })
  }

  function handleResizeStart(event: React.PointerEvent<HTMLDivElement>) {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = width
    function handlePointerMove(pointerEvent: PointerEvent) {
      const maxWidth = Math.min(MAX_PANEL_WIDTH, Math.round(window.innerWidth * 0.6))
      const nextWidth = Math.max(MIN_PANEL_WIDTH, Math.min(maxWidth, startWidth + startX - pointerEvent.clientX))
      setWidth(nextWidth)
    }
    function handlePointerUp() {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', handlePointerUp)
      setWidth((value) => {
        window.localStorage.setItem('mir3-gui-ai-panel-width', String(value))
        return value
      })
    }
    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', handlePointerUp)
  }

  const displayedError = error ?? (scope.diskConflict ? t('studio.gui.ai.disk_conflict') : null) ?? (scope.aiConflict ? t('studio.gui.ai.revision_conflict') : null)
  const fileContext = scope.currentPath ?? t('studio.gui.no_file')
  const nodeContext = scope.selectedNode?.name?.value ?? scope.selectedNode?.luaVariable ?? scope.selectedNode?.kind ?? t('studio.gui.ai.no_node')
  if (collapsed) {
    return (
      <aside className="flex h-full w-10 shrink-0 flex-col items-center border-l border-line bg-panel py-2">
        <button className="grid size-8 place-items-center rounded-lg text-accent hover:bg-panel-hover" type="button" title={t('studio.gui.ai.expand')} aria-label={t('studio.gui.ai.expand')} onClick={() => setCollapsed(false)}>
          <ChevronLeft className="size-4" />
        </button>
        <Sparkles className="mt-3 size-4 text-accent" />
      </aside>
    )
  }
  return (
    <aside className="relative flex h-full min-h-0 shrink-0 flex-col overflow-hidden border-l border-line bg-panel" style={{ width }}>
      <div className="absolute inset-y-0 left-0 z-20 w-1 cursor-col-resize hover:bg-accent/40" role="separator" aria-orientation="vertical" aria-label={t('studio.gui.ai.resize')} onPointerDown={handleResizeStart} onDoubleClick={() => setWidth(DEFAULT_PANEL_WIDTH)} />
      <header className="flex h-11 shrink-0 items-center gap-2 border-b border-line px-3">
        <Sparkles className="size-4 text-accent" />
        <strong className="text-xs font-semibold text-ink">{t('studio.gui.ai.title')}</strong>
        <span className="rounded-full bg-accent/10 px-2 py-0.5 text-[9px] text-accent">{t('studio.gui.ai.capabilities')}</span>
        <button className="ml-auto grid size-7 place-items-center rounded-lg text-muted hover:bg-panel-hover hover:text-ink" type="button" title={t('studio.gui.ai.collapse')} aria-label={t('studio.gui.ai.collapse')} onClick={() => setCollapsed(true)}>
          <ChevronRight className="size-4" />
        </button>
      </header>
      <div className="shrink-0 border-b border-line bg-panel-2/40 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2 text-[10px]">
          <span className="min-w-0 flex-1 truncate text-ink" title={fileContext}>{fileContext}</span>
          <span className="max-w-28 truncate text-muted" title={nodeContext}>{nodeContext}</span>
        </div>
      </div>
      <AiConversationPanel
        messages={messages}
        running={running}
        pending={pending}
        error={displayedError}
        input={input}
        placeholder={t('studio.gui.ai.placeholder')}
        sendDisabled={!scope.currentFile || scope.currentFile.valid === false || scope.diskConflict != null}
        scopeControl={<span className="px-2 text-[10px] text-muted">{t('studio.gui.ai.scope')}</span>}
        emptyState={<GuiAiEmpty />}
        onInputChange={setInput}
        onSend={() => void sendPrompt()}
        onCancel={cancel}
        onRespond={respond}
      />
    </aside>
  )
}

function GuiAiEmpty() {
  const { t } = useTranslation()
  return (
    <div className="flex min-h-60 flex-col items-center justify-center px-5 text-center">
      <span className="grid size-10 place-items-center rounded-xl bg-accent/10 text-accent"><Sparkles className="size-5" /></span>
      <strong className="mt-3 text-xs font-medium text-ink">{t('studio.gui.ai.empty_title')}</strong>
      <p className="mt-2 text-[11px] leading-5 text-muted">{t('studio.gui.ai.empty_desc')}</p>
    </div>
  )
}

function activeGuiTaskId(projectId: string): string {
  const key = `mir3-gui-task:${projectId}`
  const stored = window.localStorage.getItem(key)
  if (stored)
    return stored
  const taskId = `gui-${projectId}-${guiRequestId()}`
  window.localStorage.setItem(key, taskId)
  return taskId
}

function activeGuiSessionId(projectId: string): string {
  return window.localStorage.getItem(guiSessionStorageKey(projectId)) ?? ''
}

function rememberGuiSessionId(projectId: string, sessionId: string) {
  window.localStorage.setItem(guiSessionStorageKey(projectId), sessionId)
}

function forgetGuiSessionId(projectId: string) {
  window.localStorage.removeItem(guiSessionStorageKey(projectId))
}

function guiSessionStorageKey(projectId: string): string {
  return `mir3-gui-session:${projectId}`
}

function readPanelWidth(): number {
  const stored = Number(window.localStorage.getItem('mir3-gui-ai-panel-width'))
  if (!Number.isFinite(stored))
    return DEFAULT_PANEL_WIDTH
  return Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, stored))
}

function matchesGuiSession(message: Mir3BridgeEnvelope, projectId: string, taskId: string, sessionId: string): boolean {
  if (message.projectId !== projectId || message.systemId !== GUI_SYSTEM_ID || message.taskId !== taskId)
    return false
  if (!sessionId)
    return false
  return message.sessionId === sessionId
}

function guiSessionAvailable(payload: unknown): boolean {
  if (!payload || typeof payload !== 'object')
    return false
  const capabilities = (payload as { capabilities?: Record<string, unknown> }).capabilities
  return capabilities?.guiSession === true
}

function applySessionSnapshot(
  snapshot: GuiSessionSnapshot,
  setMessages: (messages: AiConversationMessage[]) => void,
  setPending: (pending: AiPendingInteraction[]) => void,
  setRunning: (running: boolean) => void,
  setError: (error: string | null) => void,
) {
  const projected = projectTaskMessages(snapshot.nodes ?? [], snapshot.partial)
  if (projected.length > 0)
    setMessages(projected)
  setPending(snapshot.pending ?? [])
  setRunning(Boolean(snapshot.running))
  setError(snapshot.openError ?? snapshot.promptError ?? null)
}

function postGuiSession(type: string, projectId: string, taskId: string, sessionId: string, payload: unknown): boolean {
  return postHarnessBridge({ type, projectId, systemId: GUI_SYSTEM_ID, taskId, sessionId, payload })
}

function guiScopedPrompt(content: string, path: string, workingRevision: number): string {
  return `[MIR3 GUI Workspace] path=${path}; workingRevision=${workingRevision}.\n[MIR3 User Request JSON]\n${JSON.stringify(content)}\n[/MIR3 User Request JSON]`
}

function bridgeError(message: Mir3BridgeEnvelope): string {
  const payload = message.payload as { code?: string, message?: string }
  return [payload.code, payload.message].filter(Boolean).join(': ')
}

function isMissingGuiSession(error: string): boolean {
  return error.includes('SYSTEM_SESSION_NOT_FOUND')
}

function guiRequestId(): string {
  if (typeof crypto.randomUUID === 'function')
    return crypto.randomUUID()
  return `gui-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
