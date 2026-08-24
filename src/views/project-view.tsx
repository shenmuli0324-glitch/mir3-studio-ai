import { FolderOpen } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { isGuiDesignerDirty } from '@/features/gui-designer/gui-designer-scope'
import { ProjectDetails } from '@/features/projects/project-details'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'
import { toast } from '@/utils'
import { EmptyPanel, ViewFrame, ViewHeader } from './view-primitives'

export function ProjectView() {
  const { t } = useTranslation()
  const {
    projects,
    activeProject,
    scan,
    busy,
    error,
    importProject,
    activateProject,
    selectWorkspace,
    startScan,
    removeProject,
    relinkProject,
  } = useMir3Projects()

  async function handleImport() {
    try {
      const project = await importProject()
      if (project)
        toast(t('studio.project.imported', { name: project.name }), {})
    }
    catch (reason) {
      toast(String(reason), { variant: 'danger' })
    }
  }

  async function handleActivate(projectId: string) {
    // 切换当前项目会销毁 GUI Working Copy，只在确有修改时追加保护。
    // eslint-disable-next-line no-alert
    if (projectId !== activeProject?.id && isGuiDesignerDirty() && !window.confirm(t('studio.gui.leave_warning')))
      return
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.switch_confirm')))
      return
    await activateProject(projectId)
  }

  async function handleRemove(projectId: string) {
    // 删除当前项目会销毁 GUI Working Copy，删除其他项目不额外拦截。
    // eslint-disable-next-line no-alert
    if (projectId === activeProject?.id && isGuiDesignerDirty() && !window.confirm(t('studio.gui.leave_warning')))
      return
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.remove_confirm')))
      return
    await removeProject(projectId)
  }

  async function handleRelink(projectId: string) {
    // 重绑定会销毁当前项目的 GUI Working Copy，只在确有修改时额外确认。
    // eslint-disable-next-line no-alert
    if (projectId === activeProject?.id && isGuiDesignerDirty() && !window.confirm(t('studio.gui.leave_warning')))
      return
    await relinkProject(projectId)
  }

  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.project.eyebrow')}
        title={t('studio.project.title')}
        description={t('studio.project.description')}
        action={(
          <Button className="rounded-lg" variant="primary" isPending={busy} onPress={() => void handleImport()}>
            <FolderOpen className="size-4" />
            {t('studio.project.open_996')}
          </Button>
        )}
      />
      {error ? <p className="rounded-xl border border-danger/30 bg-danger/8 px-4 py-3 text-sm text-danger">{String(error)}</p> : null}
      {projects.length === 0
        ? (
            <EmptyPanel
              icon={<FolderOpen className="size-5" />}
              title={t('studio.project.recent_empty')}
              description={t('studio.project.recent_empty_desc')}
            />
          )
        : (
            <ProjectDetails
              projects={projects}
              activeProject={activeProject}
              scan={scan}
              busy={busy}
              onActivate={projectId => void handleActivate(projectId)}
              onSelectWorkspace={projectId => void selectWorkspace(projectId)}
              onScan={projectId => void startScan(projectId)}
              onRemove={projectId => void handleRemove(projectId)}
              onRelink={projectId => void handleRelink(projectId)}
            />
          )}
    </ViewFrame>
  )
}
