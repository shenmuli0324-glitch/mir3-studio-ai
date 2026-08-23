import type { StudioShellState, StudioView } from './studio-types'
import { useEffect, useRef, useState } from 'react'
import { If } from 'react-if-lite'
import { useStore } from 'valtio-define'
import { PluginRecovery } from '@/components/plugin-recovery'
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
  const [shellState, setShellState] = useState<StudioShellState>({
    activeView: DEFAULT_STUDIO_VIEW,
    sidebarCollapsed: false,
    project: null,
  })
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const { activeView, sidebarCollapsed } = shellState
  const harnessVisible = isHarnessView(activeView)
  const harnessSurface = harnessSurfaceFor(activeView)

  useEffect(() => {
    store.harness.startup()
  }, [])

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
    return <StudioViewContent view={activeView} />
  }

  return (
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
        />
        <div className="flex min-h-0 flex-1">
          <StudioSidebar activeView={activeView} collapsed={sidebarCollapsed} onNavigate={navigate} />
          <main className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-canvas">
            <HarnessWorkbench active={harnessVisible} iframeRef={iframeRef} surface={harnessSurface} />
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
  )
}

function studioPageClass(view: StudioView): string {
  const base = 'absolute inset-0 min-h-0 min-w-0'
  if (isHarnessView(view))
    return `${base} invisible pointer-events-none`
  return `${base} visible`
}
