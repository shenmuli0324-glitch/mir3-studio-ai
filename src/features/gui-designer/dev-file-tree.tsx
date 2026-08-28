import type { GuiAssetMeta, GuiDevTreeEntry, GuiReadonlyDocument } from './types'
import { ChevronRight, FileCode, FolderOpen, Picture } from '@gravity-ui/icons'
import { Button, Modal } from '@heroui/react'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { guiAssetMetaQueryOptions, guiAssetQueryOptions, guiDevTreeQueryOptions, guiReadonlyDocumentQueryOptions } from './api'

const ROW_HEIGHT = 48
const OVERSCAN = 7

interface DirectoryState {
  entries: GuiDevTreeEntry[]
  nextCursor?: string | null
  loading: boolean
  loaded: boolean
  failed: boolean
}

interface TreeRow {
  id: string
  depth: number
  rowType: 'entry' | 'loading' | 'error' | 'more'
  entry?: GuiDevTreeEntry
  parentPath?: string
  cursor?: string | null
  isNew?: boolean
}

interface AssetPreview {
  meta: GuiAssetMeta
  url: string
}

export function DevFileTree({
  projectId,
  currentPath,
  newPaths,
  onOpenFile,
}: {
  projectId: string
  currentPath: string | null
  newPaths: string[]
  onOpenFile: (path: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const viewportRef = useRef<HTMLDivElement>(null)
  const [directories, setDirectories] = useState<Record<string, DirectoryState>>({})
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})
  const [search, setSearch] = useState('')
  const [scrollTop, setScrollTop] = useState(0)
  const [viewportHeight, setViewportHeight] = useState(320)
  const [readonlyPreview, setReadonlyPreview] = useState<GuiReadonlyDocument | null>(null)
  const [assetPreview, setAssetPreview] = useState<AssetPreview | null>(null)
  const [infoPreview, setInfoPreview] = useState<GuiDevTreeEntry | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewFailed, setPreviewFailed] = useState(false)

  useEffect(() => {
    const viewport = viewportRef.current
    if (!viewport || typeof ResizeObserver === 'undefined')
      return
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (entry)
        setViewportHeight(entry.contentRect.height)
    })
    observer.observe(viewport)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    let cancelled = false
    void queryClient.fetchQuery(guiDevTreeQueryOptions(projectId, '', null)).then((page) => {
      if (cancelled)
        return
      setDirectories({
        '': {
          entries: page.entries,
          nextCursor: page.nextCursor,
          loading: false,
          loaded: true,
          failed: false,
        },
      })
    }).catch(() => {
      if (!cancelled)
        setDirectories({ '': failedDirectoryState() })
    })
    return () => {
      cancelled = true
    }
  }, [projectId, queryClient])

  useEffect(() => {
    return () => {
      if (assetPreview)
        URL.revokeObjectURL(assetPreview.url)
    }
  }, [assetPreview])

  async function loadDirectory(parentPath: string, cursor?: string | null, append = false) {
    setDirectories(value => ({
      ...value,
      [parentPath]: {
        entries: value[parentPath]?.entries ?? [],
        nextCursor: value[parentPath]?.nextCursor,
        loaded: value[parentPath]?.loaded ?? false,
        loading: true,
        failed: false,
      },
    }))
    try {
      const page = await queryClient.fetchQuery(guiDevTreeQueryOptions(projectId, parentPath, cursor))
      setDirectories((value) => {
        const previous = value[parentPath]
        const entries = append ? [...(previous?.entries ?? []), ...page.entries] : page.entries
        return {
          ...value,
          [parentPath]: {
            entries,
            nextCursor: page.nextCursor,
            loading: false,
            loaded: true,
            failed: false,
          },
        }
      })
    }
    catch {
      setDirectories(value => ({
        ...value,
        [parentPath]: {
          ...(value[parentPath] ?? emptyDirectoryState(false)),
          loading: false,
          failed: true,
        },
      }))
    }
  }

  function toggleDirectory(entry: GuiDevTreeEntry) {
    const opening = !expanded[entry.path]
    setExpanded(value => ({ ...value, [entry.path]: opening }))
    if (opening && !directories[entry.path]?.loaded && !directories[entry.path]?.loading)
      void loadDirectory(entry.path)
  }

  async function openEntry(entry: GuiDevTreeEntry) {
    if (entry.entryType === 'directory') {
      toggleDirectory(entry)
      return
    }
    setPreviewFailed(false)
    if (entry.policy === 'editable') {
      await onOpenFile(entry.path).catch(() => setPreviewFailed(true))
      return
    }
    if (entry.policy === 'readonly' && entry.path.toLowerCase().endsWith('.lua')) {
      setPreviewLoading(true)
      try {
        const document = await queryClient.fetchQuery(guiReadonlyDocumentQueryOptions(projectId, entry.path))
        setReadonlyPreview(document)
      }
      catch {
        setPreviewFailed(true)
      }
      finally {
        setPreviewLoading(false)
      }
      return
    }
    if (entry.policy === 'asset') {
      setPreviewLoading(true)
      try {
        const [meta, asset] = await Promise.all([
          queryClient.fetchQuery(guiAssetMetaQueryOptions(projectId, entry.path)),
          queryClient.fetchQuery(guiAssetQueryOptions(projectId, entry.path)),
        ])
        setAssetPreview({ meta, url: URL.createObjectURL(asset.blob) })
      }
      catch {
        setPreviewFailed(true)
      }
      finally {
        setPreviewLoading(false)
      }
      return
    }
    setInfoPreview(entry)
  }

  const rows = buildRows(directories, expanded, search, newPaths)
  const visibleCount = Math.ceil(viewportHeight / ROW_HEIGHT)
  const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
  const endIndex = Math.min(rows.length, startIndex + visibleCount + OVERSCAN * 2)
  const visibleRows = rows.slice(startIndex, endIndex)

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <label className="block shrink-0">
        <span className="sr-only">{t('studio.gui.dev_tree.search')}</span>
        <input
          className="h-8 w-full rounded-lg border border-line bg-panel-2 px-2 text-[10px] text-ink outline-none placeholder:text-muted focus:border-accent"
          type="search"
          value={search}
          placeholder={t('studio.gui.dev_tree.search')}
          onChange={event => setSearch(event.target.value)}
        />
      </label>
      <div
        ref={viewportRef}
        className="min-h-0 flex-1 overflow-auto rounded-lg border border-line/70 bg-panel-2/35"
        onScroll={event => setScrollTop(event.currentTarget.scrollTop)}
      >
        <div className="relative w-full" style={{ height: rows.length * ROW_HEIGHT }}>
          {visibleRows.map((row, visibleIndex) => (
            <TreeRowView
              row={row}
              activePath={currentPath}
              expanded={expanded}
              top={(startIndex + visibleIndex) * ROW_HEIGHT}
              onOpen={openEntry}
              onLoadMore={loadDirectory}
              key={row.id}
            />
          ))}
        </div>
      </div>
      <If cond={rows.length === 0}>
        <p className="shrink-0 px-2 py-3 text-center text-[10px] text-muted">{t('studio.gui.dev_tree.empty')}</p>
      </If>
      <If cond={previewLoading}>
        <p className="shrink-0 px-2 text-center text-[10px] text-muted">{t('studio.gui.dev_tree.preview_loading')}</p>
      </If>
      <If cond={previewFailed}>
        <p className="shrink-0 px-2 text-center text-[10px] text-danger">{t('studio.gui.dev_tree.load_error')}</p>
      </If>
      <If cond={readonlyPreview != null}>
        <ReadonlyPreview document={readonlyPreview} onClose={() => setReadonlyPreview(null)} />
      </If>
      <If cond={assetPreview != null}>
        <AssetPreviewDialog preview={assetPreview} onClose={() => setAssetPreview(null)} />
      </If>
      <If cond={infoPreview != null}>
        <InfoPreviewDialog entry={infoPreview} onClose={() => setInfoPreview(null)} />
      </If>
    </div>
  )
}

function TreeRowView({
  row,
  activePath,
  expanded,
  top,
  onOpen,
  onLoadMore,
}: {
  row: TreeRow
  activePath: string | null
  expanded: Record<string, boolean>
  top: number
  onOpen: (entry: GuiDevTreeEntry) => Promise<void>
  onLoadMore: (parentPath: string, cursor?: string | null, append?: boolean) => Promise<void>
}) {
  const { t } = useTranslation()
  if (row.rowType === 'loading')
    return <StatusRow top={top} depth={row.depth} label={t('studio.gui.dev_tree.loading')} />
  if (row.rowType === 'error')
    return <StatusRow top={top} depth={row.depth} label={t('studio.gui.dev_tree.load_error')} danger />
  if (row.rowType === 'more') {
    return (
      <button
        className="absolute left-0 flex w-full items-center text-[10px] text-accent hover:bg-panel-hover"
        style={{ height: ROW_HEIGHT, top, paddingLeft: 12 + row.depth * 14 }}
        type="button"
        onClick={() => void onLoadMore(row.parentPath ?? '', row.cursor, true)}
      >
        {t('studio.gui.dev_tree.load_more')}
      </button>
    )
  }
  const entry = row.entry
  if (!entry)
    return null
  const isDirectory = entry.entryType === 'directory'
  return (
    <button
      className={treeRowClass(activePath === entry.path)}
      style={{ height: ROW_HEIGHT, top, paddingLeft: 8 + row.depth * 14 }}
      type="button"
      onClick={() => void onOpen(entry)}
    >
      <If cond={isDirectory} then={<ChevronRight className={treeChevronClass(expanded[entry.path])} />} else={<span className="size-3 shrink-0" />} />
      <EntryIcon entry={entry} />
      <span className="min-w-0 flex-1 text-left">
        <span className="flex items-center gap-1.5">
          <span className="min-w-0 truncate text-[10px] text-ink">{entry.name}</span>
          <If cond={row.isNew}><span className="shrink-0 text-[8px] font-semibold uppercase text-accent">{t('studio.gui.new_badge')}</span></If>
        </span>
        <If
          cond={isDirectory}
          then={isDevRootDirectory(entry) ? <span className="block truncate text-[9px] text-muted">{`@ ${t(`studio.gui.dev_tree.description.${entry.descriptionId}`)}`}</span> : null}
          else={<span className="block truncate text-[9px] text-muted">{fileSecondaryLabel(entry, t)}</span>}
        />
      </span>
    </button>
  )
}

function isDevRootDirectory(entry: GuiDevTreeEntry): boolean {
  return entry.entryType === 'directory' && !entry.path.includes('/')
}

function StatusRow({ top, depth, label, danger = false }: { top: number, depth: number, label: string, danger?: boolean }) {
  let className = 'absolute left-0 flex w-full items-center text-[10px] text-muted'
  if (danger)
    className = 'absolute left-0 flex w-full items-center text-[10px] text-danger'
  return <div className={className} style={{ height: ROW_HEIGHT, top, paddingLeft: 12 + depth * 14 }}>{label}</div>
}

function ReadonlyPreview({ document, onClose }: { document: GuiReadonlyDocument | null, onClose: () => void }) {
  const { t } = useTranslation()
  if (!document)
    return null
  return (
    <PreviewDialog title={t('studio.gui.dev_tree.readonly_title')} subtitle={document.devRelativePath} onClose={onClose}>
      <pre className="max-h-[62vh] overflow-auto whitespace-pre p-4 font-mono text-[11px] leading-5 text-ink">{document.source}</pre>
    </PreviewDialog>
  )
}

function AssetPreviewDialog({ preview, onClose }: { preview: AssetPreview | null, onClose: () => void }) {
  const { t } = useTranslation()
  if (!preview)
    return null
  return (
    <PreviewDialog title={t('studio.gui.dev_tree.asset_title')} subtitle={preview.meta.logicalPath} onClose={onClose}>
      <div className="grid max-h-[65vh] min-h-64 place-items-center overflow-auto bg-[linear-gradient(45deg,rgba(128,128,128,.08)_25%,transparent_25%,transparent_75%,rgba(128,128,128,.08)_75%),linear-gradient(45deg,rgba(128,128,128,.08)_25%,transparent_25%,transparent_75%,rgba(128,128,128,.08)_75%)] bg-[length:20px_20px] bg-[position:0_0,10px_10px] p-8">
        <img className="max-h-[52vh] max-w-full object-contain" src={preview.url} alt={preview.meta.logicalPath} />
      </div>
      <p className="border-t border-line px-4 py-2 text-[10px] text-muted">
        {t('studio.gui.dev_tree.asset_meta', { width: preview.meta.width, height: preview.meta.height, size: formatBytes(preview.meta.byteLength) })}
      </p>
    </PreviewDialog>
  )
}

function InfoPreviewDialog({ entry, onClose }: { entry: GuiDevTreeEntry | null, onClose: () => void }) {
  const { t } = useTranslation()
  if (!entry)
    return null
  return (
    <PreviewDialog title={t('studio.gui.dev_tree.info_title')} subtitle={entry.path} onClose={onClose}>
      <div className="space-y-2 p-5 text-[11px] text-muted">
        <p>{`@${t(`studio.gui.dev_tree.description.${entry.descriptionId}`)}`}</p>
        <p>{t('studio.gui.dev_tree.info_only')}</p>
      </div>
    </PreviewDialog>
  )
}

function PreviewDialog({ title, subtitle, children, onClose }: { title: string, subtitle: string, children: React.ReactNode, onClose: () => void }) {
  const { t } = useTranslation()

  function closeDialog(open: boolean) {
    if (!open)
      onClose()
  }

  return (
    <Modal isOpen onOpenChange={closeDialog}>
      <Modal.Backdrop>
        <Modal.Container size="lg">
          <Modal.Dialog className="max-h-[82vh] w-[min(840px,92vw)] overflow-hidden">
            <Modal.CloseTrigger />
            <Modal.Header>
              <div className="min-w-0 flex-1">
                <Modal.Heading>{title}</Modal.Heading>
                <span className="mt-1 block truncate text-[10px] text-muted">{subtitle}</span>
              </div>
            </Modal.Header>
            <Modal.Body className="min-h-0 overflow-auto p-0">{children}</Modal.Body>
            <Modal.Footer>
              <Button variant="ghost" onPress={onClose}>{t('studio.gui.dev_tree.close')}</Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}

function buildRows(
  directories: Record<string, DirectoryState>,
  expanded: Record<string, boolean>,
  search: string,
  newPaths: string[],
): TreeRow[] {
  const query = search.trim().toLocaleLowerCase()
  if (query) {
    const entries = new Map<string, GuiDevTreeEntry>()
    Object.values(directories).forEach((directory) => {
      directory.entries.forEach(entry => entries.set(entry.path, entry))
    })
    newPaths.forEach(path => entries.set(path, virtualFileEntry(path)))
    return [...entries.values()]
      .filter(entry => entry.path.toLocaleLowerCase().includes(query))
      .sort((left, right) => left.path.toLocaleLowerCase().localeCompare(right.path.toLocaleLowerCase()))
      .map(entry => ({ id: `search:${entry.path}`, depth: 0, rowType: 'entry', entry, isNew: newPaths.includes(entry.path) }))
  }

  const rows: TreeRow[] = newPaths.map(path => ({
    id: `new:${path}`,
    depth: 0,
    rowType: 'entry',
    entry: virtualFileEntry(path),
    isNew: true,
  }))
  appendDirectoryRows(rows, '', 0, directories, expanded)
  return rows
}

function appendDirectoryRows(
  rows: TreeRow[],
  parentPath: string,
  depth: number,
  directories: Record<string, DirectoryState>,
  expanded: Record<string, boolean>,
) {
  const state = directories[parentPath]
  if (!state) {
    rows.push({ id: `loading:${parentPath}`, depth, rowType: 'loading' })
    return
  }
  state.entries.forEach((entry) => {
    rows.push({ id: `entry:${entry.path}`, depth, rowType: 'entry', entry })
    if (entry.entryType === 'directory' && expanded[entry.path])
      appendDirectoryRows(rows, entry.path, depth + 1, directories, expanded)
  })
  if (state.loading)
    rows.push({ id: `loading:${parentPath}`, depth, rowType: 'loading' })
  if (state.failed)
    rows.push({ id: `error:${parentPath}`, depth, rowType: 'error' })
  if (state.nextCursor && !state.loading) {
    rows.push({
      id: `more:${parentPath}:${state.nextCursor}`,
      depth,
      rowType: 'more',
      parentPath,
      cursor: state.nextCursor,
    })
  }
}

function virtualFileEntry(path: string): GuiDevTreeEntry {
  const segments = path.split('/')
  return {
    path,
    name: segments.at(-1) ?? path,
    entryType: 'file',
    policy: 'editable',
    hidden: false,
    size: 0,
    hasChildren: false,
    descriptionId: 'GUIExport',
  }
}

function emptyDirectoryState(loading: boolean): DirectoryState {
  return { entries: [], loading, loaded: false, failed: false }
}

function failedDirectoryState(): DirectoryState {
  return { entries: [], loading: false, loaded: false, failed: true }
}

function EntryIcon({ entry }: { entry: GuiDevTreeEntry }) {
  if (entry.entryType === 'directory')
    return <FolderOpen className={entryIconClass(entry.policy)} />
  if (entry.policy === 'asset')
    return <Picture className={entryIconClass(entry.policy)} />
  return <FileCode className={entryIconClass(entry.policy)} />
}

function entryIconClass(policy: GuiDevTreeEntry['policy']): string {
  if (policy === 'editable')
    return 'size-3.5 shrink-0 text-accent'
  if (policy === 'asset')
    return 'size-3.5 shrink-0 text-warning'
  return 'size-3.5 shrink-0 text-muted'
}

function treeRowClass(active: boolean): string {
  const base = 'absolute left-0 flex w-full items-center gap-1.5 pr-2 hover:bg-panel-hover'
  if (active)
    return `${base} bg-accent/12`
  return base
}

function treeChevronClass(open: boolean | undefined): string {
  if (open)
    return 'size-3 shrink-0 rotate-90 text-muted transition-transform'
  return 'size-3 shrink-0 text-muted transition-transform'
}

function fileSecondaryLabel(entry: GuiDevTreeEntry, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (entry.policy === 'editable')
    return t('studio.gui.dev_tree.policy.editable')
  if (entry.policy === 'readonly')
    return t('studio.gui.dev_tree.policy.readonly')
  if (entry.policy === 'asset')
    return t('studio.gui.dev_tree.policy.asset', { size: formatBytes(entry.size) })
  return t('studio.gui.dev_tree.policy.info')
}

function formatBytes(bytes: number): string {
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(bytes)
}
