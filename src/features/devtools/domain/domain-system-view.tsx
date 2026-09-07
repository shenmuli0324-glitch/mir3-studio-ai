import type { DevToolDefinition } from '../devtool-registry'
import type { DomainDraftPreview, DomainFileRecord, DomainManifest, DomainValidationReport, DomainWorkingCopy, SafeTextOpen, SafeXlsSheet, SafeXlsWorkbook } from './types'
import type { Mir3Project } from '@/features/projects/types'
import type { DomainWorkingCopyHandoff, VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { File, Folder, Magnifier } from '@gravity-ui/icons'
import { useOverlay } from '@overlastic/react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useDeferredValue, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { Modal as ConfirmationModal } from '@/components/modal'
import { SystemAiPanel } from '@/features/system-ai/system-ai-panel'
import { toast } from '@/utils'
import { DevToolWorkspace } from '../shell/devtool-workspace'
import {
  listDomainSaveNodes,
  listDomainSystems,
  openDomainWorkingCopy,
  openDomainWorkingText,
  openDomainXls,
  patchDomainWorkingText,
  previewDomainWorkingCopy,
  queryDomainFiles,
  readDomainXlsSheet,
  restoreDomainSaveNode,
  saveDomainWorkingCopy,
} from './api'
import { DomainChangeReviewDialog } from './domain-change-review'
import { isSaveConfirmationRequired, requiresSaveConfirmation } from './domain-save-policy'
import { DomainWorkingToolbar } from './domain-working-toolbar'

interface WorkingTextEdit {
  content: string
  baseSha256: string
}

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
  const [confirmationHolder, openConfirmation] = useOverlay(ConfirmationModal, { type: 'holder' })
  const [search, setSearch] = useState('')
  const deferredSearch = useDeferredValue(search)
  const [selectedFile, setSelectedFile] = useState<DomainFileRecord | null>(null)
  const [editedContent, setEditedContent] = useState<string | null>(null)
  const [selectedSheet, setSelectedSheet] = useState('')
  const [workingCopy, setWorkingCopy] = useState<DomainWorkingCopy | null>(null)
  const [workingEdits, setWorkingEdits] = useState<Record<string, WorkingTextEdit>>({})
  const [changePreview, setChangePreview] = useState<DomainDraftPreview | null>(null)
  const [validation, setValidation] = useState<DomainValidationReport | null>(null)
  const [reviewOpen, setReviewOpen] = useState(false)
  const [syncing, setSyncing] = useState(false)
  const handledTargetRef = useRef('')
  const workingCopyRef = useRef<DomainWorkingCopy | null>(null)
  const workingEditsRef = useRef<Record<string, WorkingTextEdit>>({})
  const syncPromiseRef = useRef<Promise<DomainWorkingCopy | null> | null>(null)
  const acceptWorkingCopyHandoffRef = useRef(acceptWorkingCopyHandoff)
  acceptWorkingCopyHandoffRef.current = acceptWorkingCopyHandoff

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
  const activeWorkingCopyId = workingCopy?.id ?? null
  const openedFile = useQuery({
    queryKey: ['domain-source', project?.id, selectedFile?.path, activeWorkingCopyId],
    queryFn: () => openDomainWorkingText(project!.id, selectedFile!.path, activeWorkingCopyId),
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
  const saveNodes = useQuery({
    queryKey: ['domain-save-nodes', project?.id, tool.id],
    queryFn: () => listDomainSaveNodes(project!.id, tool.id),
    enabled: project != null,
  })

  const saveWorking = useMutation({
    mutationFn: async ({ copy, confirmed }: { copy: DomainWorkingCopy, confirmed: boolean }) => saveDomainWorkingCopy(project!.id, copy.id, copy.revision, confirmed),
    onSuccess: async () => {
      updateWorkingCopy(null)
      updateWorkingEdits({})
      setChangePreview(null)
      setValidation(null)
      setEditedContent(null)
      await invalidateWorkspaceQueries(queryClient, project!.id, tool.id)
      await saveNodes.refetch()
      toast(t('studio.devtools.working.saved'), {})
    },
  })
  const restoreSave = useMutation({
    mutationFn: (nodeId: string) => restoreDomainSaveNode(project!.id, nodeId),
    onSuccess: async () => {
      updateWorkingCopy(null)
      updateWorkingEdits({})
      setChangePreview(null)
      setValidation(null)
      setEditedContent(null)
      await invalidateWorkspaceQueries(queryClient, project!.id, tool.id)
      await saveNodes.refetch()
      toast(t('studio.devtools.working.restored'), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })

  useEffect(() => {
    if (!project || !target || files.isLoading || target.projectId !== project.id || target.systemId !== tool.id || handledTargetRef.current === target.nonce)
      return
    handledTargetRef.current = target.nonce
    void consumeNavigationTarget(target, projectedFiles, selectFile, acceptWorkingCopyHandoffRef.current)
      .catch(reason => toast(String(reason), { variant: 'danger' }))
  }, [files.data, files.isLoading, project, projectedFiles, target, tool.id])

  function selectFile(file: DomainFileRecord | null) {
    setSelectedFile(file)
    setEditedContent(file ? workingEditsRef.current[file.path]?.content ?? null : null)
    setSelectedSheet('')
  }

  async function handleAiWorkingCopyHandoff(handoff: DomainWorkingCopyHandoff) {
    if (!project || handoff.systemId !== tool.id)
      throw new Error('AI_DRAFT_SCOPE_MISMATCH')
    await acceptWorkingCopyHandoff(handoff.workingCopyId, handoff.revision)
    const handoffFile = projectedFiles.find(file => file.resourceId === handoff.resourceId)
    if (handoffFile)
      selectFile(handoffFile)
    await invalidateWorkspaceQueries(queryClient, project.id, tool.id)
  }

  function updateWorkingCopy(copy: DomainWorkingCopy | null) {
    workingCopyRef.current = copy
    setWorkingCopy(copy)
  }

  function updateWorkingEdits(edits: Record<string, WorkingTextEdit>) {
    workingEditsRef.current = edits
    setWorkingEdits(edits)
  }

  function editSource(content: string) {
    if (!selectedFile || !openedFile.data)
      return
    const next = { ...workingEditsRef.current }
    if (content === openedFile.data.content)
      delete next[selectedFile.path]
    else
      next[selectedFile.path] = { content, baseSha256: openedFile.data.sha256 }
    updateWorkingEdits(next)
    setEditedContent(content)
    setValidation(null)
  }

  async function acceptWorkingCopyHandoff(workingCopyId: string, revision: number) {
    if (!project)
      throw new Error('DOMAIN_WORKING_PROJECT_REQUIRED')
    const preview = await previewDomainWorkingCopy(project.id, workingCopyId)
    if (preview.draft.revision < revision)
      throw new Error('DOMAIN_WORKING_REVISION_CONFLICT')
    updateWorkingCopy({
      id: workingCopyId,
      systemId: tool.id,
      pluginVersion: manifest.version,
      revision: preview.draft.revision,
      dirty: preview.changes.length > 0,
      createdAt: preview.draft.createdAt,
      updatedAt: preview.draft.updatedAt,
    })
    setChangePreview(preview)
    setValidation(null)
  }

  async function ensureWorkingCopyBeforeAi() {
    const copy = await flushWorkingEdits()
    if (copy)
      return { workingCopyId: copy.id, revision: copy.revision }
    if (!project)
      throw new Error('DOMAIN_WORKING_PROJECT_REQUIRED')
    const opened = await openDomainWorkingCopy(project.id, manifest.systemId, manifest.version)
    updateWorkingCopy(opened)
    return { workingCopyId: opened.id, revision: opened.revision }
  }

  async function flushWorkingEdits(): Promise<DomainWorkingCopy | null> {
    if (syncPromiseRef.current)
      await syncPromiseRef.current
    if (Object.keys(workingEditsRef.current).length === 0)
      return workingCopyRef.current
    const pending = syncWorkingEdits()
    syncPromiseRef.current = pending
    setSyncing(true)
    try {
      return await pending
    }
    finally {
      syncPromiseRef.current = null
      setSyncing(false)
    }
  }

  async function syncWorkingEdits(): Promise<DomainWorkingCopy | null> {
    if (!project)
      return null
    let copy = workingCopyRef.current
    if (!copy) {
      copy = await openDomainWorkingCopy(project.id, manifest.systemId, manifest.version, t('studio.devtools.working.intent'))
      updateWorkingCopy(copy)
    }
    const edits = Object.entries(workingEditsRef.current)
    for (const [path, edit] of edits) {
      const opened = await openDomainWorkingText(project.id, path, copy.id)
      if (opened.sha256 !== edit.baseSha256)
        throw new Error('SAFE_FILE_SOURCE_CONFLICT')
      if (opened.content !== edit.content) {
        const result = await patchDomainWorkingText(project.id, copy.id, opened, edit.content)
        copy = { ...copy, revision: result.revision, dirty: true, updatedAt: Date.now() }
        updateWorkingCopy(copy)
      }
      const current = { ...workingEditsRef.current }
      if (current[path]?.content === edit.content)
        delete current[path]
      updateWorkingEdits(current)
    }
    const preview = await previewDomainWorkingCopy(project.id, copy.id)
    setChangePreview(preview)
    copy = { ...copy, revision: preview.draft.revision, dirty: preview.changes.length > 0, updatedAt: preview.draft.updatedAt }
    updateWorkingCopy(copy)
    await invalidateWorkspaceQueries(queryClient, project.id, tool.id)
    return copy
  }

  async function saveChanges() {
    try {
      const copy = await flushWorkingEdits()
      if (!copy || !copy.dirty)
        return
      const preview = await previewDomainWorkingCopy(project!.id, copy.id)
      setChangePreview(preview)
      let confirmed = false
      if (requiresSaveConfirmation(tool.id, preview)) {
        confirmed = await confirmRiskSave(preview.changes.length)
        if (!confirmed)
          return
      }
      try {
        await saveWorking.mutateAsync({ copy, confirmed })
      }
      catch (reason) {
        if (confirmed || !isSaveConfirmationRequired(reason))
          throw reason
        if (await confirmRiskSave(preview.changes.length))
          await saveWorking.mutateAsync({ copy, confirmed: true })
      }
    }
    catch (reason) {
      toast(String(reason), { variant: 'danger' })
    }
  }

  async function confirmRiskSave(changeCount: number): Promise<boolean> {
    try {
      await openConfirmation({ status: 'warning', title: t('studio.devtools.working.risk_title'), description: <p>{t('studio.devtools.working.risk_confirm', { count: changeCount })}</p> })
      return true
    }
    catch {
      return false
    }
  }

  async function reviewChanges() {
    try {
      const copy = await flushWorkingEdits()
      if (!copy)
        return
      setChangePreview(await previewDomainWorkingCopy(project!.id, copy.id))
      setReviewOpen(true)
    }
    catch (reason) {
      toast(String(reason), { variant: 'danger' })
    }
  }

  function restorePreviousSave() {
    const node = saveNodes.data?.[0]
    if (node)
      restoreSave.mutate(node.id)
  }

  const dirty = Object.keys(workingEdits).length > 0 || workingCopy?.dirty === true || (changePreview?.changes.length ?? 0) > 0
  const busy = saveWorking.isPending || restoreSave.isPending || syncing

  return (
    <>
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
        toolbar={(
          <DomainWorkingToolbar
            projectName={project?.name}
            systemId={manifest.systemId}
            selectedPath={selectedFile?.path}
            dirty={dirty}
            changeCount={changePreview?.changes.length ?? Object.keys(workingEdits).length}
            canRestore={(saveNodes.data?.length ?? 0) > 0}
            busy={busy}
            onSave={() => void saveChanges()}
            onRestore={restorePreviousSave}
            onReview={() => void reviewChanges()}
          />
        )}
        rightPanel={renderSystemAiPanel(project, manifest, selectedFile?.path, selectedFile?.resourceId, activeWorkingCopyId, ensureWorkingCopyBeforeAi, handleAiWorkingCopyHandoff)}
      >
        <If cond={project != null} else={<NoProject />}>
          <div className="flex h-full min-h-0 flex-col">
            <FileSourceWorkspace
              selectedFile={selectedFile}
              openedFile={openedFile.data}
              sourceLoading={openedFile.isLoading}
              sourceError={openedFile.error}
              editedContent={editedContent}
              onEditedContent={editSource}
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
      <If cond={reviewOpen}>
        <DomainChangeReviewDialog preview={changePreview} validation={validation} onClose={() => setReviewOpen(false)} />
      </If>
      {confirmationHolder}
    </>
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

function FileSourceWorkspace(props: {
  selectedFile: DomainFileRecord | null
  openedFile?: SafeTextOpen
  sourceLoading: boolean
  sourceError: Error | null
  editedContent: string | null
  onEditedContent: (content: string) => void
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
        <If cond={editable && content !== props.openedFile.content}>
          <small className="text-[9px] text-accent">{t('studio.devtools.working.buffered')}</small>
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
  if (sheetLoading)
    return <CenteredNotice title={t('studio.devtools.source.xls_sheet_loading')} description={sheetName} />
  if (sheetError || !sheet)
    return <CenteredNotice title={t('studio.devtools.source.xls_failed')} description={String(sheetError ?? '')} />
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
      <div className="min-h-0 flex-1 overflow-auto">
        <pre className="min-w-max whitespace-pre p-4 font-mono text-[11px] leading-5 text-ink">{xlsTsvPreview(sheet)}</pre>
      </div>
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
  workingCopyId?: string | null,
  ensureWorkingCopy?: () => Promise<{ workingCopyId: string, revision: number }>,
  onWorkingCopyHandoff?: (handoff: DomainWorkingCopyHandoff) => Promise<void>,
) {
  if (!project)
    return null
  return (
    <SystemAiPanel
      project={project}
      manifest={manifest}
      selectedPath={selectedPath}
      selectedResourceId={selectedResourceId}
      workingCopyId={workingCopyId}
      ensureWorkingCopy={ensureWorkingCopy}
      onWorkingCopyHandoff={onWorkingCopyHandoff}
    />
  )
}

async function consumeNavigationTarget(
  target: VerifiedDevtoolsTarget,
  files: DomainFileRecord[],
  selectFile: (file: DomainFileRecord | null) => void,
  acceptWorkingCopyHandoff: (workingCopyId: string, revision: number) => Promise<void>,
): Promise<void> {
  if (target.relativePath) {
    const file = files.find(item => item.path === target.relativePath)
    if (!file)
      throw new Error('DEVTOOLS_RETURN_RESOURCE_NOT_PROJECTED')
    selectFile(file)
  }
  const workingCopyId = target.workingCopyId ?? target.draftId
  if (!workingCopyId)
    return
  await acceptWorkingCopyHandoff(workingCopyId, target.revision ?? 0)
}

async function invalidateWorkspaceQueries(queryClient: ReturnType<typeof useQueryClient>, projectId: string, systemId: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['domain-files', projectId, systemId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-source', projectId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-xls', projectId] }),
    queryClient.invalidateQueries({ queryKey: ['domain-xls-sheet', projectId] }),
  ])
}

function fileTreeButtonClass(selected: boolean) {
  if (selected)
    return 'flex w-full items-center gap-1.5 rounded-md bg-accent/14 px-1.5 py-1 text-left text-[11px] text-accent'
  return 'flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[11px] text-ink hover:bg-panel2'
}
