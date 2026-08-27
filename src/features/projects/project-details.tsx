import type { ReactNode } from 'react'
import type { Mir3Project, ScanState } from './types'
import { ClockArrowRotateLeft, FolderOpen, Play, ShieldCheck } from '@gravity-ui/icons'
import { Button, Chip } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { SectionPanel } from '@/views/view-primitives'
import { useProjectDetails } from './use-project-details'

export function ProjectDetails({ projects, activeProject, scan, pending, onActivate, onSelectWorkspace, onScan, onRemove, onRelink }: {
  projects: Mir3Project[]
  activeProject: Mir3Project | null
  scan: ScanState | null
  pending: { activate: boolean, workspace: boolean, scan: boolean, remove: boolean, relink: boolean }
  onActivate: (projectId: string) => void
  onSelectWorkspace: (projectId: string) => void
  onScan: (projectId: string) => void
  onRemove: (projectId: string) => void
  onRelink: (projectId: string) => void
}) {
  const { t } = useTranslation()
  return (
    <>
      <SectionPanel title={t('studio.project.list_title')} description={t('studio.project.list_desc')}>
        <div className="divide-y divide-line">
          {projects.map(project => (
            <article className="flex items-start gap-4 px-5 py-4" key={project.id}>
              <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-line bg-panel2 text-accent"><FolderOpen className="size-5" /></span>
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <strong className="text-sm text-ink">{project.name}</strong>
                  <Chip size="sm" color={project.status === 'valid' ? 'success' : 'warning'}>{t(`studio.project.status.${project.status}`)}</Chip>
                  <If cond={activeProject?.id === project.id}><Chip size="sm" color="accent">{t('studio.project.active')}</Chip></If>
                </div>
                <p className="mt-1 truncate text-xs text-muted">{project.root}</p>
                <p className="mt-1 text-xs text-muted">{t('studio.project.engine_version', { version: project.engineVersion ?? t('studio.project.unknown') })}</p>
                <If cond={project.warnings.length > 0}>
                  <p className="mt-2 text-xs leading-5 text-warning">{project.warnings.join(' · ')}</p>
                </If>
              </div>
              <div className="flex shrink-0 gap-2">
                <Button size="sm" variant="ghost" isPending={pending.workspace} onPress={() => onSelectWorkspace(project.id)}>{t('studio.project.workspace')}</Button>
                <Button size="sm" variant="ghost" isPending={pending.scan && scan?.projectId === project.id} onPress={() => onScan(project.id)}>{t('studio.project.scan')}</Button>
                <If cond={project.status === 'missing'}>
                  <Button size="sm" variant="ghost" isPending={pending.relink} onPress={() => onRelink(project.id)}>{t('studio.project.relink')}</Button>
                </If>
                <Button size="sm" variant="ghost" className="text-danger" isPending={pending.remove} onPress={() => onRemove(project.id)}>{t('studio.project.remove')}</Button>
                <Button size="sm" variant="primary" isPending={pending.activate} isDisabled={activeProject?.id === project.id} onPress={() => onActivate(project.id)}>
                  <If cond={activeProject?.id === project.id} then={t('studio.project.active')} else={t('studio.project.activate')} />
                </Button>
              </div>
            </article>
          ))}
        </div>
      </SectionPanel>
      <If cond={activeProject != null}>
        <ActiveProjectPanels project={activeProject!} scan={scan} />
      </If>
    </>
  )
}

function ActiveProjectPanels({ project, scan }: { project: Mir3Project, scan: ScanState | null }) {
  const { t } = useTranslation()
  const details = useProjectDetails(project.id)
  const [confirmation, setConfirmation] = useState<Awaited<ReturnType<typeof details.previewDraft>> | null>(null)

  async function handlePreview(draftId: string) {
    setConfirmation(await details.previewDraft(draftId))
  }

  async function handleApply() {
    if (!confirmation)
      return
    await details.applyDraft({
      draftId: confirmation.preview.draft.id,
      confirmationToken: confirmation.confirmationToken,
    })
    setConfirmation(null)
  }

  async function handleRestore(snapshotId: string) {
    // 原生确认框用于高风险恢复操作，避免未经确认覆盖项目文件。
    // eslint-disable-next-line no-alert
    if (window.confirm(t('studio.project.restore_confirm')))
      await details.restoreSnapshot(snapshotId)
  }

  return (
    <div className="grid grid-cols-2 gap-5 max-[980px]:grid-cols-1">
      <SectionPanel title={t('studio.project.index_title')} description={t('studio.project.index_desc')}>
        <div className="grid grid-cols-2 gap-px bg-line">
          <Metric label={t('studio.project.index_files')} value={details.stats?.totalFiles ?? 0} />
          <Metric label={t('studio.project.index_text')} value={details.stats?.indexedTextFiles ?? 0} />
        </div>
        <div className="flex flex-wrap gap-2 p-4">
          {Object.entries(details.stats?.categories ?? {}).map(([name, count]) => (
            <Chip size="sm" key={name}>
              {name}
              {' '}
              ·
              {' '}
              {count}
            </Chip>
          ))}
          <If cond={scan?.projectId === project.id && scan.phase === 'running'}>
            <Chip size="sm" color="accent">{t('studio.project.scanning')}</Chip>
          </If>
        </div>
      </SectionPanel>
      <SectionPanel title={t('studio.project.workspace_title')} description={t('studio.project.workspace_desc')}>
        <div className="space-y-3 p-5 text-xs text-muted">
          <PathRow label={t('studio.project.root')} value={project.root} />
          <PathRow label={t('studio.project.client')} value={project.clientRoot} />
          <PathRow label={t('studio.project.engine')} value={project.engineRoot} />
          <PathRow label={t('studio.project.current_workspace')} value={project.activeWorkspaceRoot} />
        </div>
      </SectionPanel>
      <SectionPanel title={t('studio.project.drafts_title')} description={t('studio.project.drafts_desc')}>
        <If cond={details.drafts.length > 0} else={<PanelEmpty icon={<ShieldCheck className="size-4" />} text={t('studio.project.drafts_empty')} />}>
          <div className="divide-y divide-line">
            {details.drafts.map(draft => (
              <div className="flex items-center gap-3 px-5 py-3" key={draft.id}>
                <div className="min-w-0 flex-1">
                  <strong className="block truncate text-xs text-ink">{draft.intent}</strong>
                  <small className="text-[11px] text-muted">
                    {draft.id}
                    {' '}
                    · r
                    {draft.revision}
                  </small>
                </div>
                <Chip size="sm">{draft.status}</Chip>
                <Button size="sm" variant="ghost" isDisabled={draft.status !== 'open'} onPress={() => void handlePreview(draft.id)}>{t('studio.project.preview')}</Button>
              </div>
            ))}
          </div>
        </If>
      </SectionPanel>
      <SectionPanel title={t('studio.project.snapshots_title')} description={t('studio.project.snapshots_desc')}>
        <If cond={details.snapshots.length > 0} else={<PanelEmpty icon={<ClockArrowRotateLeft className="size-4" />} text={t('studio.project.snapshots_empty')} />}>
          <div className="divide-y divide-line">
            {details.snapshots.map(snapshot => (
              <div className="flex items-center gap-3 px-5 py-3" key={snapshot.id}>
                <div className="min-w-0 flex-1">
                  <strong className="block truncate text-xs text-ink">{snapshot.id}</strong>
                  <small className="text-[11px] text-muted">{t('studio.project.snapshot_files', { count: snapshot.files.length })}</small>
                </div>
                <Button size="sm" variant="ghost" onPress={() => void handleRestore(snapshot.id)}>{t('studio.project.restore')}</Button>
              </div>
            ))}
          </div>
        </If>
      </SectionPanel>
      <If cond={confirmation != null}>
        <section className="col-span-2 overflow-hidden rounded-2xl border border-accent/30 bg-panel max-[980px]:col-span-1">
          <header className="flex items-center justify-between border-b border-line px-5 py-4">
            <div>
              <strong className="text-sm text-ink">{t('studio.project.preview_title')}</strong>
              <p className="mt-1 text-xs text-muted">{t('studio.project.preview_desc')}</p>
            </div>
            <Button size="sm" variant="primary" isPending={details.busy} onPress={() => void handleApply()}>
              <Play className="size-4" />
              {t('studio.project.apply')}
            </Button>
          </header>
          <div className="max-h-96 overflow-auto p-4">
            {confirmation?.preview.changes.map(change => (
              <div className="mb-4" key={change.path}>
                <strong className="mb-2 block text-xs text-ink">{change.path}</strong>
                <pre className="overflow-auto rounded-xl bg-canvas p-4 text-[11px] leading-5 text-muted">{change.unifiedDiff ?? t('studio.project.binary_change')}</pre>
              </div>
            ))}
          </div>
        </section>
      </If>
    </div>
  )
}

function Metric({ label, value }: { label: string, value: number }) {
  return (
    <div className="bg-panel p-5">
      <strong className="block text-xl text-ink">{value}</strong>
      <span className="mt-1 block text-xs text-muted">{label}</span>
    </div>
  )
}

function PathRow({ label, value }: { label: string, value: string }) {
  return (
    <div>
      <strong className="mb-1 block font-medium text-ink">{label}</strong>
      <span className="block break-all">{value}</span>
    </div>
  )
}

function PanelEmpty({ icon, text }: { icon: ReactNode, text: string }) {
  return (
    <div className="flex min-h-28 items-center justify-center gap-2 p-5 text-xs text-muted">
      {icon}
      {text}
    </div>
  )
}
