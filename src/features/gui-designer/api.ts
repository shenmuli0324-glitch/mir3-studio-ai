import type {
  GuiDesignerStatus,
  GuiDocumentEntry,
  GuiDocumentOpenResult,
  GuiDraftApplyResult,
  GuiDraftChangeSet,
  GuiDraftConfirmation,
  GuiDraftPrepareResult,
  GuiTemplateRequest,
  GuiTemplateResult,
  Mir3UiDocument,
  Mir3UiNode,
  SourceSpan,
} from './types'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

export function useGuiDesignerStatus(projectId?: string) {
  return useQuery({
    queryKey: ['gui-designer-status', projectId],
    queryFn: () => invoke<GuiDesignerStatus>('gui_designer_status', { projectId }),
    enabled: projectId != null,
  })
}

export function useGuiDocumentList(projectId?: string) {
  return useQuery({
    queryKey: ['gui-document-list', projectId],
    queryFn: () => invoke<unknown>('gui_document_list', { projectId }).then(normalizeDocumentList),
    enabled: projectId != null,
  })
}

export function useGuiAsset(projectId: string | undefined, logicalPath: string | undefined) {
  return useQuery({
    queryKey: ['gui-asset', projectId, logicalPath],
    queryFn: () => invoke<{ logicalPath: string, mimeType: string, base64: string, sha256: string }>('gui_asset_read', { projectId, logicalPath }),
    enabled: projectId != null && logicalPath != null && logicalPath.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
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

function normalizeDocument(wire: WireDocument): Mir3UiDocument {
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
    visible: boundValue<boolean>(wire.visible, true),
    paint: {
      text: boundValue<string>(wire.text, ''),
      image: boundValue<string>(wire.image, ''),
      normalImage: boundValue<string>(wire.image ?? wire.normalImage, ''),
      fontSize: boundValue<number>(wire.fontSize, 14),
      color: boundValue<string>(wire.color, '#ffffff'),
      opacity: boundValue<number>(wire.opacity, 255),
    },
    compatibility: compatibilityValue(compatibility?.status ?? wire.compatibility),
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
  if (value === 'Panel' || value === 'Image' || value === 'Text' || value === 'Button' || value === 'Node')
    return value
  return 'Unsupported'
}

function compatibilityValue(input: unknown): Mir3UiNode['compatibility'] {
  const value = String(input ?? 'unsupported').toLowerCase()
  if (value === 'supported' || value === 'partial')
    return value
  return 'unsupported'
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
