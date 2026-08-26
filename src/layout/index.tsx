import type { StudioShellState, StudioView } from './studio-types'
import type { DevtoolsReturnTarget, VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { useEffect, useRef, useState } from 'react'
import { If } from 'react-if-lite'
import { useStore } from 'valtio-define'
import { PluginRecovery } from '@/components/plugin-recovery'
import { DEV_TOOLS } from '@/features/devtools/devtool-registry'
import { getDomainResource, previewDomainDraft, validateDomainDraft } from '@/features/devtools/domain/api'
import { GuiDesignerScope } from '@/features/gui-designer/gui-designer-scope'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'
import { bridgeRequestId, subscribeHarnessBridge } from '@/features/projects/workspace-bridge'
import { draftHandoffs, registeredGlobalTask, returnTarget, unregisterGlobalTask } from '@/features/system-ai/ai-handoff'
import { stopScopeLease } from '@/features/system-ai/scope-lease-manager'
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
  const { activeView, sidebarCollapsed } = shellState
  const harnessVisible = isHarnessView(activeView)
  const harnessSurface = harnessSurfaceFor(activeView)

  useEffect(() => {
    store.harness.startup()
  }, [])

  useEffect(() => {
    // 项目数据来自 Tauri Query；同步进壳层状态以保证顶栏、页面和工作台使用同一快照。
    // eslint-disable-next-line react/set-state-in-effect
    setShellState(value => ({ ...value, project: activeProject }))
  }, [activeProject])

  useEffect(() => {
    let disposed = false
    const unsubscribe = subscribeHarnessBridge((message) => {
      if (message.type !== 'mir3/globalSession.completed')
        return
      const registration = registeredGlobalTask(message)
      if (!registration)
        return
      const requestedTarget = returnTarget(message, registration)
      const handoffs = draftHandoffs(message, registration)
      stopScopeLease(registration)
      unregisterGlobalTask(registration)
      if (!requestedTarget || requestedTarget.projectId !== activeProject?.id)
        return
      void verifyDevtoolsTarget(requestedTarget, handoffs).then((verified) => {
        if (disposed || !verified)
          return
        setDevtoolsTarget(verified)
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

async function verifyDevtoolsTarget(
  target: DevtoolsReturnTarget,
  handoffs: ReturnType<typeof draftHandoffs>,
): Promise<VerifiedDevtoolsTarget | null> {
  if (!DEV_TOOLS.some(tool => tool.id === target.systemId))
    return null
  let relativePath: string | null = null
  if (target.resourceId) {
    const resource = await getDomainResource(target.projectId, target.systemId, target.resourceId)
    if (resource.systemId !== target.systemId || resource.id !== target.resourceId || !resource.files[0])
      return null
    relativePath = resource.files[0].path
  }
  const reportedDraft = handoffs.find(handoff => handoff.systemId === target.systemId && (!target.draftId || handoff.draftId === target.draftId))
  const draftId = target.draftId ?? reportedDraft?.draftId ?? null
  let revision: number | null = reportedDraft?.revision ?? null
  if (draftId) {
    const [preview, validation] = await Promise.all([
      previewDomainDraft(target.projectId, draftId),
      validateDomainDraft(target.projectId, draftId),
    ])
    if (validation.systemId !== target.systemId || (revision != null && preview.preview.draft.revision < revision))
      return null
    revision = preview.preview.draft.revision
  }
  return {
    ...target,
    draftId,
    relativePath,
    revision,
    nonce: bridgeRequestId(),
  }
}

function studioPageClass(view: StudioView): string {
  const base = 'absolute inset-0 min-h-0 min-w-0'
  if (isHarnessView(view))
    return `${base} invisible pointer-events-none`
  return `${base} visible`
}
