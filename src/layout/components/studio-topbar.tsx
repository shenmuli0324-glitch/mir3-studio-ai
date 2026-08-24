import type { RefObject } from 'react'
import type { StudioView } from '../studio-types'
import type { Mir3Project } from '@/features/projects/types'
import {
  ArrowLeft,
  ArrowRight,
  Bars,
  LayoutSideContent,
  LayoutSideContentLeft,
  Minus,
  Square,
  Xmark,
} from '@gravity-ui/icons'
import { Button, Chip, Description, Dropdown, Label } from '@heroui/react'
import { useOverlay } from '@overlastic/react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useStore } from 'valtio-define'
import { ConfigDialog } from '@/components/config-dialog'
import { DesktopAboutDialog } from '@/components/desktop-about-dialog'
import { DesktopUpdateDialog } from '@/components/desktop-update-dialog'
import { useDshPlugins } from '@/hooks/use-dsh-plugins'
import { useIframeTauri } from '@/hooks/use-iframe-tauri'
import { store } from '@/store'
import { toast } from '@/utils'

const TAURI_PLUGIN_ID = 'dsh-tauri'

export function StudioTopbar({
  activeView,
  sidebarCollapsed,
  iframeRef,
  showSidebarToggle,
  onToggleSidebar,
  project,
  onSelectWorkspace,
}: {
  activeView: StudioView
  sidebarCollapsed: boolean
  iframeRef?: RefObject<HTMLIFrameElement | null>
  showSidebarToggle: boolean
  onToggleSidebar: () => void
  project: Mir3Project | null
  onSelectWorkspace: () => void
}) {
  const { t } = useTranslation()
  const { plugins } = useDshPlugins()
  const { updateInfo } = useStore(store.desktopUpdater)
  const { sidebarCollapsed: harnessSidebarCollapsed, canGoBack, canGoForward, sendNav } = useIframeTauri(iframeRef)
  const openConfigDialog = useOverlay(ConfigDialog)
  const openAboutDialog = useOverlay(DesktopAboutDialog)
  const openUpdateDialog = useOverlay(DesktopUpdateDialog)
  const workbenchActive = activeView === 'workbench'
  const tauriEnabled = plugins.some(plugin => plugin.id === TAURI_PLUGIN_ID)
  const harnessControlsVisible = workbenchActive && tauriEnabled && iframeRef != null

  function handleWindowAction(action: 'minimize' | 'maximize' | 'background') {
    const appWindow = getCurrentWindow()
    switch (action) {
      case 'minimize':
        void appWindow.minimize()
        break
      case 'maximize':
        void appWindow.toggleMaximize()
        break
      case 'background':
        void appWindow.hide()
        break
    }
  }

  function handleHelpAction(key: string) {
    if (key === 'check-update')
      void handleCheckUpdate()
    else if (key === 'about')
      void openAboutDialog().catch(() => {})
    else if (key === 'copy-run-logs')
      void copyRunLogs()
  }

  async function handleCheckUpdate() {
    try {
      const info = await store.desktopUpdater.check()
      if (info)
        void openUpdateDialog().catch(() => {})
      else
        toast(t('update.up_to_date'), {})
    }
    catch (error) {
      console.warn('[StudioTopbar] check update failed:', error)
      toast(t('update.check_failed'), { variant: 'danger' })
    }
  }

  async function copyRunLogs() {
    try {
      const logs = await invoke<string>('read_run_logs')
      await navigator.clipboard.writeText(logs)
      toast(t('messages.logs_copied'), {})
    }
    catch (error) {
      console.error('[StudioTopbar] failed to copy run logs:', error)
      toast(t('messages.copy_failed'), { variant: 'danger' })
    }
  }

  return (
    <header className="flex h-11 w-full shrink-0 select-none items-center gap-1 border-b border-line bg-panel px-1.5">
      <If cond={showSidebarToggle}>
        <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t(sidebarCollapsed ? 'nav.sidebar_expand' : 'nav.sidebar_collapse')} onPress={onToggleSidebar}>
          <Bars />
        </Button>
      </If>
      <If cond={harnessControlsVisible}>
        <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t(harnessSidebarCollapsed ? 'studio.workbench.expand_core_sidebar' : 'studio.workbench.collapse_core_sidebar')} onPress={() => sendNav('sidebar:toggle')}>
          <If cond={harnessSidebarCollapsed} then={<LayoutSideContentLeft />} else={<LayoutSideContent />} />
        </Button>
        <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t('nav.back')} isDisabled={!canGoBack} onPress={() => sendNav('page:prev')}>
          <ArrowLeft />
        </Button>
        <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t('nav.forward')} isDisabled={!canGoForward} onPress={() => sendNav('page:next')}>
          <ArrowRight />
        </Button>
      </If>
      <div className="ml-1 flex min-w-0 items-center gap-2">
        <strong className="max-w-44 truncate text-xs font-medium text-ink">{t(`studio.nav.${activeView}`)}</strong>
        <span className="hidden max-w-56 truncate text-[11px] text-muted min-[760px]:inline">
          {project?.name ?? t('studio.shell.no_project')}
        </span>
      </div>
      <div className="min-w-0 flex-1 self-stretch" data-tauri-drag-region onDoubleClick={() => { void getCurrentWindow().toggleMaximize() }} />
      <If cond={project != null}>
        <Button className="hidden h-7 max-w-56 rounded-lg px-2 text-xs min-[920px]:flex" size="sm" variant="ghost" onPress={onSelectWorkspace}>
          <span className="truncate">{project?.activeWorkspaceRoot}</span>
        </Button>
      </If>
      <Button className="h-7 rounded-lg px-2 text-xs" size="sm" variant="ghost" onPress={() => void openConfigDialog().catch(() => {})}>
        {t('app.config')}
      </Button>
      <Dropdown>
        <Button className="h-7 rounded-lg px-2 text-xs" size="sm" variant="ghost" aria-label={t('app.help')}>
          {t('app.help')}
        </Button>
        <Dropdown.Popover className="min-w-44 rounded-md">
          <Dropdown.Menu>
            <Dropdown.Item id="copy-run-logs" textValue={t('menu.run_logs')} onAction={() => handleHelpAction('copy-run-logs')}>
              <Label>{t('menu.run_logs')}</Label>
            </Dropdown.Item>
            <Dropdown.Item id="check-update" textValue={t('menu.check_update')} onAction={() => handleHelpAction('check-update')}>
              <span className="flex w-full items-center justify-between gap-3">
                <Label>{t('menu.check_update')}</Label>
                <If cond={updateInfo != null}>
                  <Description>{t('menu.new_version')}</Description>
                </If>
              </span>
            </Dropdown.Item>
            <Dropdown.Item id="about" textValue={t('menu.about')} onAction={() => handleHelpAction('about')}>
              <Label>{t('menu.about')}</Label>
            </Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
      <If cond={import.meta.env.DEV}>
        <Chip size="sm" variant="primary" color="warning" className="ml-1 text-xs text-background">{t('app.dev_env')}</Chip>
      </If>
      <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t('nav.minimize')} onPress={() => handleWindowAction('minimize')}><Minus /></Button>
      <Button className="size-7 rounded-lg" isIconOnly size="sm" variant="ghost" aria-label={t('nav.maximize')} onPress={() => handleWindowAction('maximize')}><Square className="size-3.5" /></Button>
      <Button className="size-7 rounded-lg transition-colors enabled:hover:bg-danger/16 enabled:hover:text-danger" isIconOnly size="sm" variant="ghost" aria-label={t('nav.background')} onPress={() => handleWindowAction('background')}><Xmark /></Button>
    </header>
  )
}
