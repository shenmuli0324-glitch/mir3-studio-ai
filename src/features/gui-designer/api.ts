import type {
  BoundValue,
  GuiAssetMeta,
  GuiDesignerStatus,
  GuiDevTreePage,
  GuiDiagnostic,
  GuiDocumentEntry,
  GuiDocumentOpenResult,
  GuiDraftApplyResult,
  GuiDraftChangeSet,
  GuiDraftConfirmation,
  GuiDraftPrepareResult,
  GuiPropertyValue,
  GuiReadonlyDocument,
  GuiRuntimeCapabilities,
  GuiRuntimeDataSource,
  GuiRuntimeSceneCatalog,
  GuiRuntimeSceneComposition,
  GuiRuntimeScenePatch,
  GuiRuntimeSceneResult,
  GuiRuntimeStage,
  GuiRuntimeWindow,
  GuiTemplateRequest,
  GuiTemplateResult,
  Mir3RuntimeSceneDocument,
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

export function useGuiDesignerStatus(projectId?: string) {
  return useQuery({
    queryKey: ['gui-designer-status', projectId],
    queryFn: () => invoke<GuiDesignerStatus>('gui_designer_status', { projectId }),
    enabled: projectId != null,
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

export function useGuiDocumentList(projectId?: string) {
  return useQuery({
    queryKey: ['gui-document-list', projectId],
    queryFn: () => invoke<unknown>('gui_document_list', { projectId }).then(normalizeDocumentList),
    enabled: projectId != null,
  })
}

export function useGuiRuntimeCapabilities(projectId?: string) {
  return useQuery({
    queryKey: ['gui-runtime-capabilities', projectId],
    queryFn: () => invoke<unknown>('gui_runtime_capabilities', { projectId }).then(normalizeRuntimeCapabilities),
    enabled: projectId != null,
    retry: false,
  })
}

export function useGuiRuntimeCatalog(projectId?: string) {
  return useQuery({
    queryKey: ['gui-runtime-catalog', projectId],
    queryFn: () => invoke<unknown>('gui_runtime_catalog', { projectId }).then(normalizeRuntimeCatalog),
    enabled: projectId != null,
    retry: false,
  })
}

export function useGuiRuntimeActions(projectId?: string) {
  const queryClient = useQueryClient()
  const start = useMutation({
    mutationFn: (request: { sceneId?: string, presetId?: string, mockProfileId?: string, device: string, viewport: { width: number, height: number }, workingSources?: Record<string, string> }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_runtime_scene_start', { projectId, request }).then(normalizeRuntimeSceneResult)
    },
  })
  const event = useMutation({
    mutationFn: (request: { sessionId: string, nodeId: string, eventType: string, payload?: Record<string, unknown>, expectedSequence: number }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_runtime_scene_event', { projectId, ...request }).then(normalizeRuntimeSceneResult)
    },
  })
  const reload = useMutation({
    mutationFn: (request: { sessionId: string, workingSources: Record<string, string> }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_runtime_scene_reload', { projectId, ...request }).then(normalizeRuntimeSceneResult)
    },
  })
  const stop = useMutation({
    mutationFn: (sessionId: string) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<{ stopped: boolean }>('gui_runtime_scene_stop', { projectId, sessionId })
    },
  })
  const setDataSource = useMutation({
    mutationFn: (mode: GuiRuntimeDataSource) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_runtime_data_source_set', { projectId, mode }).then(normalizeRuntimeCapabilities)
    },
    onSuccess: (capabilities) => {
      queryClient.setQueryData(['gui-runtime-capabilities', projectId], capabilities)
    },
  })
  return {
    start: start.mutateAsync,
    event: event.mutateAsync,
    reload: reload.mutateAsync,
    stop: stop.mutateAsync,
    setDataSource: setDataSource.mutateAsync,
    busy: start.isPending || event.isPending || reload.isPending || stop.isPending || setDataSource.isPending,
    error: start.error || event.error || reload.error || stop.error || setDataSource.error,
  }
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
    mutationFn: ({ devRelativePath, draftId }: { devRelativePath: string, draftId?: string }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_document_open', { projectId, devRelativePath, draftId }).then(normalizeOpenResult)
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
  const prepare = useMutation({
    mutationFn: (changeSet: GuiDraftChangeSet) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<GuiDraftPrepareResult>('gui_draft_prepare', { projectId, changeSet })
    },
  })
  const confirm = useMutation({
    mutationFn: (draftId: string) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<unknown>('gui_draft_confirm', { projectId, draftId }).then(normalizeDraftConfirmation)
    },
  })
  const apply = useMutation({
    mutationFn: ({ draftId, token }: { draftId: string, token: string }) => {
      if (!projectId)
        throw new Error('GUI_PROJECT_REQUIRED')
      return invoke<GuiDraftApplyResult>('gui_draft_apply', { projectId, draftId, confirmationToken: token })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['gui-document-list', projectId] })
    },
  })

  return {
    open: open.mutateAsync,
    reparse: reparse.mutateAsync,
    createTemplate: template.mutateAsync,
    prepareDraft: prepare.mutateAsync,
    confirmDraft: confirm.mutateAsync,
    applyDraft: apply.mutateAsync,
    busy: open.isPending || reparse.isPending || template.isPending || prepare.isPending || confirm.isPending || apply.isPending,
    error: open.error || reparse.error || template.error || prepare.error || confirm.error || apply.error,
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
  provenance?: unknown[]
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
    draftId: stringValue(wire.draftId),
    revision: numberValue(wire.revision),
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

function normalizeDraftConfirmation(input: unknown): GuiDraftConfirmation {
  const wire = input as Record<string, unknown>
  const preview = wire.preview as Record<string, unknown> | undefined
  const draft = preview?.draft as Record<string, unknown> | undefined
  const changes = Array.isArray(preview?.changes) ? preview.changes : []
  return {
    draftId: String(draft?.id ?? wire.draftId ?? ''),
    confirmationToken: String(wire.confirmationToken ?? ''),
    diff: changes.map((change) => {
      const value = change as Record<string, unknown>
      return String(value.unifiedDiff ?? '')
    }).filter(Boolean).join('\n'),
    diffHash: stringValue(preview?.diffHash ?? wire.diffHash),
  }
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
    provenance: normalizeProvenance(wire.provenance),
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
    sourceRef: isRecord(wire.sourceRef)
      ? {
          devRelativePath: String(wire.sourceRef.devRelativePath ?? ''),
          line: numberValue(wire.sourceRef.line),
          column: numberValue(wire.sourceRef.column),
          templateNodeId: stringValue(wire.sourceRef.templateNodeId),
        }
      : undefined,
  }
}

function normalizeRuntimeCapabilities(input: unknown): GuiRuntimeCapabilities {
  const wire = isRecord(input) ? input : {}
  const tables = Array.isArray(wire.tables) ? wire.tables : []
  return {
    available: wire.available === true,
    backend: wire.backend === 'sidecar' ? 'sidecar' : 'unavailable',
    dataSource: wire.dataSource === 'projectStatic' ? 'projectStatic' : 'builtInMock',
    projectStaticAvailable: wire.projectStaticAvailable === true,
    tables: tables.map((table) => {
      const value = isRecord(table) ? table : {}
      return { name: String(value.name ?? ''), available: value.available === true }
    }),
    limits: numericRecord(wire.limits),
    diagnostics: normalizeDiagnostics(wire.diagnostics),
  }
}

function normalizeRuntimeCatalog(input: unknown): GuiRuntimeSceneCatalog {
  const wire = isRecord(input) ? input : {}
  const legacyScenes = normalizeRuntimeCatalogEntries(wire.scenes)
  const presets = Array.isArray(wire.presets) ? normalizeRuntimeCatalogEntries(wire.presets) : legacyScenes
  return {
    presets,
    scenes: legacyScenes.length > 0 ? legacyScenes : presets,
  }
}

function normalizeRuntimeCatalogEntries(input: unknown): GuiRuntimeSceneCatalog['scenes'] {
  const entries = Array.isArray(input) ? input : []
  return entries.map((scene) => {
    const value = isRecord(scene) ? scene : {}
    return {
      id: String(value.id ?? value.presetId ?? ''),
      name: String(value.name ?? value.title ?? value.id ?? ''),
      category: String(value.category ?? 'general'),
      layoutPath: String(value.layoutPath ?? value.entryPath ?? ''),
      platform: runtimePlatform(value.platform ?? value.device),
      compatibility: compatibilityValue(value.compatibility),
      overlayIds: arrayStrings(value.overlayIds),
    }
  }).filter(scene => scene.id.length > 0)
}

function normalizeRuntimeSceneResult(input: unknown): GuiRuntimeSceneResult {
  const wire = isRecord(input) ? input : {}
  const scene = isRecord(wire.scene) ? normalizeRuntimeScene(wire.scene) : null
  return {
    sessionId: String(wire.sessionId ?? ''),
    sequence: numberValue(wire.sequence) ?? 0,
    scene,
    patch: isRecord(wire.patch) ? normalizeRuntimeScenePatch(wire.patch) : null,
    fallback: wire.fallback === true,
    diagnostics: normalizeDiagnostics(wire.diagnostics),
  }
}

function normalizeRuntimeScene(scene: Record<string, unknown>): Mir3RuntimeSceneDocument {
  const rawNodes = Array.isArray(scene.nodes)
    ? scene.nodes
    : isRecord(scene.nodes)
      ? Object.values(scene.nodes)
      : []
  if (rawNodes.some(node => isRecord(node) && isRecord(node.position))) {
    const document = normalizeDocument(scene as WireDocument)
    return { ...document, runtime: normalizeRuntimeComposition(scene, document) }
  }
  const runtimeNodes = rawNodes.map((node) => {
    const value = isRecord(node) ? node : {}
    const transform = isRecord(value.transform) ? value.transform : {}
    const size = isRecord(value.size) ? value.size : {}
    const properties = isRecord(value.properties) ? value.properties : {}
    const sourceRef = isRecord(value.sourceRef) ? value.sourceRef : {}
    const nodeKind = runtimeNodeKind(value.nodeType)
    const asset = String(value.asset ?? '')
    const assetSlots = runtimeAssetSlots(nodeKind, asset, value.assetSlots)
    const primaryAsset = assetSlots[runtimePrimaryAssetSlot(nodeKind)]?.value ?? asset
    return {
      id: String(value.id ?? ''),
      nodeType: nodeKind,
      parentId: stringValue(value.parentId),
      children: arrayStrings(value.children),
      luaVariable: null,
      name: runtimeBound(String(value.name ?? value.nodeType ?? 'RuntimeNode')),
      position: {
        x: runtimeBound(numberValue(transform.x) ?? 0),
        y: runtimeBound(numberValue(transform.y) ?? 0),
      },
      size: {
        width: runtimeBound(numberValue(size.width) ?? 0),
        height: runtimeBound(numberValue(size.height) ?? 0),
      },
      anchor: {
        x: runtimeBound(numberValue(transform.anchorX) ?? 0),
        y: runtimeBound(numberValue(transform.anchorY) ?? 0),
      },
      transform: {
        scaleX: runtimeBound(numberValue(transform.scaleX) ?? 1),
        scaleY: runtimeBound(numberValue(transform.scaleY) ?? 1),
        rotation: runtimeBound(numberValue(transform.rotation) ?? 0),
        skewX: runtimeBound(0),
        skewY: runtimeBound(0),
      },
      visible: runtimeBound(value.visible !== false),
      text: runtimeBound(String(value.text ?? '')),
      image: runtimeBound(primaryAsset),
      assetSlots,
      properties: Object.fromEntries(Object.entries(properties).map(([key, property]) => [key, runtimeBound(property as GuiPropertyValue)])),
      compatibility: { status: 'supported' },
      sourceRef: {
        devRelativePath: String(sourceRef.devRelativePath ?? ''),
        line: numberValue(sourceRef.line),
        column: numberValue(sourceRef.column),
        templateNodeId: stringValue(sourceRef.templateNodeId),
      },
      sourceBinding: sourceRef.line == null
        ? undefined
        : {
            statement: {
              startByte: 0,
              endByte: 0,
              start: { row: Math.max(0, (numberValue(sourceRef.line) ?? 1) - 1), column: numberValue(sourceRef.column) ?? 0 },
              end: { row: Math.max(0, (numberValue(sourceRef.line) ?? 1) - 1), column: numberValue(sourceRef.column) ?? 0 },
            },
          },
    }
  })
  const document = normalizeDocument({
    schemaVersion: String(scene.schemaVersion ?? 'runtime-1'),
    projectId: '',
    devRelativePath: runtimeSceneSourcePath(rawNodes),
    viewport: isRecord(scene.viewport)
      ? { width: numberValue(scene.viewport.width) ?? 1136, height: numberValue(scene.viewport.height) ?? 640 }
      : undefined,
    roots: arrayStrings(scene.roots),
    nodes: Object.fromEntries(runtimeNodes.map(node => [node.id, node])),
    diagnostics: Array.isArray(scene.diagnostics) ? scene.diagnostics : [],
    provenance: Array.isArray(scene.provenance) ? scene.provenance : [],
  })
  return { ...document, runtime: normalizeRuntimeComposition(scene, document) }
}

function normalizeRuntimeScenePatch(input: Record<string, unknown>): GuiRuntimeScenePatch {
  const upsertedNodes = normalizeRuntimePatchNodes(input.upsertedNodes)
  const updatedNodes = normalizeRuntimePatchNodes(input.updatedNodes)
  return {
    sequence: numberValue(input.sequence) ?? 0,
    addedNodes: normalizeRuntimePatchNodes(input.addedNodes),
    updatedNodes: mergeRuntimePatchNodes(upsertedNodes, updatedNodes),
    removedNodeIds: arrayStrings(input.removedNodeIds),
    roots: Array.isArray(input.roots) ? arrayStrings(input.roots) : undefined,
    stage: isRecord(input.stage) ? normalizeRuntimeStage(input.stage) : undefined,
    layers: Array.isArray(input.layers) ? normalizeRuntimeLayers(input.layers) : undefined,
    windows: Array.isArray(input.windows) ? normalizeRuntimeWindows(input.windows) : undefined,
    diagnostics: Array.isArray(input.diagnostics) ? normalizeDiagnostics(input.diagnostics) : undefined,
    provenance: Array.isArray(input.provenance) ? normalizeProvenance(input.provenance) : undefined,
  }
}

function mergeRuntimePatchNodes(base: Record<string, Mir3UiNode> | undefined, override: Record<string, Mir3UiNode> | undefined): Record<string, Mir3UiNode> | undefined {
  if (base == null)
    return override
  if (override == null)
    return base
  return { ...base, ...override }
}

function normalizeRuntimePatchNodes(input: unknown): Record<string, Mir3UiNode> | undefined {
  if (!isRecord(input))
    return undefined
  const nodes = Object.values(input)
  if (nodes.length === 0)
    return {}
  const document = normalizeRuntimeScene({ nodes: input, roots: [], diagnostics: [] })
  return document.nodes
}

function normalizeRuntimeComposition(scene: Record<string, unknown>, document: Mir3UiDocument): GuiRuntimeSceneComposition {
  const runtime = isRecord(scene.runtime) ? scene.runtime : scene
  const profileId = String(runtime.profileId ?? scene.profileId ?? 'hud-mobile')
  const device = runtime.device === 'pc' || profileId === 'game-pc' || profileId === 'hud-pc' ? 'pc' : 'mobile'
  return {
    profileId,
    device,
    stage: isRecord(runtime.stage) ? normalizeRuntimeStage(runtime.stage) : defaultRuntimeStage(profileId),
    layers: Array.isArray(runtime.layers)
      ? normalizeRuntimeLayers(runtime.layers)
      : [
          { id: 'stage', rootNodeIds: [], zOrder: 0 },
          { id: 'world', rootNodeIds: [], zOrder: 100 },
          { id: 'hud', rootNodeIds: document.roots, zOrder: 200 },
          { id: 'windows', rootNodeIds: [], zOrder: 300 },
        ],
    windows: Array.isArray(runtime.windows) ? normalizeRuntimeWindows(runtime.windows) : [],
  }
}

function normalizeRuntimeStage(input: Record<string, unknown>): GuiRuntimeStage {
  const kind = input.kind === 'login' || input.kind === 'snapshot' || input.kind === 'empty' ? input.kind : 'world'
  return {
    kind,
    backgroundAsset: stringValue(input.backgroundAsset),
    cameraX: numberValue(input.cameraX),
    cameraY: numberValue(input.cameraY),
    scale: numberValue(input.scale),
    compatibility: compatibilityValue(input.compatibility),
  }
}

function defaultRuntimeStage(profileId: string): GuiRuntimeStage {
  return {
    kind: profileId.startsWith('character-') || profileId.startsWith('role-') ? 'login' : 'world',
    compatibility: 'approximate',
  }
}

function normalizeRuntimeLayers(input: unknown[]): GuiRuntimeSceneComposition['layers'] {
  return input.map((entry, index) => {
    const value = isRecord(entry) ? entry : {}
    const rawId = String(value.id ?? 'hud')
    const id = rawId === 'stage' || rawId === 'world' || rawId === 'windows' ? rawId : 'hud'
    return { id, rootNodeIds: arrayStrings(value.rootNodeIds), zOrder: numberValue(value.zOrder) ?? index * 100 }
  })
}

function normalizeRuntimeWindows(input: unknown[]): GuiRuntimeWindow[] {
  return input.map((entry, index) => {
    const value = isRecord(entry) ? entry : {}
    const kind = value.kind === 'bag' || value.kind === 'team' || value.kind === 'store' ? value.kind : 'custom'
    return {
      id: String(value.id ?? `runtime-window-${index}`),
      kind,
      titleKey: stringValue(value.titleKey),
      layoutPath: stringValue(value.layoutPath),
      rootNodeIds: arrayStrings(value.rootNodeIds),
      modal: value.modal === true,
      zOrder: numberValue(value.zOrder) ?? 300 + index,
      source: value.source === 'localFallback' ? 'localFallback' : 'runtime',
    }
  })
}

function runtimeAssetSlots(kind: string, asset: string, input: unknown): Record<string, BoundValue<string>> {
  const slots = isRecord(input)
    ? Object.fromEntries(Object.entries(input).filter((entry): entry is [string, string] => typeof entry[1] === 'string' && entry[1].length > 0).map(([slot, value]) => [slot, runtimeBound(value)]))
    : {}
  if (Object.keys(slots).length > 0 || !asset)
    return slots
  return { [runtimePrimaryAssetSlot(kind)]: runtimeBound(asset) }
}

function runtimePrimaryAssetSlot(kind: string): string {
  if (kind === 'Panel' || kind === 'ListView' || kind === 'ScrollView' || kind === 'Slider')
    return 'background'
  if (kind === 'LoadingBar')
    return 'progress'
  if (kind === 'TextAtlas')
    return 'atlas'
  if (kind === 'SpineAnim')
    return 'json'
  return 'normal'
}

function runtimeBound<T>(value: T): { value: T, source: 'default', writable: false, originalToken: null, span: null } {
  return { value, source: 'default', writable: false, originalToken: null, span: null }
}

function runtimeNodeKind(input: unknown): string {
  const value = String(input ?? 'Unsupported')
  if (value === 'Layout')
    return 'Panel'
  if (value === 'Scene')
    return 'Node'
  return value
}

function runtimeSceneSourcePath(nodes: unknown[]): string {
  for (const node of nodes) {
    if (!isRecord(node) || !isRecord(node.sourceRef))
      continue
    const path = stringValue(node.sourceRef.devRelativePath)
    if (path)
      return path
  }
  return ''
}

function normalizeDiagnostics(input: unknown): GuiDiagnostic[] {
  if (!Array.isArray(input))
    return []
  return input.map((diagnostic) => {
    const value = isRecord(diagnostic) ? diagnostic : {}
    return {
      code: String(value.code ?? 'GUI_RUNTIME_DIAGNOSTIC'),
      severity: severityValue(value.severity),
      message: String(value.message ?? ''),
      span: spanValue(value.span),
      nodeId: stringValue(value.nodeId),
    }
  })
}

function normalizeProvenance(input: unknown): import('./types').GuiDataProvenance[] {
  if (!Array.isArray(input))
    return []
  return input.map((entry) => {
    const value = isRecord(entry) ? entry : {}
    return {
      kind: provenanceKind(value.kind),
      key: String(value.key ?? ''),
      description: String(value.description ?? ''),
    }
  })
}

function provenanceKind(input: unknown): import('./types').GuiDataProvenance['kind'] {
  const value = String(input ?? '').toLowerCase()
  if (value === 'staticconfig')
    return 'staticConfig'
  if (value === 'runtimederived')
    return 'runtimeDerived'
  if (value === 'missing')
    return 'missing'
  if (value === 'usersnapshot')
    return 'userSnapshot'
  return 'sceneMock'
}

function numericRecord(input: unknown): Record<string, number> {
  if (!isRecord(input))
    return {}
  return Object.fromEntries(Object.entries(input).flatMap(([key, value]) => {
    const number = numberValue(value)
    return number == null ? [] : [[key, number]]
  }))
}

function runtimePlatform(input: unknown): 'mobile' | 'pc' | 'shared' {
  if (input === 'mobile' || input === 'pc')
    return input
  return 'shared'
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
