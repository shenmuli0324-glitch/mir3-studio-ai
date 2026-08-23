import type { RefObject } from 'react'
import type { SetupStatus } from '@/store/modules/harness/types'
import { If } from 'react-if-lite'
import { PluginRecovery } from '@/components/plugin-recovery'
import { PreinstallSetup } from './preinstall-setup'
import { Setup } from './setup'
import { StudioTopbar } from './studio-topbar'

export function StartupGate({ status, recoveryRequired, iframeRef }: {
  status: SetupStatus
  recoveryRequired: boolean
  iframeRef: RefObject<HTMLIFrameElement | null>
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      <StudioTopbar activeView="project" sidebarCollapsed={false} iframeRef={iframeRef} showSidebarToggle={false} onToggleSidebar={() => {}} />
      <div className="min-h-0 flex-1">
        <If cond={status === 'error'}>
          <If cond={recoveryRequired} else={<Setup />}>
            <PluginRecovery fullScreen />
          </If>
        </If>
        <If cond={status === 'preinstall'}>
          <PreinstallSetup />
        </If>
        <If cond={status !== 'error' && status !== 'preinstall'}>
          <Setup />
        </If>
      </div>
    </div>
  )
}
