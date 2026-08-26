import type { DevToolDefinition } from '../devtool-registry'
import type { DomainDraftConfirmation, DomainFileRecord, DomainManifest, DomainPackRemoteCandidate, DomainPackState, DomainResourceRecord, DomainValidationReport, SafeTextOpen } from './types'
import type { Mir3Project } from '@/features/projects/types'
import type { DomainDraftHandoff, VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { CircleCheck, CircleExclamation, Magnifier } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { SystemAiPanel } from '@/features/system-ai/system-ai-panel'
import { toast } from '@/utils'
import { DevToolWorkspace } from '../shell/devtool-workspace'
import {
  activateDomainPack,
  applyDomainDraft,
  checkDomainPackUpdates,
  describeDomainSystem,
  getDomainPackState,
  getDomainResource,
  listDomainDrafts,
  listDomainSystems,
  openDomainDraft,
  openDomainText,
  patchDomainText,
  previewDomainDraft,
  queryDomainFiles,
  queryDomainResources,
  queryUnclaimedDomainFiles,
  rollbackDomainPack,
  setDomainPackEnabled,
  stageDomainPackUpdate,
  validateDomainDraft,
  validateDomainSystem,
} from './api'
import { ResourceRenderer } from './renderers/resource-renderer'

type ResourceTab = 'resources' | 'files' | 'dependencies'
type CenterTab = 'domain' | 'source' | 'diff' | 'validation'

export function DomainSystemView({ tool, project, onBack, target }: {
  tool: DevToolDefinition
  project: Mir3Project | null
  onBack: () => void
  target?: VerifiedDevtoolsTarget | null
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [resourceTab, setResourceTab] = useState<ResourceTab>('resources')
  const [centerTab, setCenterTab] = useState<CenterTab>('domain')
  const [search, setSearch] = useState('')
  const [selectedFile, setSelectedFile] = useState<DomainFileRecord | null>(null)
  const [selectedResourceId, setSelectedResourceId] = useState<string | null>(null)
  const [editedContent, setEditedContent] = useState<string | null>(null)
  const [draftPreview, setDraftPreview] = useState<DomainDraftConfirmation | null>(null)
  const [validation, setValidation] = useState<DomainValidationReport | null>(null)
  const handledTargetRef = useRef('')

  const manifests = useQuery({
    queryKey: ['domain-systems'],
    queryFn: listDomainSystems,
    enabled: project != null,
  })
  const manifest = manifests.data?.find(item => item.systemId === tool.id) ?? fallbackManifest(tool)
  const description = useQuery({
    queryKey: ['domain-system-description', project?.id, tool.id],
    queryFn: () => describeDomainSystem(project!.id, tool.id),
    enabled: project != null,
  })
  const packState = useQuery({
    queryKey: ['domain-pack-state', tool.id],
    queryFn: () => getDomainPackState(tool.id),
    enabled: project != null,
  })
  const activatePack = useMutation({
    mutationFn: () => activateDomainPack(tool.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['domain-pack-state', tool.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-systems'] })
    },
  })
  const rollbackPack = useMutation({
    mutationFn: () => rollbackDomainPack(tool.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['domain-pack-state', tool.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-systems'] })
    },
  })
  const togglePack = useMutation({
    mutationFn: (enabled: boolean) => setDomainPackEnabled(tool.id, enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['domain-pack-state', tool.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-systems'] })
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const checkPackUpdates = useMutation({
    mutationFn: () => checkDomainPackUpdates(tool.id),
    onSuccess: (result) => {
      if (result.updates.length === 0)
        toast(t('studio.devtools.pack.no_update'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const remoteCandidate = checkPackUpdates.data?.updates[0]
  const stagePackUpdate = useMutation({
    mutationFn: (version: string) => stageDomainPackUpdate(tool.id, version),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['domain-pack-state', tool.id] })
      toast(t('studio.devtools.pack.staged'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const files = useQuery({
    queryKey: ['domain-files', project?.id, tool.id, search],
    queryFn: () => queryDomainFiles(project!.id, tool.id, search),
    enabled: project != null,
  })
  const unclaimedFiles = useQuery({
    queryKey: ['domain-unclaimed-files', project?.id, search],
    queryFn: () => queryUnclaimedDomainFiles(project!.id, search),
    enabled: project != null && resourceTab === 'files',
  })
  const resources = useQuery({
    queryKey: ['domain-resources', project?.id, tool.id, search],
    queryFn: () => queryDomainResources(project!.id, tool.id, search, 10_000),
    enabled: project != null && resourceTab === 'resources',
  })
  const selectedResourceKey = selectedResourceId ?? selectedFile?.resourceId
  const selectedResource = useQuery({
    queryKey: ['domain-resource', project?.id, tool.id, selectedResourceKey],
    queryFn: () => getDomainResource(project!.id, tool.id, selectedResourceKey!),
    enabled: project != null && selectedResourceKey != null,
  })
  const drafts = useQuery({
    queryKey: ['domain-drafts', project?.id],
    queryFn: () => listDomainDrafts(project!.id),
    enabled: project != null,
  })
  const openedFile = useQuery({
    queryKey: ['domain-source', project?.id, selectedFile?.path],
    queryFn: () => openDomainText(project!.id, selectedFile!.path, null),
    enabled: project != null && isTextFile(selectedFile),
  })
  const patch = useMutation({
    mutationFn: async ({ opened, content }: { opened: SafeTextOpen, content: string }) => {
      const draft = await openDomainDraft(
        project!.id,
        manifest.systemId,
        manifest.version,
        t('studio.devtools.source.draft_intent', { path: opened.relativePath }),
      )
      const scoped = await openDomainText(project!.id, opened.relativePath, draft.id)
      if (scoped.sha256 !== opened.sha256)
        throw new Error('SAFE_FILE_SOURCE_CONFLICT')
      return patchDomainText(project!.id, scoped, content)
    },
    onSuccess: async (result) => {
      setDraftPreview(await previewDomainDraft(project!.id, result.draftId))
      setCenterTab('diff')
      void queryClient.invalidateQueries({ queryKey: ['domain-drafts', project?.id] })
      toast(t('studio.devtools.source.staged'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const applyDraft = useMutation({
    mutationFn: async (confirmation: DomainDraftConfirmation) => {
      const report = await validateDomainDraft(project!.id, confirmation.preview.draft.id)
      setValidation(report)
      if (!report.valid)
        throw new Error('DOMAIN_VALIDATION_FAILED')
      return applyDomainDraft(project!.id, confirmation.preview.draft.id, confirmation.confirmationToken)
    },
    onSuccess: () => {
      setDraftPreview(null)
      setEditedContent(null)
      void queryClient.invalidateQueries({ queryKey: ['domain-drafts', project?.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-files', project?.id, tool.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-resources', project?.id, tool.id] })
      void queryClient.invalidateQueries({ queryKey: ['domain-source', project?.id] })
      toast(t('studio.devtools.diff.applied'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })

  useEffect(() => {
    if (!project || !target || files.isLoading || target.projectId !== project.id || target.systemId !== tool.id || handledTargetRef.current === target.nonce)
      return
    handledTargetRef.current = target.nonce
    void consumeNavigationTarget(project, target, files.data ?? [], selectFile, setDraftPreview, setValidation, setCenterTab)
      .catch(reason => toast(String(reason), { variant: 'danger' }))
  }, [files.data, files.isLoading, project, target, tool.id])

  function selectFile(file: DomainFileRecord) {
    setSelectedFile(file)
    setSelectedResourceId(null)
    setEditedContent(null)
    setCenterTab('domain')
  }

  function selectResource(resource: DomainResourceRecord) {
    setSelectedResourceId(resource.id)
    setSelectedFile(resource.files[0] ?? null)
    setEditedContent(null)
    setCenterTab('domain')
  }

  async function runValidation() {
    if (!project)
      return
    try {
      setValidation(await validateDomainSystem(project.id, tool.id))
      setCenterTab('validation')
    }
    catch (reason) {
      toast(String(reason), { variant: 'danger' })
    }
  }

  async function showDraft(draftId: string) {
    if (!project)
      return
    try {
      setDraftPreview(await previewDomainDraft(project.id, draftId))
      setCenterTab('diff')
    }
    catch (reason) {
      toast(String(reason), { variant: 'danger' })
    }
  }

  async function handleAiDraftHandoff(handoff: DomainDraftHandoff) {
    if (!project || handoff.systemId !== tool.id)
      throw new Error('AI_DRAFT_SCOPE_MISMATCH')
    const [preview, report] = await Promise.all([
      previewDomainDraft(project.id, handoff.draftId),
      validateDomainDraft(project.id, handoff.draftId),
    ])
    if (report.systemId !== tool.id || preview.preview.draft.revision < handoff.revision)
      throw new Error('AI_DRAFT_BINDING_MISMATCH')
    setDraftPreview(preview)
    setValidation(report)
    if (handoff.resourceId) {
      const resource = await getDomainResource(project.id, tool.id, handoff.resourceId)
      selectResource(resource)
    }
    setCenterTab(report.valid ? 'diff' : 'validation')
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['domain-drafts', project.id] }),
      queryClient.invalidateQueries({ queryKey: ['domain-files', project.id, tool.id] }),
      queryClient.invalidateQueries({ queryKey: ['domain-resources', project.id, tool.id] }),
      queryClient.invalidateQueries({ queryKey: ['domain-resource', project.id, tool.id] }),
      queryClient.invalidateQueries({ queryKey: ['domain-source', project.id] }),
    ])
  }

  function saveSource() {
    if (!canEditSource(selectedFile) || !openedFile.data || editedContent == null || editedContent === openedFile.data.content)
      return
    patch.mutate({ opened: openedFile.data, content: editedContent })
  }

  return (
    <DevToolWorkspace
      tool={tool}
      onBack={onBack}
      sidebar={(
        <ResourceSidebar
          activeTab={resourceTab}
          onTab={setResourceTab}
          search={search}
          onSearch={setSearch}
          files={files.data ?? []}
          resources={resources.data ?? []}
          unclaimedFiles={unclaimedFiles.data ?? []}
          manifest={manifest}
          selectedPath={selectedFile?.path}
          selectedResourceId={selectedResourceId}
          onSelectFile={selectFile}
          onSelectResource={selectResource}
          loading={files.isLoading || unclaimedFiles.isLoading || resources.isLoading}
        />
      )}
      toolbar={(
        <WorkspaceToolbar
          manifest={manifest}
          project={project}
          activeTab={centerTab}
          onTab={setCenterTab}
          onValidate={() => void runValidation()}
          packState={packState.data}
          remoteCandidate={remoteCandidate}
          busy={activatePack.isPending || rollbackPack.isPending || togglePack.isPending || checkPackUpdates.isPending || stagePackUpdate.isPending}
          onCheckUpdate={() => checkPackUpdates.mutate()}
          onStageUpdate={() => {
            if (remoteCandidate)
              stagePackUpdate.mutate(remoteCandidate.version)
          }}
          onActivate={() => {
            // eslint-disable-next-line no-alert
            if (window.confirm(t('studio.devtools.pack.activate_confirm', { version: packState.data?.candidate?.version })))
              activatePack.mutate()
          }}
          onRollback={() => {
            // eslint-disable-next-line no-alert
            if (window.confirm(t('studio.devtools.pack.rollback_confirm')))
              rollbackPack.mutate()
          }}
          onToggleEnabled={() => {
            const enabled = packState.data?.enabled !== false
            // eslint-disable-next-line no-alert
            if (window.confirm(t(packToggleConfirmationKey(enabled))))
              togglePack.mutate(!enabled)
          }}
        />
      )}
      rightPanel={renderSystemAiPanel(project, manifest, selectedFile?.path, selectedResourceKey, draftPreview?.preview.draft.id, handleAiDraftHandoff)}
    >
      <If cond={project != null} else={<NoProject />}>
        <CenterWorkspace
          activeTab={centerTab}
          manifest={manifest}
          files={files.data ?? []}
          description={description.data}
          resource={selectedResource.data}
          resourceLoading={selectedResource.isLoading}
          resourceError={selectedResource.error}
          selectedFile={selectedFile}
          openedFile={openedFile.data}
          sourceLoading={openedFile.isLoading}
          sourceError={openedFile.error}
          editedContent={editedContent}
          onEditedContent={setEditedContent}
          onSaveSource={saveSource}
          saving={patch.isPending}
          drafts={drafts.data ?? []}
          draftPreview={draftPreview}
          onShowDraft={draftId => void showDraft(draftId)}
          onApplyDraft={(confirmation) => {
            // eslint-disable-next-line no-alert
            if (window.confirm(t('studio.devtools.diff.apply_confirm')))
              applyDraft.mutate(confirmation)
          }}
          applying={applyDraft.isPending}
          validation={validation}
        />
      </If>
    </DevToolWorkspace>
  )
}

function ResourceSidebar({ activeTab, onTab, search, onSearch, files, resources, unclaimedFiles, manifest, selectedPath, selectedResourceId, onSelectFile, onSelectResource, loading }: {
  activeTab: ResourceTab
  onTab: (tab: ResourceTab) => void
  search: string
  onSearch: (value: string) => void
  files: DomainFileRecord[]
  resources: DomainResourceRecord[]
  unclaimedFiles: DomainFileRecord[]
  manifest: DomainManifest
  selectedPath?: string
  selectedResourceId: string | null
  onSelectFile: (file: DomainFileRecord) => void
  onSelectResource: (resource: DomainResourceRecord) => void
  loading: boolean
}) {
  const { t } = useTranslation()
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="grid grid-cols-3 gap-1 border-b border-line p-2">
        {(['resources', 'files', 'dependencies'] as const).map(tab => <SidebarTab key={tab} tab={tab} active={activeTab === tab} onPress={() => onTab(tab)} />)}
      </div>
      <If cond={activeTab !== 'dependencies'}>
        <div className="border-b border-line p-2">
          <label className="flex items-center gap-2 rounded-lg border border-line bg-panel2 px-2.5 py-2">
            <Magnifier className="size-3.5 text-muted" />
            <input className="min-w-0 flex-1 bg-transparent text-xs text-ink outline-none placeholder:text-muted" value={search} placeholder={t('studio.devtools.resources.search')} aria-label={t('studio.devtools.resources.search')} onChange={event => onSearch(event.target.value)} />
          </label>
        </div>
      </If>
      <div className="min-h-0 flex-1 overflow-auto p-2">
        <If cond={activeTab === 'dependencies'} else={renderResourceOrFileList(activeTab, resources, files, unclaimedFiles, selectedResourceId, selectedPath, onSelectResource, onSelectFile, loading)}>
          <DependencyList manifest={manifest} files={files.filter(file => file.ownership === 'dependency')} selectedPath={selectedPath} onSelect={onSelectFile} loading={loading} />
        </If>
      </div>
    </div>
  )
}

function renderResourceOrFileList(
  activeTab: ResourceTab,
  resources: DomainResourceRecord[],
  files: DomainFileRecord[],
  unclaimedFiles: DomainFileRecord[],
  selectedResourceId: string | null,
  selectedPath: string | undefined,
  onSelectResource: (resource: DomainResourceRecord) => void,
  onSelectFile: (file: DomainFileRecord) => void,
  loading: boolean,
) {
  if (activeTab === 'resources')
    return <ResourceRecordList resources={resources} selectedId={selectedResourceId} onSelect={onSelectResource} loading={loading} />
  return <FileProjectionList files={[...files, ...unclaimedFiles]} resourceMode={false} selectedPath={selectedPath} onSelect={onSelectFile} loading={loading} />
}

function ResourceRecordList({ resources, selectedId, onSelect, loading }: {
  resources: DomainResourceRecord[]
  selectedId: string | null
  onSelect: (resource: DomainResourceRecord) => void
  loading: boolean
}) {
  const { t } = useTranslation()
  if (loading)
    return <p className="p-4 text-center text-xs text-muted">{t('studio.devtools.resources.loading')}</p>
  if (resources.length === 0)
    return <p className="p-4 text-center text-xs leading-5 text-muted">{t('studio.devtools.resources.empty')}</p>
  return (
    <div className="space-y-1">
      {resources.map(resource => (
        <button key={resource.id} type="button" className={projectionButtonClass(selectedId === resource.id)} onClick={() => onSelect(resource)}>
          <strong className="block truncate text-[11px] font-medium text-ink">{resource.label}</strong>
          <span className="mt-0.5 block truncate text-[9px] text-muted">{resource.source.path}</span>
          <span className="mt-1 flex gap-1">
            <small className={accessBadgeClass(resourceAccess(resource))}>{t(resourceAccessKey(resource))}</small>
            <small className="rounded bg-accent/10 px-1 text-[8px] text-accent">{resource.resourceType}</small>
            <If cond={hasUnresolvedDependency(resource)}><small className="rounded bg-danger/10 px-1 text-[8px] text-danger">{t('studio.devtools.resources.unresolved')}</small></If>
          </span>
        </button>
      ))}
    </div>
  )
}

function hasUnresolvedDependency(resource: DomainResourceRecord) {
  return resource.dependencies.some(dependency => dependency.required && dependency.resolvedResourceId == null)
}

function resourceAccess(resource: DomainResourceRecord) {
  if (resource.writable)
    return 'editable'
  return 'readonly'
}

function resourceAccessKey(resource: DomainResourceRecord) {
  if (resource.writable)
    return 'studio.devtools.access.editable'
  return 'studio.devtools.access.readonly'
}

function SidebarTab({ tab, active, onPress }: { tab: ResourceTab, active: boolean, onPress: () => void }) {
  const { t } = useTranslation()
  return <button type="button" className={sidebarTabClass(active)} onClick={onPress}>{t(`studio.devtools.resources.${tab}`)}</button>
}

function FileProjectionList({ files, resourceMode, selectedPath, onSelect, loading }: {
  files: DomainFileRecord[]
  resourceMode: boolean
  selectedPath?: string
  onSelect: (file: DomainFileRecord) => void
  loading: boolean
}) {
  const { t } = useTranslation()
  if (loading)
    return <p className="p-4 text-center text-xs text-muted">{t('studio.devtools.resources.loading')}</p>
  if (files.length === 0)
    return <p className="p-4 text-center text-xs leading-5 text-muted">{t('studio.devtools.resources.empty')}</p>
  if (!resourceMode) {
    return (
      <div className="space-y-1">
        {groupFilesByDirectory(files).map(group => (
          <details key={group.directory} open={files.length < 200} className="rounded-lg border border-line/70 bg-panel2/40">
            <summary className="cursor-pointer truncate px-2 py-2 text-[10px] font-medium text-muted">
              {group.directory}
              {' '}
              ·
              {' '}
              {group.files.length}
            </summary>
            <div className="space-y-1 border-t border-line p-1">
              {group.files.map(file => <ProjectionButton key={file.resourceId} file={file} resourceMode={false} selected={selectedPath === file.path} onSelect={onSelect} />)}
            </div>
          </details>
        ))}
      </div>
    )
  }
  return (
    <div className="space-y-1">
      {files.map(file => <ProjectionButton key={file.resourceId} file={file} resourceMode selected={selectedPath === file.path} onSelect={onSelect} />)}
    </div>
  )
}

function ProjectionButton({ file, resourceMode, selected, onSelect }: { file: DomainFileRecord, resourceMode: boolean, selected: boolean, onSelect: (file: DomainFileRecord) => void }) {
  const { t } = useTranslation()
  return (
    <button type="button" className={projectionButtonClass(selected)} onClick={() => onSelect(file)}>
      <strong className="block truncate text-[11px] font-medium text-ink">{projectionLabel(file, resourceMode)}</strong>
      <span className="mt-0.5 block truncate text-[9px] text-muted">{file.path}</span>
      <span className="mt-1 flex gap-1">
        <small className={accessBadgeClass(file.access)}>{t(`studio.devtools.access.${file.access}`)}</small>
        <If cond={file.ownership !== 'owned'}><small className="rounded bg-accent/10 px-1 text-[8px] text-accent">{t(`studio.devtools.resources.${file.ownership}`)}</small></If>
      </span>
    </button>
  )
}

function groupFilesByDirectory(files: DomainFileRecord[]) {
  const groups = new Map<string, DomainFileRecord[]>()
  files.forEach((file) => {
    const segments = file.path.split('/')
    const directory = directoryPath(segments)
    const entries = groups.get(directory) ?? []
    entries.push(file)
    groups.set(directory, entries)
  })
  return [...groups.entries()].map(([directory, entries]) => ({ directory, files: entries }))
}

function directoryPath(segments: string[]) {
  if (segments.length > 1)
    return segments.slice(0, -1).join('/')
  return '.'
}

function DependencyList({ manifest, files, selectedPath, onSelect, loading }: {
  manifest: DomainManifest
  files: DomainFileRecord[]
  selectedPath?: string
  onSelect: (file: DomainFileRecord) => void
  loading: boolean
}) {
  const { t } = useTranslation()
  return (
    <If cond={manifest.dependencies.length > 0} else={<p className="p-4 text-center text-xs text-muted">{t('studio.devtools.dependencies.empty')}</p>}>
      <div className="space-y-2">
        {manifest.dependencies.map(dependency => (
          <div key={dependency} className="rounded-lg border border-line bg-panel2 p-3">
            <strong className="text-xs text-ink">{t(`studio.devtools.tool.${dependency}.title`)}</strong>
            <p className="mt-1 text-[10px] text-muted">{t('studio.devtools.dependencies.readonly')}</p>
          </div>
        ))}
        <FileProjectionList files={files} resourceMode={false} selectedPath={selectedPath} onSelect={onSelect} loading={loading} />
      </div>
    </If>
  )
}

function WorkspaceToolbar({ manifest, project, activeTab, onTab, onValidate, packState, remoteCandidate, busy, onCheckUpdate, onStageUpdate, onActivate, onRollback, onToggleEnabled }: {
  manifest: DomainManifest
  project: Mir3Project | null
  activeTab: CenterTab
  onTab: (tab: CenterTab) => void
  onValidate: () => void
  packState?: DomainPackState
  remoteCandidate?: DomainPackRemoteCandidate
  busy: boolean
  onCheckUpdate: () => void
  onStageUpdate: () => void
  onActivate: () => void
  onRollback: () => void
  onToggleEnabled: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex min-w-0 flex-1 items-center justify-between gap-3">
      <span className="min-w-0">
        <strong className="block truncate text-xs text-ink">{project?.name ?? t('studio.devtools.no_project')}</strong>
        <small className="block truncate text-[9px] text-muted">
          v
          {packState?.current?.version ?? manifest.version}
          {' '}
          ·
          {project?.engineVersion ?? t('studio.devtools.engine_unknown')}
          {' '}
          ·
          {manifest.renderer}
        </small>
        <small className="block truncate text-[9px] text-muted">{t('studio.devtools.package_docs')}</small>
      </span>
      <div className="flex items-center gap-1 overflow-auto">
        <Button size="sm" variant="ghost" isDisabled={busy} onPress={onCheckUpdate}>{t('studio.devtools.pack.check_update')}</Button>
        <If cond={remoteCandidate != null && packState?.candidate?.version !== remoteCandidate?.version}>
          <Button size="sm" variant="ghost" isDisabled={busy} onPress={onStageUpdate}>{t('studio.devtools.pack.stage_update', { version: remoteCandidate?.version })}</Button>
        </If>
        <If cond={packState?.candidate != null}>
          <Button size="sm" className="bg-accent text-white" isPending={busy} onPress={onActivate}>{t('studio.devtools.pack.activate', { version: packState?.candidate?.version })}</Button>
        </If>
        <If cond={packState?.previous != null || packState?.lkg != null}>
          <Button size="sm" variant="ghost" isDisabled={busy} onPress={onRollback}>{t('studio.devtools.pack.rollback')}</Button>
        </If>
        <Button size="sm" variant="ghost" isDisabled={busy} onPress={onToggleEnabled}>
          {t(packToggleLabelKey(packState?.enabled !== false))}
        </Button>
        <details className="group relative shrink-0">
          <summary className="cursor-pointer list-none rounded-lg px-2 py-1.5 text-[10px] text-muted hover:bg-panel2 hover:text-ink">{t('studio.devtools.pack.changelog')}</summary>
          <div className="absolute right-0 top-9 z-30 w-[420px] max-w-[70vw] rounded-xl border border-line bg-panel p-4 shadow-2xl">
            <strong className="text-xs text-ink">{t('studio.devtools.pack.changelog_title', { system: manifest.systemId })}</strong>
            <pre className="mt-3 max-h-80 overflow-auto whitespace-pre-wrap text-[10px] leading-5 text-muted">{packState?.changelog ?? t('studio.devtools.pack.changelog_unavailable')}</pre>
          </div>
        </details>
        {(['domain', 'source', 'diff', 'validation'] as const).map(tab => <Button key={tab} size="sm" variant="ghost" className={centerTabClass(activeTab === tab)} onPress={() => onTab(tab)}>{t(`studio.devtools.center.${tab}`)}</Button>)}
        <Button size="sm" variant="ghost" onPress={onValidate}>{t('studio.devtools.validate')}</Button>
      </div>
    </div>
  )
}

function CenterWorkspace(props: {
  activeTab: CenterTab
  manifest: DomainManifest
  files: DomainFileRecord[]
  description?: { ownedFiles: number, sharedFiles: number, writableFiles: number, readonlyFiles: number, diagnostics: string[] }
  resource?: DomainResourceRecord
  resourceLoading: boolean
  resourceError: Error | null
  selectedFile: DomainFileRecord | null
  openedFile?: SafeTextOpen
  sourceLoading: boolean
  sourceError: Error | null
  editedContent: string | null
  onEditedContent: (content: string) => void
  onSaveSource: () => void
  saving: boolean
  drafts: Array<{ id: string, intent: string, revision: number, status: string }>
  draftPreview: DomainDraftConfirmation | null
  onShowDraft: (draftId: string) => void
  onApplyDraft: (confirmation: DomainDraftConfirmation) => void
  applying: boolean
  validation: DomainValidationReport | null
}) {
  return (
    <div className="h-full min-h-0 overflow-auto p-4">
      <If cond={props.activeTab === 'domain'}><DomainRenderer manifest={props.manifest} description={props.description} resource={props.resource} resourceLoading={props.resourceLoading} resourceError={props.resourceError} /></If>
      <If cond={props.activeTab === 'source'}><SourcePanel {...props} /></If>
      <If cond={props.activeTab === 'diff'}><DiffPanel drafts={props.drafts} preview={props.draftPreview} onShow={props.onShowDraft} onApply={props.onApplyDraft} applying={props.applying} /></If>
      <If cond={props.activeTab === 'validation'}><ValidationPanel report={props.validation} /></If>
    </div>
  )
}

function DomainRenderer({ manifest, description, resource, resourceLoading, resourceError }: {
  manifest: DomainManifest
  description?: { ownedFiles: number, sharedFiles: number, writableFiles: number, readonlyFiles: number, diagnostics: string[] }
  resource?: DomainResourceRecord
  resourceLoading: boolean
  resourceError: Error | null
}) {
  const { t } = useTranslation()
  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <div className="grid grid-cols-4 gap-3 max-[1000px]:grid-cols-2">
        <Metric label={t('studio.devtools.metrics.owned')} value={description?.ownedFiles ?? 0} />
        <Metric label={t('studio.devtools.metrics.shared')} value={description?.sharedFiles ?? 0} />
        <Metric label={t('studio.devtools.metrics.writable')} value={description?.writableFiles ?? 0} />
        <Metric label={t('studio.devtools.metrics.readonly')} value={description?.readonlyFiles ?? 0} />
      </div>
      <div className="rounded-2xl border border-line bg-panel p-5">
        <span className="text-[10px] uppercase tracking-[0.16em] text-accent">{manifest.renderer}</span>
        <h2 className="mt-2 text-lg font-semibold text-ink">{t(`studio.devtools.tool.${manifest.systemId}.title`)}</h2>
        <p className="mt-1 text-sm leading-6 text-muted">{t(`studio.devtools.tool.${manifest.systemId}.description`)}</p>
        <ResourceRenderer renderer={manifest.renderer} resource={resource} loading={resourceLoading} error={resourceError} />
      </div>
      <div className="rounded-2xl border border-line bg-panel p-5">
        <strong className="text-xs text-ink">{t('studio.devtools.capabilities.title')}</strong>
        <div className="mt-3 grid grid-cols-2 gap-2 max-[900px]:grid-cols-1">
          {manifest.capabilities.map(capability => (
            <div key={capability.id} className="rounded-lg border border-line bg-panel2 px-3 py-2">
              <strong className="block text-[11px] text-ink">{capability.id}</strong>
              <small className="text-[9px] text-muted">
                v
                {capability.version}
                {' '}
                ·
                {' '}
                {t('studio.devtools.capabilities.guards')}
              </small>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function SourcePanel(props: {
  selectedFile: DomainFileRecord | null
  openedFile?: SafeTextOpen
  sourceLoading: boolean
  sourceError: Error | null
  editedContent: string | null
  onEditedContent: (content: string) => void
  onSaveSource: () => void
  saving: boolean
}) {
  const { t } = useTranslation()
  if (!props.selectedFile)
    return <CenteredNotice title={t('studio.devtools.source.empty')} description={t('studio.devtools.source.empty_desc')} />
  if (!isTextFile(props.selectedFile))
    return <CenteredNotice title={t('studio.devtools.source.readonly')} description={t('studio.devtools.source.readonly_desc', { extension: props.selectedFile.extension ?? '' })} />
  if (props.sourceLoading)
    return <CenteredNotice title={t('studio.devtools.source.loading')} description={props.selectedFile.path} />
  if (props.sourceError || !props.openedFile)
    return <CenteredNotice title={t('studio.devtools.source.failed')} description={String(props.sourceError ?? '')} />
  const content = props.editedContent ?? props.openedFile.content
  const editable = canEditSource(props.selectedFile)
  return (
    <div className="flex h-full min-h-[420px] flex-col rounded-xl border border-line bg-panel">
      <header className="flex items-center justify-between border-b border-line px-4 py-2">
        <span>
          <strong className="block text-xs text-ink">{props.selectedFile.path}</strong>
          <small className="text-[9px] text-muted">
            {props.openedFile.encoding}
            {' '}
            · SHA
            {' '}
            {props.openedFile.sha256.slice(0, 10)}
          </small>
        </span>
        <If cond={editable}>
          <Button size="sm" className="bg-accent text-white" isDisabled={content === props.openedFile.content} isPending={props.saving} onPress={props.onSaveSource}>{t('studio.devtools.source.stage')}</Button>
        </If>
      </header>
      <textarea readOnly={!editable} className="h-full min-h-[360px] flex-1 resize-none bg-canvas p-4 font-mono text-xs leading-5 text-ink outline-none" value={content} aria-label={t('studio.devtools.source.editor')} onChange={event => props.onEditedContent(event.target.value)} />
    </div>
  )
}

function DiffPanel({ drafts, preview, onShow, onApply, applying }: { drafts: Array<{ id: string, intent: string, revision: number, status: string }>, preview: DomainDraftConfirmation | null, onShow: (draftId: string) => void, onApply: (confirmation: DomainDraftConfirmation) => void, applying: boolean }) {
  const { t } = useTranslation()
  function applyPreview() {
    if (preview)
      onApply(preview)
  }
  return (
    <div className="grid min-h-[420px] grid-cols-[220px_1fr] overflow-hidden rounded-xl border border-line bg-panel max-[900px]:grid-cols-1">
      <aside className="border-r border-line p-2 max-[900px]:border-b max-[900px]:border-r-0">
        <strong className="px-2 text-[10px] uppercase tracking-wider text-muted">{t('studio.devtools.diff.drafts')}</strong>
        <div className="mt-2 space-y-1">
          {drafts.map(draft => (
            <button key={draft.id} type="button" className="w-full rounded-lg p-2 text-left hover:bg-panel2" onClick={() => onShow(draft.id)}>
              <strong className="block truncate text-[11px] text-ink">{draft.intent}</strong>
              <small className="text-[9px] text-muted">
                r
                {draft.revision}
                {' '}
                ·
                {draft.status}
              </small>
            </button>
          ))}
        </div>
      </aside>
      <div className="min-w-0 p-4">
        <If cond={preview != null} else={<CenteredNotice title={t('studio.devtools.diff.empty')} description={t('studio.devtools.diff.empty_desc')} />}>
          <div>
            <div className="mb-3 flex justify-end">
              <Button className="bg-accent text-white" size="sm" isPending={applying} onPress={applyPreview}>{t('studio.devtools.diff.apply')}</Button>
            </div>
            <div className="space-y-3">
              {preview?.preview.changes.map(change => (
                <div key={change.path} className="overflow-hidden rounded-lg border border-line">
                  <header className="bg-panel2 px-3 py-2 text-[10px] text-ink">{change.path}</header>
                  <pre className="max-h-72 overflow-auto whitespace-pre-wrap p-3 text-[10px] leading-5 text-muted">{change.unifiedDiff ?? t('studio.devtools.diff.binary')}</pre>
                </div>
              ))}
            </div>
          </div>
        </If>
      </div>
    </div>
  )
}

function ValidationPanel({ report }: { report: DomainValidationReport | null }) {
  const { t } = useTranslation()
  if (!report)
    return <CenteredNotice title={t('studio.devtools.validation.empty')} description={t('studio.devtools.validation.empty_desc')} />
  return (
    <div className="mx-auto max-w-3xl rounded-2xl border border-line bg-panel p-6">
      <ValidationStateIcon valid={report.valid} />
      <h2 className="mt-3 text-lg font-semibold text-ink">{t(validationTitleKey(report.valid))}</h2>
      <p className="mt-1 text-sm text-muted">{t('studio.devtools.validation.summary', { owned: report.ownedFiles, writable: report.writableFiles, readonly: report.readonlyFiles })}</p>
      <div className="mt-5 space-y-2">{report.diagnostics.map(diagnostic => <div key={diagnostic} className="rounded-lg border border-line bg-panel2 px-3 py-2 text-xs text-muted">{diagnostic}</div>)}</div>
    </div>
  )
}

function ValidationStateIcon({ valid }: { valid: boolean }) {
  if (valid)
    return <CircleCheck className={validationIconClass(valid)} />
  return <CircleExclamation className={validationIconClass(valid)} />
}

function Metric({ label, value }: { label: string, value: number }) {
  return (
    <div className="rounded-xl border border-line bg-panel px-4 py-3">
      <strong className="block text-xl tabular-nums text-ink">{value}</strong>
      <span className="text-[10px] text-muted">{label}</span>
    </div>
  )
}

function CenteredNotice({ title, description }: { title: string, description: string }) {
  return (
    <div className="grid min-h-[360px] place-items-center">
      <div className="max-w-sm text-center">
        <strong className="text-sm text-ink">{title}</strong>
        <p className="mt-2 text-xs leading-5 text-muted">{description}</p>
      </div>
    </div>
  )
}

function NoProject() {
  const { t } = useTranslation()
  return <CenteredNotice title={t('studio.devtools.no_project')} description={t('studio.devtools.no_project_desc')} />
}

function isTextFile(file?: DomainFileRecord | null): boolean {
  const extension = file?.extension?.toLowerCase()
  return extension === 'txt' || extension === 'lua'
}

function canEditSource(file?: DomainFileRecord | null): boolean {
  if (!isTextFile(file) || file?.access === 'readonly')
    return false
  return file?.ownership === 'owned' || file?.ownership === 'shared'
}

function fallbackManifest(tool: DevToolDefinition): DomainManifest {
  return {
    kind: 'domain',
    systemId: tool.id,
    version: '1.0.0',
    kernelApiRange: '^1.0.0',
    supportedEngineRange: '*',
    manifestSchemaVersion: 1,
    resourceSchemaVersion: 1,
    capabilitySchemaVersion: 1,
    memorySchemaVersion: 1,
    category: tool.category,
    complexity: 1,
    renderer: 'table-v1',
    fileProjection: { keywords: [], editableExtensions: ['txt', 'lua'], structuredExtensions: ['xls'], readonlyExtensions: [] },
    dependencies: [],
    capabilities: [],
  }
}

function renderSystemAiPanel(
  project: Mir3Project | null,
  manifest: DomainManifest,
  selectedPath?: string,
  selectedResourceId?: string,
  draftId?: string,
  onDraftHandoff?: (handoff: DomainDraftHandoff) => Promise<void>,
) {
  if (!project)
    return null
  return <SystemAiPanel project={project} manifest={manifest} selectedPath={selectedPath} selectedResourceId={selectedResourceId} draftId={draftId} onDraftHandoff={onDraftHandoff} />
}

async function consumeNavigationTarget(
  project: Mir3Project,
  target: VerifiedDevtoolsTarget,
  files: DomainFileRecord[],
  selectFile: (file: DomainFileRecord) => void,
  setDraftPreview: (preview: DomainDraftConfirmation | null) => void,
  setValidation: (report: DomainValidationReport | null) => void,
  setCenterTab: (tab: CenterTab) => void,
): Promise<void> {
  if (target.relativePath) {
    const file = files.find(item => item.path === target.relativePath)
    if (!file)
      throw new Error('DEVTOOLS_RETURN_RESOURCE_NOT_PROJECTED')
    selectFile(file)
  }
  if (!target.draftId)
    return
  const [preview, report] = await Promise.all([
    previewDomainDraft(project.id, target.draftId),
    validateDomainDraft(project.id, target.draftId),
  ])
  if (report.systemId !== target.systemId || (target.revision != null && preview.preview.draft.revision < target.revision))
    throw new Error('DEVTOOLS_RETURN_DRAFT_SCOPE_MISMATCH')
  setDraftPreview(preview)
  setValidation(report)
  setCenterTab(report.valid ? 'diff' : 'validation')
}

function sidebarTabClass(active: boolean) {
  if (active)
    return 'rounded-md bg-accent/14 px-1 py-1.5 text-[10px] font-medium text-accent'
  return 'rounded-md px-1 py-1.5 text-[10px] text-muted hover:bg-panel2 hover:text-ink'
}

function projectionButtonClass(selected: boolean) {
  if (selected)
    return 'w-full rounded-lg border border-accent/30 bg-accent/8 p-2 text-left'
  return 'w-full rounded-lg border border-transparent p-2 text-left hover:border-line hover:bg-panel2'
}

function projectionLabel(file: DomainFileRecord, resourceMode: boolean) {
  if (resourceMode)
    return file.resourceId
  return file.path.split('/').at(-1)
}

function accessBadgeClass(access: DomainFileRecord['access']) {
  if (access === 'readonly')
    return 'rounded bg-warning/10 px-1 text-[8px] text-warning'
  return 'rounded bg-success/10 px-1 text-[8px] text-success'
}

function centerTabClass(active: boolean) {
  if (active)
    return 'bg-accent/14 text-accent'
  return 'text-muted'
}

function validationIconClass(valid: boolean) {
  if (valid)
    return 'size-7 text-success'
  return 'size-7 text-warning'
}

function validationTitleKey(valid: boolean) {
  if (valid)
    return 'studio.devtools.validation.passed'
  return 'studio.devtools.validation.failed'
}

function packToggleLabelKey(enabled: boolean) {
  if (enabled)
    return 'studio.devtools.pack.disable'
  return 'studio.devtools.pack.enable'
}

function packToggleConfirmationKey(enabled: boolean) {
  if (enabled)
    return 'studio.devtools.pack.disable_confirm'
  return 'studio.devtools.pack.enable_confirm'
}
