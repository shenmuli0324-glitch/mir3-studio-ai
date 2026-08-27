import { FolderCode, TriangleExclamation } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { DesignerDialogs } from '@/features/gui-designer/designer-dialogs'
import { DesignerSidebar } from '@/features/gui-designer/designer-sidebar'
import { DesignerToolbar } from '@/features/gui-designer/designer-toolbar'
import { DesignerWorkspace } from '@/features/gui-designer/designer-workspace'
import { GuiAiPanel } from '@/features/gui-designer/gui-ai-panel'
import { GuiDesignerScope } from '@/features/gui-designer/gui-designer-scope'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'
import { useScope } from '@/hooks/use-scope'

export function GuiDesignerView() {
  const { activeProject } = useMir3Projects()
  const { t } = useTranslation()
  if (!activeProject)
    return <DesignerEmpty icon={<FolderCode />} title={t('studio.gui.no_project')} description={t('studio.gui.no_project_desc')} />
  return <GuiDesignerContent />
}

function GuiDesignerContent() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  if (scope.statusLoading)
    return <DesignerEmpty title={t('studio.gui.loading')} description={t('studio.gui.loading_desc')} />
  if (scope.statusError || scope.status?.available === false)
    return <DesignerEmpty icon={<TriangleExclamation />} title={t('studio.gui.unavailable')} description={scope.status?.reason ?? String(scope.statusError ?? t('studio.gui.unavailable_desc'))} />
  return (
    <div className="relative flex h-full min-h-0 flex-col overflow-hidden bg-canvas">
      <DesignerToolbar />
      <If cond={scope.notice != null}>
        <div className="flex h-8 shrink-0 items-center bg-accent/8 px-3 text-[10px] text-accent">
          <span>{t(scope.notice ?? 'studio.gui.notice.no_peer')}</span>
          <button className="ml-auto px-2 text-muted hover:text-ink" type="button" onClick={() => scope.setNotice(null)}>×</button>
        </div>
      </If>
      <div className="flex min-h-0 min-w-0 flex-1">
        <DesignerSidebar />
        <DesignerWorkspace />
        <GuiAiPanel />
      </div>
      <DesignerDialogs />
    </div>
  )
}

function DesignerEmpty({ icon, title, description }: { icon?: React.ReactNode, title: string, description: string }) {
  return (
    <div className="grid h-full place-items-center bg-canvas p-8">
      <div className="flex max-w-md flex-col items-center text-center">
        <If cond={icon != null}><span className="mb-5 grid size-14 place-items-center rounded-2xl bg-panel text-accent ring-1 ring-line">{icon}</span></If>
        <strong className="text-lg font-semibold text-ink">{title}</strong>
        <p className="mt-2 text-sm leading-6 text-muted">{description}</p>
      </div>
    </div>
  )
}
