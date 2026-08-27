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
    pending,
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
    // 切换当前项目会销毁编辑器 Working Copy，只在确有修改时追加保护。
    // eslint-disable-next-line no-alert
    if (projectId !== activeProject?.id && hasDirtyStudioEditor() && !window.confirm(t('studio.project.editor_leave_warning')))
      return
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.switch_confirm')))
      return
    await runProjectAction(() => activateProject(projectId), 'studio.project.switched')
  }

  async function handleRemove(projectId: string) {
    // 删除当前项目会销毁编辑器 Working Copy，删除其他项目不额外拦截。
    // eslint-disable-next-line no-alert
    if (projectId === activeProject?.id && hasDirtyStudioEditor() && !window.confirm(t('studio.project.editor_leave_warning')))
      return
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.remove_confirm')))
      return
    await runProjectAction(() => removeProject(projectId), 'studio.project.removed')
  }

  async function handleRelink(projectId: string) {
    // 重绑定会销毁当前项目的编辑器 Working Copy，只在确有修改时额外确认。
    // eslint-disable-next-line no-alert
    if (projectId === activeProject?.id && hasDirtyStudioEditor() && !window.confirm(t('studio.project.editor_leave_warning')))
      return
    await runProjectAction(() => relinkProject(projectId), 'studio.project.relinked')
  }

  async function handleWorkspace(projectId: string) {
    await runProjectAction(() => selectWorkspace(projectId), 'studio.project.workspace_selected')
  }

  async function handleScan(projectId: string) {
    await runProjectAction(() => startScan(projectId), 'studio.project.scan_started')
  }

  async function runProjectAction(action: () => Promise<unknown>, successKey: string) {
    try {
      const result = await action()
      if (result != null)
        toast(t(successKey), {})
    }
    catch (reason) {
      toast(projectActionError(reason, t), { variant: 'danger' })
    }
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
              pending={pending}
              onActivate={projectId => void handleActivate(projectId)}
              onSelectWorkspace={projectId => void handleWorkspace(projectId)}
              onScan={projectId => void handleScan(projectId)}
              onRemove={projectId => void handleRemove(projectId)}
              onRelink={projectId => void handleRelink(projectId)}
            />
          )}
    </ViewFrame>
  )
}

function hasDirtyStudioEditor(): boolean {
  return isGuiDesignerDirty()
}

function projectActionError(reason: unknown, t: (key: string) => string): string {
  const message = String(reason)
  if (message.includes('WORKSPACE_OUTSIDE_PROJECT'))
    return t('studio.project.workspace_outside')
  if (message.includes('SCAN_BUSY'))
    return t('studio.project.scan_busy')
  return message
}
