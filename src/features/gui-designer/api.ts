import type {
  GuiAiWorkspace,
  GuiAssetMeta,
  GuiDesignerStatus,
  GuiDevTreePage,
  GuiDocumentEntry,
  GuiDocumentOpenResult,
  GuiDocumentProbeResult,
  GuiGameProcessStatus,
  GuiReadonlyDocument,
  GuiSaveNode,
  GuiTemplateRequest,
  GuiTemplateResult,
  GuiWorkingSaveChangeSet,
  GuiWorkingSaveResult,
  Mir3UiDocument,
  Mir3UiNode,
  SourceSpan,
} from './types'
import { queryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

let assetDecodingPaused = false
const assetDecodeWaiters = new Set<() => void>()

export function setGuiAssetDecodingPaused(paused: boolean): void {
  assetDecodingPaused = paused
  if (paused)
    return
  for (const resume of assetDecodeWaiters)
    resume()
  assetDecodeWaiters.clear()
}

export function useGuiDesignerStatus(projectId?: string, active = true) {
  return useQuery({
    queryKey: ['gui-designer-status', projectId],
    queryFn: () => invoke<GuiDesignerStatus>('gui_designer_status', { projectId }),
    enabled: active && projectId != null,
  })
}

export function guiDevTreeQueryOptions(projectId: string, parentPath: string, cursor?: string | null) {
  return queryOptions({
    queryKey: ['gui-dev-tree', projectId, parentPath, cursor ?? null],
    queryFn: () => invoke<GuiDevTreePage>('gui_dev_tree_list', { projectId, parentPath, cursor }),
    staleTime: 30_000,
  })
}

export function guiReadonlyDocumentQueryOptions(projectId: string, devRelativePath: string) {
  return queryOptions({
    queryKey: ['gui-readonly-document', projectId, devRelativePath],
    queryFn: () => invoke<GuiReadonlyDocument>('gui_readonly_document_open', { projectId, devRelativePath }),
    staleTime: Number.POSITIVE_INFINITY,
  })
}

export function guiAssetMetaQueryOptions(projectId: string, logicalPath: string) {
  return queryOptions({
    queryKey: ['gui-asset-meta', projectId, logicalPath],
    queryFn: () => invoke<GuiAssetMeta>('gui_asset_meta', { projectId, logicalPath }),
    staleTime: Number.POSITIVE_INFINITY,
  })
}

export function useGuiDocumentList(projectId?: string, active = true) {
  return useQuery({
    queryKey: ['gui-document-list', projectId],
    queryFn: () => invoke<unknown>('gui_document_list', { projectId }).then(normalizeDocumentList),
    enabled: active && projectId != null,
  })
}

export interface GuiAssetBinary {
  logicalPath: string
  mimeType: string
  blob: Blob
  sha256: string
  width?: number
  height?: number
}

export function guiAssetQueryOptions(projectId: string, logicalPath: string) {
  return queryOptions({
    queryKey: ['gui-asset', projectId, logicalPath],
    queryFn: () => invoke<unknown>('gui_asset_read', { projectId, logicalPath })
      .then(payload => normalizeGuiAssetPayload(payload, logicalPath))
      .then(readGuiAssetDimensions),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export async function normalizeGuiAssetPayload(payload: unknown, logicalPath: string): Promise<GuiAssetBinary> {
  const legacy = isRecord(payload) ? payload : undefined
  const mimeType = stringValue(legacy?.mimeType) ?? inferAssetMimeType(logicalPath)
  const bytes = assetBytes(payload)
  const blob = new Blob([bytes], { type: mimeType })
  return {
    logicalPath: stringValue(legacy?.logicalPath) ?? logicalPath,
    mimeType,
    blob,
    sha256: stringValue(legacy?.sha256) ?? `${logicalPath}:${bytes.byteLength}`,
  }
}

export function useGuiDocumentActions(projectId?: string) {
  const queryClient = useQueryClient()
  const open = useMutation({
    mutationFn: ({ devRelativePath }: { devRelativePath: string }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_document_open', { projectId, devRelativePath }).then(normalizeOpenResult)
    },
  })
  const reparse = useMutation({
    mutationFn: ({ devRelativePath, workingSource, expectedSha256 }: { devRelativePath: string, workingSource: string, expectedSha256?: string | null }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_document_reparse', { projectId, request: { devRelativePath, workingSource, expectedSha256 } }).then(normalizeOpenResult)
    },
  })
  const template = useMutation({
    mutationFn: (request: GuiTemplateRequest) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      const wireRequest = { path: request.relativePath, platform: request.targets, pcResolution: request.pcResolution }
      return invoke<unknown>('gui_document_template', { projectId, request: wireRequest }).then(normalizeTemplateResult)
    },
  })
  const save = useMutation({
    mutationFn: (changeSet: GuiWorkingSaveChangeSet) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_working_save', { projectId, changeSet }).then(normalizeWorkingSaveResult)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['gui-document-list', projectId] })
    },
  })
  const restore = useMutation({
    mutationFn: (nodeId: string) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_save_node_restore', { projectId, nodeId }).then(normalizeWorkingSaveResult)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['gui-document-list', projectId] })
    },
  })
  const acceptExternal = useMutation({
    mutationFn: (request: {
      devRelativePath: string
      previousSource: string
      previousSha256: string
      previousEncoding?: string | null
      previousNewline?: string | null
    }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_external_change_record', { projectId, request }).then(normalizeWorkingSaveResult)
    },
  })

  function listSaveNodes(limit = 100): Promise<GuiSaveNode[]> {
    if (!projectId)
      return Promise.reject(new Error('GUI_PROJECT_REQUIRED'))
    return invoke<unknown>('gui_save_node_list', { projectId, limit }).then(normalizeSaveNodes)
  }

  function probeDocument(request: {
    devRelativePath: string
    knownSha256?: string | null
    knownModifiedAt?: number | null
    knownSize?: number | null
    forceHash?: boolean
  }): Promise<GuiDocumentProbeResult> {
    if (!projectId)
      return Promise.reject(new Error('GUI_PROJECT_REQUIRED'))
    return invoke<unknown>('gui_document_probe', { projectId, ...request }).then(normalizeDocumentProbe)
  }

  function syncAiWorkspace(context: {
    path: string
    workingRevision: number
    baseSha256: string
    selectedNodeId?: string | null
    dirty: boolean
    source: string
  }): Promise<GuiAiWorkspace> {
    if (!projectId)
      return Promise.reject(new Error('GUI_PROJECT_REQUIRED'))
    return invoke<unknown>('gui_ai_workspace_sync', { projectId, context }).then(normalizeAiWorkspace)
  }

  function getAiWorkspace(path: string): Promise<GuiAiWorkspace> {
    if (!projectId)
      return Promise.reject(new Error('GUI_PROJECT_REQUIRED'))
    return invoke<unknown>('gui_ai_workspace_get', { projectId, path }).then(normalizeAiWorkspace)
  }

  function gameProcessStatus(): Promise<GuiGameProcessStatus> {
    if (!projectId)
      return Promise.reject(new Error('GUI_PROJECT_REQUIRED'))
    return invoke<unknown>('gui_game_process_status', { projectId }).then(normalizeGameProcessStatus)
  }

  return {
    open: open.mutateAsync,
    reparse: reparse.mutateAsync,
    createTemplate: template.mutateAsync,
    saveWorking: save.mutateAsync,
    restoreSaveNode: restore.mutateAsync,
    acceptExternalChange: acceptExternal.mutateAsync,
    listSaveNodes,
    probeDocument,
    syncAiWorkspace,
    getAiWorkspace,
    gameProcessStatus,
    busy: open.isPending || reparse.isPending || template.isPending || save.isPending || restore.isPending || acceptExternal.isPending,
    error: open.error || reparse.error || template.error || save.error || restore.error || acceptExternal.error,
  }
}

function normalizeWorkingSaveResult(input: unknown): GuiWorkingSaveResult {
  const wire = isRecord(input) ? input : {}
  const files = Array.isArray(wire.files) ? wire.files : wire.file ? [wire.file] : []
  return {
    files: files.map((item) => {
      const value = isRecord(item) ? item : {}
      const result = normalizeOpenResult(value)
      return {
        ...result,
        path: stringValue(value.path ?? value.devRelativePath) ?? result.document.devRelativePath,
      }
    }),
    saveNode: normalizeSaveNode(wire.saveNode ?? wire.save_node),
  }
}

function normalizeSaveNodes(input: unknown): GuiSaveNode[] {
  const wire = isRecord(input) ? input : {}
  const nodes = Array.isArray(input) ? input : Array.isArray(wire.nodes) ? wire.nodes : []
  return nodes.map(normalizeSaveNode)
}

function normalizeSaveNode(input: unknown): GuiSaveNode {
  const wire = isRecord(input) ? input : {}
  const rawOrigin = wire.origin ?? wire.source
  const origin = rawOrigin === 'external' || rawOrigin === 'restore' ? rawOrigin : 'studio'
  const nodeFiles = Array.isArray(wire.files) ? wire.files : []
  const paths = Array.isArray(wire.paths)
    ? wire.paths.map(String)
    : nodeFiles.map(file => String(isRecord(file) ? file.path ?? '' : '')).filter(Boolean)
  return {
    id: String(wire.id ?? wire.nodeId ?? wire.node_id ?? ''),
    previousNodeId: stringValue(wire.previousNodeId ?? wire.previous_node_id),
    restoredFromNodeId: stringValue(wire.restoredFromNodeId ?? wire.restored_from_node_id),
    createdAt: numberValue(wire.createdAt ?? wire.created_at) ?? Date.now(),
    origin,
    paths,
  }
}

function normalizeDocumentProbe(input: unknown): GuiDocumentProbeResult {
  const wire = isRecord(input) ? input : {}
  const fallbackState = wire.exists === false ? 'missing' : wire.changed === true ? 'changed' : 'unchanged'
  const rawState = String(wire.state ?? wire.status ?? fallbackState)
  const state = rawState === 'changed' || rawState === 'missing' ? rawState : 'unchanged'
  return {
    state,
    sha256: stringValue(wire.sha256),
    byteLength: numberValue(wire.byteLength ?? wire.byte_length) ?? 0,
    modifiedAt: numberValue(wire.modifiedAt ?? wire.modified_at),
  }
}

function normalizeAiWorkspace(input: unknown): GuiAiWorkspace {
  const wire = isRecord(input) ? input : {}
  const document = isRecord(wire.document) ? wire.document : {}
  const diagnostics = Array.isArray(wire.diagnostics)
    ? wire.diagnostics
    : Array.isArray(document.diagnostics)
      ? document.diagnostics
      : []
  const normalizedDiagnostics = diagnostics.map((diagnostic) => {
    const value = isRecord(diagnostic) ? diagnostic : {}
    return {
      code: String(value.code ?? 'GUI_DIAGNOSTIC'),
      severity: severityValue(value.severity),
      message: String(value.message ?? ''),
      span: spanValue(value.span),
      nodeId: stringValue(value.nodeId ?? value.node_id),
    }
  })
  return {
    workspaceId: String(wire.workspaceId ?? wire.workspace_id ?? ''),
    workspaceToken: String(wire.workspaceToken ?? wire.workspace_token ?? ''),
    path: String(wire.path ?? wire.devRelativePath ?? ''),
    source: String(wire.source ?? ''),
    baseSha256: stringValue(wire.baseSha256 ?? wire.base_sha256),
    workingRevision: numberValue(wire.workingRevision ?? wire.working_revision ?? wire.revision) ?? 0,
    valid: wire.valid !== false && !normalizedDiagnostics.some(diagnostic => diagnostic.severity === 'error'),
    diagnostics: normalizedDiagnostics,
  }
}

function normalizeGameProcessStatus(input: unknown): GuiGameProcessStatus {
  const wire = isRecord(input) ? input : {}
  const executablePath = String(wire.executablePath ?? wire.executable_path ?? '')
  return {
    supported: wire.supported !== false,
    executablePath,
    configured: wire.configured === true || executablePath.length > 0,
    running: wire.running === true,
  }
}

interface WireDocument {
  schemaVersion?: number | string
  source?: { devRelativePath?: string, sha256?: string, encoding?: string, newline?: string }
  projectId?: string
  devRelativePath?: string
  sourceSha256?: string
  encoding?: string
  newline?: string
  viewport?: { width: number, height: number }
  roots?: string[]
  nodes?: unknown[] | Record<string, unknown>
  assets?: unknown[]
  diagnostics?: unknown[]
}

function normalizeOpenResult(input: unknown): GuiDocumentOpenResult {
  const wire = input as Record<string, unknown>
  const document = normalizeDocument((wire.document ?? wire) as WireDocument)
  return {
    source: String(wire.sourceText ?? wire.source ?? ''),
    document,
    sha256: stringValue(wire.sha256) ?? document.sourceSha256,
    encoding: stringValue(wire.encoding) ?? document.encoding,
    newline: stringValue(wire.newline) ?? document.newline,
  }
}

function normalizeDocumentList(input: unknown): GuiDocumentEntry[] {
  const wire = input as { entries?: unknown[] }
  const entries = Array.isArray(input) ? input : wire.entries ?? []
  return entries.map((item) => {
    const value = item as Record<string, unknown>
    return {
      path: String(value.path ?? value.devRelativePath ?? ''),
      kind: value.kind === 'readonly' ? 'readonly' : 'editable',
      platform: platformValue(value.platform),
      peerPath: stringValue(value.peerPath),
    }
  })
}

function normalizeTemplateResult(input: unknown): GuiTemplateResult {
  const wire = input as Record<string, unknown>
  if (Array.isArray(wire.documents)) {
    return {
      documents: wire.documents.map((item) => {
        const result = normalizeOpenResult(item)
        const record = item as Record<string, unknown>
        return { path: stringValue(record.path) ?? result.document.devRelativePath, source: result.source, document: result.document }
      }),
    }
  }
  const result = normalizeOpenResult(input)
  return { documents: [{ path: result.document.devRelativePath, source: result.source, document: result.document }] }
}

export function normalizeDocument(wire: WireDocument): Mir3UiDocument {
  const source = wire.source
  const rawNodes = Array.isArray(wire.nodes) ? wire.nodes : Object.values(wire.nodes ?? {})
  const nodes = rawNodes.map(normalizeNode)
  return {
    schemaVersion: String(wire.schemaVersion ?? 1),
    projectId: wire.projectId ?? '',
    devRelativePath: source?.devRelativePath ?? wire.devRelativePath ?? '',
    sourceSha256: source?.sha256 ?? wire.sourceSha256,
    encoding: source?.encoding ?? wire.encoding,
    newline: source?.newline ?? wire.newline,
    viewport: wire.viewport,
    roots: wire.roots ?? [],
    nodes: Object.fromEntries(nodes.map(node => [node.id, node])),
    assets: (wire.assets ?? []).map((asset) => {
      const value = asset as Record<string, unknown>
      return { logicalPath: String(value.logicalPath ?? ''), available: value.available !== false }
    }),
    diagnostics: (wire.diagnostics ?? []).map((diagnostic) => {
      const value = diagnostic as Record<string, unknown>
      return {
        code: String(value.code ?? 'GUI_DIAGNOSTIC'),
        severity: severityValue(value.severity),
        message: String(value.message ?? ''),
        span: spanValue(value.span),
        nodeId: stringValue(value.nodeId),
      }
    }),
  }
}

function normalizeNode(input: unknown): Mir3UiNode {
  const wire = input as Record<string, unknown>
  const compatibility = wire.compatibility as Record<string, unknown> | undefined
  const sourceBinding = wire.sourceBinding as Record<string, unknown> | undefined
  const transform = wire.transform as Record<string, unknown> | undefined
  const scale9 = wire.scale9 as Record<string, unknown> | undefined
  const container = wire.container as Record<string, unknown> | undefined
  const insertByte = numberValue(sourceBinding?.insertByte) ?? 0
  const zeroSpan: SourceSpan | null = insertByte > 0 ? { startByte: insertByte, endByte: insertByte } : null
  return {
    id: String(wire.id ?? ''),
    kind: nodeKindValue(wire.nodeType ?? wire.kind),
    parentId: stringValue(wire.parentId),
    children: arrayStrings(wire.children ?? wire.childIds),
    luaVariable: stringValue(wire.luaVariable),
    name: boundValue<string>(wire.name, ''),
    tag: boundValue<number>(wire.tag, 0),
    position: pointValue(wire.position),
    size: sizeValue(wire.size),
    anchor: pointValue(wire.anchor),
    transform: {
      scaleX: boundValue<number>(transform?.scaleX ?? wire.scaleX, 1),
      scaleY: boundValue<number>(transform?.scaleY ?? wire.scaleY, 1),
      rotation: boundValue<number>(transform?.rotation ?? wire.rotation, 0),
      skewX: boundValue<number>(transform?.skewX ?? wire.skewX, 0),
      skewY: boundValue<number>(transform?.skewY ?? wire.skewY, 0),
    },
    visible: boundValue<boolean>(wire.visible, true),
    ignoreContentAdaptWithSize: boundValue<boolean>(wire.ignoreContentAdaptWithSize, true),
    clippingEnabled: boundValue<boolean>(wire.clippingEnabled, false),
    scale9: {
      enabled: boundValue<boolean>(scale9?.enabled, false),
      left: boundValue<number>(scale9?.left, 0),
      bottom: boundValue<number>(scale9?.bottom, 0),
      right: boundValue<number>(scale9?.right, 0),
      top: boundValue<number>(scale9?.top, 0),
    },
    container: {
      direction: boundValue<number>(container?.direction, 1),
      gravity: boundValue<number>(container?.gravity, 0),
      itemsMargin: boundValue<number>(container?.itemsMargin, 0),
      innerWidth: boundValue<number>(container?.innerWidth, 0),
      innerHeight: boundValue<number>(container?.innerHeight, 0),
    },
    properties: propertyValues(wire.properties),
    assetSlots: stringBoundValues(wire.assetSlots),
    paint: {
      text: boundValue<string>(wire.text, ''),
      image: boundValue<string>(wire.image, ''),
      normalImage: boundValue<string>(wire.image ?? wire.normalImage, ''),
      pressedImage: boundValue<string>(wire.pressedImage, ''),
      disabledImage: boundValue<string>(wire.disabledImage, ''),
      fontSize: boundValue<number>(wire.fontSize, 14),
      color: boundValue<string>(wire.color, '#ffffff'),
      opacity: boundValue<number>(wire.opacity, 255),
    },
    compatibility: compatibilityValue(compatibility?.status ?? wire.compatibility),
    compatibilityReasonCode: stringValue(compatibility?.reasonCode),
    compatibilityReason: stringValue(compatibility?.reason),
    binding: {
      createCall: spanValue(sourceBinding?.createCall),
      statement: spanValue(sourceBinding?.statement),
      properties: spansValue(sourceBinding?.propertySpans),
      safeInsertion: zeroSpan,
    },
  }
}

function boundValue<T>(input: unknown, fallback: T) {
  const wire = input as Record<string, unknown> | undefined
  return {
    value: (wire?.value ?? fallback) as T,
    source: sourceValue(wire?.source ?? wire?.origin),
    writable: wire?.writable === true,
    rawToken: stringValue(wire?.originalToken ?? wire?.raw),
    span: spanValue(wire?.span),
  }
}

function pointValue(input: unknown) {
  const wire = input as Record<string, unknown> | undefined
  return { x: boundValue<number>(wire?.x, 0), y: boundValue<number>(wire?.y, 0) }
}

function sizeValue(input: unknown) {
  const wire = input as Record<string, unknown> | undefined
  return { width: boundValue<number>(wire?.width, 0), height: boundValue<number>(wire?.height, 0) }
}

function spanValue(input: unknown): SourceSpan | null {
  if (!input || typeof input !== 'object')
    return null
  const wire = input as Record<string, unknown>
  const start = wire.start as Record<string, unknown> | undefined
  const end = wire.end as Record<string, unknown> | undefined
  return {
    startByte: numberValue(wire.startByte) ?? 0,
    endByte: numberValue(wire.endByte) ?? 0,
    startLine: numberValue(start?.row),
    startColumn: numberValue(start?.column),
    endLine: numberValue(end?.row),
    endColumn: numberValue(end?.column),
  }
}

function spansValue(input: unknown): Record<string, SourceSpan> {
  if (!input || typeof input !== 'object')
    return {}
  return Object.fromEntries(Object.entries(input).flatMap(([key, value]) => {
    const span = spanValue(value)
    return span ? [[key, span]] : []
  }))
}

function nodeKindValue(input: unknown): Mir3UiNode['kind'] {
  const value = String(input ?? 'Unsupported')
  const kinds: Mir3UiNode['kind'][] = [
    'Panel',
    'Image',
    'Text',
    'Button',
    'Node',
    'TextAtlas',
    'RichText',
    'ScrollText',
    'ItemShow',
    'CheckBox',
    'TextInput',
    'Slider',
    'ProgressTimer',
    'LoadingBar',
    'Effect',
    'UIModel',
    'SpineAnim',
    'PageView',
    'ListView',
    'ScrollView',
    'QuickCell',
    'MenuItem',
    'TableView',
  ]
  const kind = kinds.find(item => item === value)
  if (kind)
    return kind
  return 'Unsupported'
}

function propertyValues(input: unknown): Mir3UiNode['properties'] {
  if (!isRecord(input))
    return {}
  return Object.fromEntries(Object.entries(input).map(([key, value]) => [key, boundValue(value, null)]))
}

function stringBoundValues(input: unknown): Record<string, import('./types').BoundValue<string>> {
  if (!isRecord(input))
    return {}
  return Object.fromEntries(Object.entries(input).map(([key, value]) => [key, boundValue(value, '')]))
}

function compatibilityValue(input: unknown): Mir3UiNode['compatibility'] {
  const value = String(input ?? 'unknown').toLowerCase()
  if (value === 'supported' || value === 'approximate' || value === 'dynamic')
    return value
  return 'unknown'
}

function sourceValue(input: unknown): 'literal' | 'default' | 'dynamic' {
  const value = String(input ?? 'default').toLowerCase()
  if (value === 'literal' || value === 'dynamic')
    return value
  return 'default'
}

function severityValue(input: unknown): 'info' | 'warning' | 'error' {
  const value = String(input ?? 'info').toLowerCase()
  if (value === 'warning' || value === 'error')
    return value
  return 'info'
}

function stringValue(input: unknown): string | undefined {
  return typeof input === 'string' ? input : undefined
}

function numberValue(input: unknown): number | undefined {
  return typeof input === 'number' ? input : undefined
}

function arrayStrings(input: unknown): string[] {
  return Array.isArray(input) ? input.map(String) : []
}

function platformValue(input: unknown): GuiDocumentEntry['platform'] {
  const value = String(input ?? 'shared').toLowerCase()
  if (value === 'mobile' || value === 'pc')
    return value
  return 'shared'
}

async function readGuiAssetDimensions(asset: GuiAssetBinary): Promise<GuiAssetBinary> {
  await waitForAssetDecoding()
  if (typeof createImageBitmap === 'function') {
    try {
      const bitmap = await createImageBitmap(asset.blob)
      const dimensions = { width: bitmap.width, height: bitmap.height }
      bitmap.close()
      return { ...asset, ...dimensions }
    }
    catch {
      // WebView不支持该容器时继续使用Image解码，不让素材失败阻断画布。
    }
  }
  return readGuiAssetDimensionsWithImage(asset)
}

async function waitForAssetDecoding(): Promise<void> {
  if (!assetDecodingPaused)
    return
  await new Promise<void>((resolve) => {
    assetDecodeWaiters.add(resolve)
  })
}

async function readGuiAssetDimensionsWithImage(asset: GuiAssetBinary): Promise<GuiAssetBinary> {
  if (typeof Image !== 'function' || typeof URL.createObjectURL !== 'function')
    return asset
  const url = URL.createObjectURL(asset.blob)
  try {
    const dimensions = await new Promise<{ width: number, height: number }>((resolve, reject) => {
      const image = new Image()
      image.onload = () => resolve({ width: image.naturalWidth, height: image.naturalHeight })
      image.onerror = reject
      image.src = url
    })
    return { ...asset, ...dimensions }
  }
  catch {
    return asset
  }
  finally {
    URL.revokeObjectURL(url)
  }
}

function assetBytes(payload: unknown): Uint8Array {
  if (payload instanceof ArrayBuffer)
    return new Uint8Array(payload)
  if (ArrayBuffer.isView(payload))
    return new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength)
  if (Array.isArray(payload))
    return Uint8Array.from(payload.map(Number))
  if (isRecord(payload)) {
    if (typeof payload.base64 === 'string')
      return decodeBase64(payload.base64)
    if (Array.isArray(payload.bytes))
      return Uint8Array.from(payload.bytes.map(Number))
    if (Array.isArray(payload.data))
      return Uint8Array.from(payload.data.map(Number))
  }
  throw new Error('GUI_ASSET_PAYLOAD_INVALID')
}

function decodeBase64(value: string): Uint8Array {
  const decoded = globalThis.atob(value)
  const bytes = new Uint8Array(decoded.length)
  for (let index = 0; index < decoded.length; index += 1)
    bytes[index] = decoded.charCodeAt(index)
  return bytes
}

function inferAssetMimeType(logicalPath: string): string {
  return /\.jpe?g$/i.test(logicalPath) ? 'image/jpeg' : 'image/png'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === 'object'
}
