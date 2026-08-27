import type { StudioShellState, StudioView } from './studio-types'
import type { CompositeDraftReviewRequest } from '@/features/devtools/domain/composite-draft-review'
import type { RegisteredGlobalTask, VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { Button } from '@heroui/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useStore } from 'valtio-define'
import { PluginRecovery } from '@/components/plugin-recovery'
import { DEV_TOOLS } from '@/features/devtools/devtool-registry'
import { getDomainResource, previewDomainDraft, recoverGlobalTaskScope, revokeTaskScope, validateDomainDraft } from '@/features/devtools/domain/api'
import { CompositeDraftReviewDialog } from '@/features/devtools/domain/composite-draft-review'
import { GuiDesignerScope } from '@/features/gui-designer/gui-designer-scope'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'
import { bridgeRequestId, postHarnessBridge, subscribeHarnessBridge } from '@/features/projects/workspace-bridge'
import { draftHandoffs, GLOBAL_WORKBENCH_EVENT, isCompletedGlobalTask, isGlobalDraftEvent, isGlobalTerminalEvent, markGlobalTaskMcpActive, markGlobalTaskMcpDisabled, markGlobalTaskReviewPending, registeredGlobalTask, registeredGlobalTasks, restoreGlobalTasks, returnTarget, unregisterGlobalTask, verifyDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { deliverGlobalTaskScope, recoverAndManageGlobalTaskScope } from '@/features/system-ai/global-task-recovery'
import { currentScopeLease, stopScopeLease } from '@/features/system-ai/scope-lease-manager'
import { HarnessWorkbench } from '@/features/workbench/harness-workbench'
import { useDshTheme } from '@/hooks/use-dsh-theme'
import { store } from '@/store'
import { StudioViewContent } from '@/views'
import { DesktopUpdater } from './components/desktop-updater'
import { DownloadToast } from './components/download-toast-trigger'
import { HarnessUpdater } from './components/harness-updater'
import { StartupGate } from './components/startup-gate'
import { StudioSidebar } from './components/studio-sidebar'
import { StudioTopbar } from './components/studio-topbar'
import { DEFAULT_STUDIO_VIEW, harnessSurfaceFor, isHarnessView } from './studio-types'
import '../i18n'

export function App() {
  const { t } = useTranslation()
  useDshTheme()
  const { status, recovery } = useStore(store.harness)
  const { activeProject, selectWorkspace } = useMir3Projects()
  const [shellState, setShellState] = useState<StudioShellState>({
    activeView: DEFAULT_STUDIO_VIEW,
    sidebarCollapsed: false,
    project: null,
  })
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const [devtoolsTarget, setDevtoolsTarget] = useState<VerifiedDevtoolsTarget | null>(null)
  const [compositeReview, setCompositeReview] = useState<CompositeDraftReviewRequest | null>(null)
  const [pendingCompositeReview, setPendingCompositeReview] = useState<CompositeDraftReviewRequest | null>(null)
  const globalResumeRequestsRef = useRef(new Set<string>())
  const { activeView, sidebarCollapsed } = shellState
  const harnessVisible = isHarnessView(activeView)
  const harnessSurface = harnessSurfaceFor(activeView)
  const visibleCompositeReview = reviewForActiveProject(compositeReview, activeProject?.id)
  const visiblePendingCompositeReview = reviewForActiveProject(pendingCompositeReview, activeProject?.id)

  useEffect(() => {
    store.harness.startup()
  }, [])

  useEffect(() => {
    const pending = registeredGlobalTasks().find(task => task.projectId === activeProject?.id && task.reviewPending)
    // eslint-disable-next-line react/set-state-in-effect
    setPendingCompositeReview(pending ? reviewRequest(pending) : null)
  }, [activeProject?.id])

  useEffect(() => {
    restoreGlobalTasks()
    function showGlobalWorkbench(event: Event) {
      const detail = (event as CustomEvent<{ projectId?: string }>).detail
      if (!detail?.projectId || detail.projectId !== activeProject?.id)
        return
      setShellState(value => ({ ...value, activeView: 'workbench' }))
    }
    window.addEventListener(GLOBAL_WORKBENCH_EVENT, showGlobalWorkbench)
    return () => window.removeEventListener(GLOBAL_WORKBENCH_EVENT, showGlobalWorkbench)
  }, [activeProject?.id])

  useEffect(() => {
    // 项目数据来自 Tauri Query；同步进壳层状态以保证顶栏、页面和工作台使用同一快照。
    // eslint-disable-next-line react/set-state-in-effect
    setShellState(value => ({ ...value, project: activeProject }))
  }, [activeProject])

  useEffect(() => {
    let disposed = false

    function postRecoveredGlobalScope(task: RegisteredGlobalTask, content: string): boolean {
      return postHarnessBridge({
        type: 'mir3/globalSession.prompt',
        projectId: task.projectId,
        systemId: task.systemId,
        taskId: task.taskId,
        sessionId: task.sessionId,
        payload: { content, mode: 'steer' },
      })
    }

    async function recoverAndResumeGlobalTask(task: RegisteredGlobalTask) {
      const key = globalTaskKey(task)
      if (globalResumeRequestsRef.current.has(key))
        return
      globalResumeRequestsRef.current.add(key)
      if (!task.reviewPending && !currentScopeLease(task)) {
        markGlobalTaskMcpDisabled(task, 'GLOBAL_TASK_SCOPE_RECOVERY_PENDING')
        try {
          await recoverAndManageGlobalTaskScope(task, {
            recover: (registered, previous) => recoverGlobalTaskScope(
              registered.projectId,
              registered.taskId,
              registered.compositeId,
              previous?.readSystems ?? registered.allowedSystems,
              previous?.writeSystems ?? registered.allowedWriteSystems ?? registered.allowedSystems,
              previous?.draftIds ?? registered.draftIds,
              previous?.pluginVersions ?? registered.handoff.pluginVersions,
            ),
            revoke: revokeTaskScope,
            postPrompt: postRecoveredGlobalScope,
            onActive: () => void markGlobalTaskMcpActive(task),
            onError: reason => void markGlobalTaskMcpDisabled(task, reason),
          })
        }
        catch (reason) {
          markGlobalTaskMcpDisabled(task, reason)
        }
      }
      const posted = postHarnessBridge({
        type: 'mir3/globalSession.resume',
        projectId: task.projectId,
        systemId: task.systemId,
        taskId: task.taskId,
        sessionId: task.sessionId,
        payload: {},
      })
      if (!posted) {
        globalResumeRequestsRef.current.delete(key)
        await stopScopeLease(task)
        markGlobalTaskMcpDisabled(task, 'HARNESS_BRIDGE_UNAVAILABLE: global Session resume was not delivered')
      }
    }

    const unsubscribe = subscribeHarnessBridge((message) => {
      if (message.type === 'mir3/plugin.ready' || message.type === 'mir3/bridge.description') {
        for (const task of registeredGlobalTasks()) {
          if (task.projectId === activeProject?.id)
            void recoverAndResumeGlobalTask(task)
        }
        return
      }
      if (!isGlobalDraftEvent(message.type))
        return
      const registration = registeredGlobalTask(message)
      if (!registration)
        return
      const completed = isCompletedGlobalTask(message)
      const terminal = isGlobalTerminalEvent(message.type)
      if (message.type === 'mir3/globalSession.resumed') {
        globalResumeRequestsRef.current.delete(globalTaskKey(registration))
        if (!completed) {
          const lease = currentScopeLease(registration)
          if (lease && deliverGlobalTaskScope(registration, lease, postRecoveredGlobalScope)) {
            markGlobalTaskMcpActive(registration)
          }
          else if (lease) {
            void stopScopeLease(registration)
            markGlobalTaskMcpDisabled(registration, 'GLOBAL_TASK_SCOPE_DELIVERY_FAILED')
          }
        }
      }
      if (completed) {
        const request = reviewRequest(registration)
        markGlobalTaskReviewPending(registration)
        if (registration.projectId === activeProject?.id) {
          setPendingCompositeReview(request)
          setCompositeReview(request)
        }
      }
      const requestedTarget = returnTarget(message, registration)
      const handoffs = draftHandoffs(message, registration)
      if (completed) {
        void stopScopeLease(registration)
      }
      else if (terminal) {
        void stopScopeLease(registration)
        globalResumeRequestsRef.current.delete(globalTaskKey(registration))
        if (message.type === 'mir3/globalSession.cancelled') {
          unregisterGlobalTask(registration)
          setPendingCompositeReview(value => sameReviewTask(value, registration) ? null : value)
          setCompositeReview(value => sameReviewTask(value, registration) ? null : value)
        }
        else {
          markGlobalTaskMcpDisabled(registration, bridgeFailure(message))
        }
      }
      if (!requestedTarget || requestedTarget.projectId !== activeProject?.id)
        return
      void verifyDevtoolsTarget(requestedTarget, handoffs, {
        isKnownSystem: systemId => DEV_TOOLS.some(tool => tool.id === systemId),
        getResource: getDomainResource,
        previewDraft: previewDomainDraft,
        validateDraft: validateDomainDraft,
        nonce: bridgeRequestId,
      }).then((verified) => {
        if (disposed || !verified)
          return
        setDevtoolsTarget(verified)
        if (completed)
          setShellState(value => ({ ...value, activeView: 'devtools' }))
      }).catch(() => {})
    })
    return () => {
      disposed = true
      unsubscribe()
    }
  }, [activeProject?.id])

  useEffect(() => {
    if (!import.meta.env.DEV)
      return
    function onKeyDown(event: KeyboardEvent) {
      if (!event.ctrlKey || !event.shiftKey)
        return
      if (event.code === 'Digit1') {
        event.preventDefault()
        store.harness.setRuntimeRecovery({
          plugins: ['dsh-better-sidebar'],
          reason: 'slot_conflict',
          detail: 'sidebar',
          raw_error: 'Preview: dsh-better-sidebar reported a UI slot conflict.',
        })
      }
      else if (event.code === 'Digit2') {
        event.preventDefault()
        store.harness.fail('Preview: plugin startup failure')
        store.harness.setRuntimeRecovery({
          plugins: ['dsh-better-sidebar'],
          reason: 'duplicate_loader_entry',
          detail: 'dshSidebarApi',
          raw_error: 'Preview: duplicate loader entry id: dshSidebarApi',
        })
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  function toggleSidebar() {
    setShellState(value => ({ ...value, sidebarCollapsed: !value.sidebarCollapsed }))
  }

  function navigate(view: StudioView) {
    setShellState(value => ({ ...value, activeView: view }))
  }

  function finishCompositeReview() {
    const review = compositeReview ?? pendingCompositeReview
    if (!review)
      return
    const task = registeredGlobalTasks().find(candidate => sameReviewTask(review, candidate))
    if (task) {
      postHarnessBridge({
        type: 'mir3/globalSession.complete',
        projectId: task.projectId,
        systemId: task.systemId,
        taskId: task.taskId,
        sessionId: task.sessionId,
        payload: {},
      })
    }
    void stopScopeLease(review)
    unregisterGlobalTask(review)
    setCompositeReview(null)
    setPendingCompositeReview(null)
  }

  function readyContent() {
    if (harnessVisible)
      return null
    return <StudioViewContent view={activeView} devtoolsTarget={devtoolsTarget} />
  }

  return (
    <GuiDesignerScope.Provider key={activeProject?.id ?? 'no-project'}>
      {guiScope => (
        <div className="flex h-screen w-screen flex-col bg-canvas">
          <If
            cond={status === 'ready'}
            else={<StartupGate status={status} recoveryRequired={recovery.required} iframeRef={iframeRef} />}
          >
            <StudioTopbar
              activeView={activeView}
              sidebarCollapsed={sidebarCollapsed}
              iframeRef={iframeRef}
              showSidebarToggle
              onToggleSidebar={toggleSidebar}
              project={shellState.project}
              onSelectWorkspace={() => {
                if (shellState.project)
                  void selectWorkspace(shellState.project.id)
              }}
            />
            <div className="flex min-h-0 flex-1">
              <StudioSidebar activeView={activeView} collapsed={sidebarCollapsed} guiDirty={guiScope.dirty} onNavigate={navigate} />
              <main className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-canvas">
                <HarnessWorkbench active={harnessVisible} iframeRef={iframeRef} surface={harnessSurface} project={shellState.project} />
                <div className={studioPageClass(activeView)}>{readyContent()}</div>
                <If cond={visiblePendingCompositeReview != null && visibleCompositeReview == null}>
                  <Button
                    className="absolute right-5 top-5 z-20 bg-accent text-white shadow-lg"
                    size="sm"
                    onPress={() => setCompositeReview(visiblePendingCompositeReview)}
                  >
                    {t('studio.composite_review.reopen')}
                  </Button>
                </If>
                <If cond={visibleCompositeReview != null}>
                  <CompositeDraftReviewDialog request={visibleCompositeReview!} onClose={() => setCompositeReview(null)} onApplied={finishCompositeReview} />
                </If>
              </main>
            </div>
          </If>
          <If cond={status === 'ready'}>
            <HarnessUpdater />
            <DownloadToast />
            <PluginRecovery />
          </If>
          <DesktopUpdater />
        </div>
      )}
    </GuiDesignerScope.Provider>
  )
}

function studioPageClass(view: StudioView): string {
  const base = 'absolute inset-0 min-h-0 min-w-0'
  if (isHarnessView(view))
    return `${base} invisible pointer-events-none`
  return `${base} visible`
}

function reviewForActiveProject(review: CompositeDraftReviewRequest | null, projectId?: string): CompositeDraftReviewRequest | null {
  if (!review || review.projectId !== projectId)
    return null
  return review
}

function reviewRequest(task: { projectId: string, compositeId: string, taskId: string, sessionId: string }): CompositeDraftReviewRequest {
  return {
    projectId: task.projectId,
    compositeId: task.compositeId,
    taskId: task.taskId,
    sessionId: task.sessionId,
  }
}

function sameReviewTask(review: CompositeDraftReviewRequest | null, task: { projectId: string, taskId: string, sessionId: string }): boolean {
  return review?.projectId === task.projectId
    && review.taskId === task.taskId
    && review.sessionId === task.sessionId
}

function globalTaskKey(task: Pick<RegisteredGlobalTask, 'projectId' | 'taskId' | 'sessionId'>): string {
  return `${task.projectId}\u241F${task.taskId}\u241F${task.sessionId}`
}

function bridgeFailure(message: { payload: unknown }): string {
  if (!message.payload || typeof message.payload !== 'object' || Array.isArray(message.payload))
    return 'HARNESS_BRIDGE_ERROR'
  const payload = message.payload as Record<string, unknown>
  const code = typeof payload.code === 'string' ? payload.code : 'HARNESS_BRIDGE_ERROR'
  const detail = typeof payload.message === 'string' ? payload.message : null
  return detail ? `${code}: ${detail}` : code
}
