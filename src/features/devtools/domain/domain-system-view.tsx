import type { DevToolDefinition } from '../devtool-registry'
import type { DomainDraftConfirmation, DomainFileRecord, DomainManifest, DomainSnapshot, DomainValidationReport, SafeTextOpen, SafeXlsSheet, SafeXlsWorkbook } from './types'
import type { Mir3Project } from '@/features/projects/types'
import type { DomainDraftHandoff, VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { CircleCheck, CircleExclamation, File, Folder, Magnifier } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useDeferredValue, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { SystemAiPanel } from '@/features/system-ai/system-ai-panel'
import { toast } from '@/utils'
import { DevToolWorkspace } from '../shell/devtool-workspace'
import {
  applyDomainDraft,
  discardDomainDraft,
  listDomainSystems,
  openDomainDraft,
  openDomainText,
  openDomainXls,
  patchDomainText,
  previewDomainDraft,
  queryDomainFiles,
  readDomainXlsSheet,
  validateDomainDraft,
} from './api'

interface MutableFileTree {
  directories: Map<string, MutableFileTree>
  files: DomainFileRecord[]
}

interface FileTreeNode {
  name: string
  path: string
  directories: FileTreeNode[]
  files: DomainFileRecord[]
}

export function DomainSystemView({ tool, project, onBack, target }: {
  tool: DevToolDefinition
  project: Mir3Project | null
  onBack: () => void
  onOpenSystem?: (systemId: string) => void
  target?: VerifiedDevtoolsTarget | null
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const deferredSearch = useDeferredValue(search)
  const [selectedFile, setSelectedFile] = useState<DomainFileRecord | null>(null)
  const [editedContent, setEditedContent] = useState<string | null>(null)
  const [selectedSheet, setSelectedSheet] = useState('')
  const [draftPreview, setDraftPreview] = useState<DomainDraftConfirmation | null>(null)
  const [validation, setValidation] = useState<DomainValidationReport | null>(null)
  const handledTargetRef = useRef('')

  const manifests = useQuery({
    queryKey: ['domain-systems'],
    queryFn: listDomainSystems,
    enabled: project != null,
  })
  const manifest = manifests.data?.find(item => item.systemId === tool.id) ?? fallbackManifest(tool)
  const files = useQuery({
    queryKey: ['domain-files', project?.id, tool.id, deferredSearch],
    queryFn: () => queryDomainFiles(project!.id, tool.id, deferredSearch),
    enabled: project != null,
  })
  const projectedFiles = currentSystemFiles(files.data ?? [])
  const activeDraftId = draftPreview?.preview.draft.id ?? null
  const openedFile = useQuery({
    queryKey: ['domain-source', project?.id, selectedFile?.path, activeDraftId],
    queryFn: () => openDomainText(project!.id, selectedFile!.path, activeDraftId),
    enabled: project != null && isTextFile(selectedFile),
  })
  const workbook = useQuery({
    queryKey: ['domain-xls', project?.id, selectedFile?.path],
    queryFn: () => openDomainXls(project!.id, selectedFile!.path),
    enabled: project != null && isXlsFile(selectedFile),
  })
  const sheetName = selectedSheet || workbook.data?.sheets[0]?.name || ''
  const sheet = useQuery({
    queryKey: ['domain-xls-sheet', project?.id, selectedFile?.path, workbook.data?.sha256, sheetName],
    queryFn: () => readDomainXlsSheet(project!.id, selectedFile!.path, sheetName, workbook.data!.sha256),
    enabled: project != null && isXlsFile(selectedFile) && workbook.data != null && sheetName.length > 0,
  })

  const patch = useMutation({
    mutationFn: async ({ opened, content }: { opened: SafeTextOpen, content: string }) => {
      if (opened.draftId)
        return patchDomainText(project!.id, opened, content)
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
      setValidation(null)
      setEditedContent(null)
      await invalidateWorkspaceQueries(queryClient, project!.id, tool.id)
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
    onSuccess: async (_snapshot: DomainSnapshot) => {
      setDraftPreview(null)
      setValidation(null)
      setEditedContent(null)
      await invalidateWorkspaceQueries(queryClient, project!.id, tool.id)
      toast(t('studio.devtools.diff.applied'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const discardDraft = useMutation({
    mutationFn: (draftId: string) => discardDomainDraft(project!.id, draftId),
    onSuccess: async () => {
      setDraftPreview(null)
      setValidation(null)
      setEditedContent(null)
      await invalidateWorkspaceQueries(queryClient, project!.id, tool.id)
      toast(t('studio.devtools.draft.discarded'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })
  const validateDraft = useMutation({
    mutationFn: (draftId: string) => validateDomainDraft(project!.id, draftId),
    onSuccess: setValidation,
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })

  useEffect(() => {
    if (!project || !target || files.isLoading || target.projectId !== project.id || target.systemId !== tool.id || handledTargetRef.current === target.nonce)
      return
    handledTargetRef.current = target.nonce
    void consumeNavigationTarget(project, target, projectedFiles, selectFile, setDraftPreview, setValidation)
      .catch(reason => toast(String(reason), { variant: 'danger' }))
  }, [files.data, files.isLoading, project, projectedFiles, target, tool.id])

  function selectFile(file: DomainFileRecord | null) {
    setSelectedFile(file)
    setEditedContent(null)
    setSelectedSheet('')
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
    const handoffFile = projectedFiles.find(file => file.resourceId === handoff.resourceId)
    if (handoffFile)
      selectFile(handoffFile)
    await invalidateWorkspaceQueries(queryClient, project.id, tool.id)
  }

  function saveSource() {
    if (!canEditSource(selectedFile) || !openedFile.data || editedContent == null || editedContent === openedFile.data.content)
      return
    patch.mutate({ opened: openedFile.data, content: editedContent })
  }

  function validateActiveDraft() {
    if (!project || !draftPreview)
      return
    validateDraft.mutate(draftPreview.preview.draft.id)
  }

  function applyActiveDraft() {
    if (!draftPreview)
      return
    // eslint-disable-next-line no-alert
    if (window.confirm(t('studio.devtools.diff.apply_confirm')))
      applyDraft.mutate(draftPreview)
  }

  function discardActiveDraft() {
    if (!draftPreview)
      return
    // eslint-disable-next-line no-alert
    if (window.confirm(t('studio.devtools.draft.discard_confirm')))
      discardDraft.mutate(draftPreview.preview.draft.id)
  }

  return (
    <DevToolWorkspace
      tool={tool}
      onBack={onBack}
      sidebar={(
        <DomainFileSidebar
          files={projectedFiles}
          loading={files.isLoading}
          search={search}
          selectedPath={selectedFile?.path}
          onSearch={setSearch}
          onSelect={selectFile}
        />
      )}
      toolbar={<FileWorkspaceToolbar manifest={manifest} project={project} selectedPath={selectedFile?.path} />}
      rightPanel={renderSystemAiPanel(project, manifest, selectedFile?.path, selectedFile?.resourceId, activeDraftId ?? undefined, handleAiDraftHandoff)}
    >
      <If cond={project != null} else={<NoProject />}>
        <div className="flex h-full min-h-0 flex-col">
          <If cond={draftPreview != null}>
            <CompactDraftBar
              preview={draftPreview}
              validation={validation}
              validating={validateDraft.isPending}
              applying={applyDraft.isPending}
              discarding={discardDraft.isPending}
              onValidate={() => void validateActiveDraft()}
              onApply={applyActiveDraft}
              onDiscard={discardActiveDraft}
            />
          </If>
          <FileSourceWorkspace
            selectedFile={selectedFile}
            openedFile={openedFile.data}
            sourceLoading={openedFile.isLoading}
            sourceError={openedFile.error}
            editedContent={editedContent}
            onEditedContent={setEditedContent}
            onSaveSource={saveSource}
            saving={patch.isPending}
            workbook={workbook.data}
            workbookLoading={workbook.isLoading}
            workbookError={workbook.error}
            sheetName={sheetName}
            sheet={sheet.data}
            sheetLoading={sheet.isLoading}
            sheetError={sheet.error}
            onSheet={setSelectedSheet}
          />
        </div>
      </If>
    </DevToolWorkspace>
  )
}

function DomainFileSidebar({ files, loading, search, selectedPath, onSearch, onSelect }: {
  files: DomainFileRecord[]
  loading: boolean
  search: string
  selectedPath?: string
  onSearch: (value: string) => void
  onSelect: (file: DomainFileRecord) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-line p-2">
        <label className="flex items-center gap-2 rounded-lg border border-line bg-panel2 px-2.5 py-2">
          <Magnifier className="size-3.5 text-muted" />
          <input className="min-w-0 flex-1 bg-transparent text-xs text-ink outline-none placeholder:text-muted" value={search} placeholder={t('studio.devtools.files.search')} aria-label={t('studio.devtools.files.search')} onChange={event => onSearch(event.target.value)} />
        </label>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-2">
        <If cond={!loading} else={<p className="p-4 text-center text-xs text-muted">{t('studio.devtools.resources.loading')}</p>}>
          <If cond={files.length > 0} else={<p className="p-4 text-center text-xs leading-5 text-muted">{t('studio.devtools.files.empty')}</p>}>
            <DirectoryTree files={files} selectedPath={selectedPath} onSelect={onSelect} />
          </If>
        </If>
      </div>
    </div>
  )
}

function DirectoryTree({ files, selectedPath, onSelect }: { files: DomainFileRecord[], selectedPath?: string, onSelect: (file: DomainFileRecord) => void }) {
  const tree = buildFileTree(files)
  return (
    <div className="space-y-0.5">
      {tree.files.map(file => <FileTreeButton key={file.path} file={file} selected={selectedPath === file.path} onSelect={onSelect} />)}
      {tree.directories.map(directory => <DirectoryBranch key={directory.path} node={directory} selectedPath={selectedPath} onSelect={onSelect} depth={0} />)}
    </div>
  )
}

function DirectoryBranch({ node, selectedPath, onSelect, depth }: { node: FileTreeNode, selectedPath?: string, onSelect: (file: DomainFileRecord) => void, depth: number }) {
  return (
    <details open={depth < 1} className="group">
      <summary className="flex cursor-pointer list-none items-center gap-1.5 rounded-md px-1.5 py-1 text-[11px] text-muted hover:bg-panel2 hover:text-ink">
        <Folder className="size-3 shrink-0 text-accent" />
        <span className="truncate">{node.name}</span>
      </summary>
      <div className="ml-2 border-l border-line/70 pl-1.5">
        {node.files.map(file => <FileTreeButton key={file.path} file={file} selected={selectedPath === file.path} onSelect={onSelect} />)}
        {node.directories.map(directory => <DirectoryBranch key={directory.path} node={directory} selectedPath={selectedPath} onSelect={onSelect} depth={depth + 1} />)}
      </div>
    </details>
  )
}

function FileTreeButton({ file, selected, onSelect }: { file: DomainFileRecord, selected: boolean, onSelect: (file: DomainFileRecord) => void }) {
  return (
    <button type="button" className={fileTreeButtonClass(selected)} title={file.path} onClick={() => onSelect(file)}>
      <File className="size-3 shrink-0 text-muted" />
      <span className="truncate">{fileName(file.path)}</span>
    </button>
  )
}

function FileWorkspaceToolbar({ manifest, project, selectedPath }: { manifest: DomainManifest, project: Mir3Project | null, selectedPath?: string }) {
  const { t } = useTranslation()
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <span className="min-w-0 shrink-0">
        <strong className="block truncate text-xs font-medium text-ink">{t(`studio.devtools.tool.${manifest.systemId}.title`)}</strong>
        <small className="block truncate text-[9px] text-muted">{project?.name ?? t('studio.devtools.no_project')}</small>
      </span>
      <If cond={selectedPath != null}>
        <span className="min-w-0 truncate border-l border-line pl-3 font-mono text-[10px] text-muted">{selectedPath}</span>
      </If>
    </div>
  )
}

function CompactDraftBar({ preview, validation, validating, applying, discarding, onValidate, onApply, onDiscard }: {
  preview: DomainDraftConfirmation | null
  validation: DomainValidationReport | null
  validating: boolean
  applying: boolean
  discarding: boolean
  onValidate: () => void
  onApply: () => void
  onDiscard: () => void
}) {
  const { t } = useTranslation()
  if (!preview)
    return null
  const valid = validation?.valid === true
  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-line bg-panel px-4 py-2">
      <DraftStateIcon validation={validation} />
      <span className="min-w-0 flex-1">
        <strong className="block truncate text-[11px] text-ink">{preview.preview.draft.intent}</strong>
        <small className={draftMessageClass(validation)}>{draftMessage(t, preview, validation)}</small>
      </span>
      <div className="flex shrink-0 items-center gap-1.5">
        <Button size="sm" variant="ghost" isPending={validating} onPress={onValidate}>{t('studio.devtools.draft.validate')}</Button>
        <Button size="sm" className="bg-accent text-white" isDisabled={!valid} isPending={applying} onPress={onApply}>{t('studio.devtools.draft.apply')}</Button>
        <Button size="sm" variant="ghost" className="text-danger" isPending={discarding} onPress={onDiscard}>{t('studio.devtools.draft.discard')}</Button>
      </div>
    </div>
  )
}

function DraftStateIcon({ validation }: { validation: DomainValidationReport | null }) {
  if (validation?.valid === true)
    return <CircleCheck className="size-4 shrink-0 text-success" />
  if (validation?.valid === false)
    return <CircleExclamation className="size-4 shrink-0 text-danger" />
  return <span className="size-2 shrink-0 rounded-full bg-warning" />
}

function FileSourceWorkspace(props: {
  selectedFile: DomainFileRecord | null
  openedFile?: SafeTextOpen
  sourceLoading: boolean
  sourceError: Error | null
  editedContent: string | null
  onEditedContent: (content: string) => void
  onSaveSource: () => void
  saving: boolean
  workbook?: SafeXlsWorkbook
  workbookLoading: boolean
  workbookError: Error | null
  sheetName: string
  sheet?: SafeXlsSheet
  sheetLoading: boolean
  sheetError: Error | null
  onSheet: (sheet: string) => void
}) {
  const { t } = useTranslation()
  if (!props.selectedFile)
    return <CenteredNotice title={t('studio.devtools.source.empty')} description={t('studio.devtools.source.empty_desc_simple')} />
  if (isXlsFile(props.selectedFile)) {
    return (
      <XlsSourcePreview
        file={props.selectedFile}
        workbook={props.workbook}
        workbookLoading={props.workbookLoading}
        workbookError={props.workbookError}
        sheetName={props.sheetName}
        sheet={props.sheet}
        sheetLoading={props.sheetLoading}
        sheetError={props.sheetError}
        onSheet={props.onSheet}
      />
    )
  }
  if (!isTextFile(props.selectedFile))
    return <CenteredNotice title={t('studio.devtools.source.readonly')} description={t('studio.devtools.source.readonly_desc_simple', { extension: props.selectedFile.extension ?? '' })} />
  if (props.sourceLoading)
    return <CenteredNotice title={t('studio.devtools.source.loading')} description={props.selectedFile.path} />
  if (props.sourceError || !props.openedFile)
    return <CenteredNotice title={t('studio.devtools.source.failed')} description={String(props.sourceError ?? '')} />
  const content = props.editedContent ?? props.openedFile.content
  const editable = canEditSource(props.selectedFile)
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      <header className="flex shrink-0 items-center justify-between border-b border-line px-4 py-2">
        <span className="min-w-0">
          <strong className="block truncate text-xs text-ink">{props.selectedFile.path}</strong>
          <small className="text-[9px] text-muted">{props.openedFile.encoding}</small>
        </span>
        <If cond={editable}>
          <Button size="sm" className="bg-accent text-white" isDisabled={content === props.openedFile.content} isPending={props.saving} onPress={props.onSaveSource}>{t('studio.devtools.source.stage')}</Button>
        </If>
      </header>
      <textarea readOnly={!editable} className="min-h-0 flex-1 resize-none bg-canvas p-4 font-mono text-xs leading-5 text-ink outline-none" value={content} aria-label={t('studio.devtools.source.editor')} onChange={event => props.onEditedContent(event.target.value)} />
    </div>
  )
}

function XlsSourcePreview({ file, workbook, workbookLoading, workbookError, sheetName, sheet, sheetLoading, sheetError, onSheet }: {
  file: DomainFileRecord
  workbook?: SafeXlsWorkbook
  workbookLoading: boolean
  workbookError: Error | null
  sheetName: string
  sheet?: SafeXlsSheet
  sheetLoading: boolean
  sheetError: Error | null
  onSheet: (sheet: string) => void
}) {
  const { t } = useTranslation()
  if (workbookLoading)
    return <CenteredNotice title={t('studio.devtools.source.xls_loading')} description={file.path} />
  if (workbookError || !workbook)
    return <CenteredNotice title={t('studio.devtools.source.xls_failed')} description={String(workbookError ?? '')} />
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      <header className="flex shrink-0 items-center justify-between gap-3 border-b border-line px-4 py-2">
        <span className="min-w-0">
          <strong className="block truncate text-xs text-ink">{file.path}</strong>
          <small className="text-[9px] text-muted">{t('studio.devtools.source.xls_readonly')}</small>
        </span>
        <If cond={workbook.sheets.length > 1}>
          <select className="max-w-52 rounded-md border border-line bg-panel2 px-2 py-1 text-[10px] text-ink outline-none" value={sheetName} aria-label={t('studio.devtools.source.xls_sheet')} onChange={event => onSheet(event.target.value)}>
            {workbook.sheets.map(item => <option key={item.name} value={item.name}>{item.name}</option>)}
          </select>
        </If>
      </header>
      <If cond={!sheetLoading} else={<CenteredNotice title={t('studio.devtools.source.xls_sheet_loading')} description={sheetName} />}>
        <If cond={!sheetError && sheet != null} else={<CenteredNotice title={t('studio.devtools.source.xls_failed')} description={String(sheetError ?? '')} />}>
          <div className="min-h-0 flex-1 overflow-auto">
            <pre className="min-w-max whitespace-pre p-4 font-mono text-[11px] leading-5 text-ink">{xlsTsvPreview(sheet!)}</pre>
          </div>
        </If>
      </If>
    </div>
  )
}

function CenteredNotice({ title, description }: { title: string, description: string }) {
  return (
    <div className="grid min-h-0 flex-1 place-items-center p-6">
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

function currentSystemFiles(files: DomainFileRecord[]) {
  const unique = new Map<string, DomainFileRecord>()
  files.forEach((file) => {
    if ((file.ownership === 'owned' || file.ownership === 'shared') && !unique.has(file.path))
      unique.set(file.path, file)
  })
  return [...unique.values()].sort((left, right) => left.path.localeCompare(right.path, 'zh-CN'))
}

function buildFileTree(files: DomainFileRecord[]): FileTreeNode {
  const root: MutableFileTree = { directories: new Map(), files: [] }
  files.forEach((file) => {
    const segments = file.path.split('/').filter(Boolean)
    let cursor = root
    segments.slice(0, -1).forEach((segment) => {
      let child = cursor.directories.get(segment)
      if (!child) {
        child = { directories: new Map(), files: [] }
        cursor.directories.set(segment, child)
      }
      cursor = child
    })
    cursor.files.push(file)
  })
  return finalizeFileTree('', '', root)
}

function finalizeFileTree(name: string, path: string, node: MutableFileTree): FileTreeNode {
  const directories = [...node.directories.entries()]
    .sort(([left], [right]) => left.localeCompare(right, 'zh-CN'))
    .map(([childName, child]) => finalizeFileTree(childName, joinPath(path, childName), child))
  return {
    name,
    path,
    directories,
    files: [...node.files].sort((left, right) => fileName(left.path).localeCompare(fileName(right.path), 'zh-CN')),
  }
}

function joinPath(parent: string, name: string) {
  if (parent.length === 0)
    return name
  return `${parent}/${name}`
}

function fileName(path: string) {
  return path.split('/').at(-1) ?? path
}

function xlsTsvPreview(sheet: SafeXlsSheet) {
  return sheet.rows
    .slice(0, 500)
    .map(row => row.slice(0, 100).join('\t'))
    .join('\n')
}

function isTextFile(file?: DomainFileRecord | null): boolean {
  const extension = file?.extension?.toLowerCase()
  return extension === 'txt' || extension === 'lua'
}

function isXlsFile(file?: DomainFileRecord | null): boolean {
  return file?.extension?.toLowerCase() === 'xls'
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
    version: '1.3.1',
    kernelApiRange: '^1.0.0',
    supportedEngineRange: '>=1.0.0',
    engineCompatibility: {
      strategy: 'evidence-gated-auto-generalization-v1',
      versionAliases: ['semver', 'v-prefixed-semver', 'major-minor'],
      requiredEvidence: ['project-directory-layout', 'owned-selector-or-content-fingerprint', 'resource-schema-validation'],
      unknownVersionPolicy: 'readonly',
      incompatibleVersionPolicy: 'readonly',
    },
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
  selectFile: (file: DomainFileRecord | null) => void,
  setDraftPreview: (preview: DomainDraftConfirmation | null) => void,
  setValidation: (report: DomainValidationReport | null) => void,
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
}

async function invalidateWorkspaceQueries(queryClient: ReturnType<typeof useQueryClient>, projectId: string, systemId: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['domain-files', projectId, systemId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-source', projectId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-xls', projectId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-xls-sheet', projectId] }),
  ])
}

function draftMessage(t: ReturnType<typeof useTranslation>['t'], preview: DomainDraftConfirmation, validation: DomainValidationReport | null) {
  if (validation?.valid === true)
    return t('studio.devtools.draft.valid', { count: preview.preview.changes.length })
  if (validation?.valid === false)
    return validation.diagnostics[0] ?? t('studio.devtools.draft.invalid')
  return t('studio.devtools.draft.pending', { count: preview.preview.changes.length })
}

function draftMessageClass(validation: DomainValidationReport | null) {
  if (validation?.valid === true)
    return 'block truncate text-[9px] text-success'
  if (validation?.valid === false)
    return 'block truncate text-[9px] text-danger'
  return 'block truncate text-[9px] text-muted'
}

function fileTreeButtonClass(selected: boolean) {
  if (selected)
    return 'flex w-full items-center gap-1.5 rounded-md bg-accent/14 px-1.5 py-1 text-left text-[11px] text-accent'
  return 'flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[11px] text-ink hover:bg-panel2'
}
