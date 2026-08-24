import { Wrench } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { StudioSidebar } from '@/layout/components/studio-sidebar'
import { DevToolsView } from '@/views/devtools-view'

export function DevToolsPreview() {
  const { t } = useTranslation()
  return (
    <div className="flex h-screen w-screen flex-col bg-canvas">
      <header className="flex h-11 shrink-0 items-center gap-2 border-b border-line bg-panel px-3">
        <Wrench className="size-4 text-accent" />
        <strong className="text-xs font-medium text-ink">{t('studio.nav.devtools')}</strong>
        <span className="text-[11px] text-muted">{t('studio.shell.no_project')}</span>
      </header>
      <div className="flex min-h-0 flex-1">
        <StudioSidebar activeView="devtools" collapsed={false} onNavigate={() => {}} />
        <main className="min-h-0 min-w-0 flex-1 overflow-hidden"><DevToolsView preview /></main>
      </div>
    </div>
  )
}
