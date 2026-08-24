import { FolderOpen } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
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
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.switch_confirm')))
      return
    await activateProject(projectId)
  }

  async function handleRemove(projectId: string) {
    // eslint-disable-next-line no-alert
    if (!window.confirm(t('studio.project.remove_confirm')))
      return
    await removeProject(projectId)
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
              onRelink={projectId => void relinkProject(projectId)}
            />
          )}
    </ViewFrame>
  )
}
