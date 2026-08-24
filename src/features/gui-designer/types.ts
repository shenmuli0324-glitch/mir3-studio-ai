export type GuiDevice = 'mobile' | 'pc'
export type GuiMode = 'visual' | 'code' | 'split'
export type GuiLeftPanel = 'files' | 'layers' | 'components'
export type GuiNodeKind = 'Panel' | 'Image' | 'Text' | 'Button' | 'Node' | 'Unsupported'
export type GuiValueSource = 'literal' | 'default' | 'dynamic'

export interface SourceSpan {
  startByte: number
  endByte: number
  startLine?: number
  startColumn?: number
  endLine?: number
  endColumn?: number
}

export interface BoundValue<T> {
  value: T
  source: GuiValueSource
  writable: boolean
  rawToken?: string | null
  span?: SourceSpan | null
}

export interface GuiPoint {
  x: BoundValue<number>
  y: BoundValue<number>
}

export interface GuiSize {
  width: BoundValue<number>
  height: BoundValue<number>
}

export interface GuiPaint {
  text?: BoundValue<string> | null
  image?: BoundValue<string> | null
  normalImage?: BoundValue<string> | null
  fontSize?: BoundValue<number> | null
  color?: BoundValue<string> | null
  opacity?: BoundValue<number> | null
}

export interface GuiSourceBinding {
  createCall?: SourceSpan | null
  statement?: SourceSpan | null
  properties?: Record<string, SourceSpan>
  safeInsertion?: SourceSpan | null
}

export interface Mir3UiNode {
  id: string
  kind: GuiNodeKind
  parentId?: string | null
  children: string[]
  luaVariable?: string | null
  name?: BoundValue<string> | null
  tag?: BoundValue<number> | null
  position: GuiPoint
  size: GuiSize
  anchor?: GuiPoint | null
  visible?: BoundValue<boolean> | null
  paint?: GuiPaint | null
  compatibility: 'supported' | 'partial' | 'unsupported'
  binding?: GuiSourceBinding | null
}

export interface GuiDiagnostic {
  code: string
  severity: 'info' | 'warning' | 'error'
  message: string
  span?: SourceSpan | null
  nodeId?: string | null
}

export interface Mir3UiDocument {
  schemaVersion: string
  projectId: string
  devRelativePath: string
  sourceSha256?: string | null
  encoding?: string | null
  newline?: string | null
  viewport?: { width: number, height: number } | null
  roots: string[]
  nodes: Record<string, Mir3UiNode>
  assets?: Array<{ logicalPath: string, available: boolean }>
  diagnostics: GuiDiagnostic[]
}

export interface GuiDocumentEntry {
  path: string
  kind: 'editable' | 'readonly'
  platform: 'mobile' | 'pc' | 'shared'
  peerPath?: string | null
}

export interface GuiDesignerStatus {
  available: boolean
  devRoot?: string | null
  guiExportRoot?: string | null
  assetRoot?: string | null
  reason?: string | null
}

export interface GuiDocumentOpenResult {
  source: string
  document: Mir3UiDocument
  sha256?: string | null
  encoding?: string | null
  newline?: string | null
  draftId?: string | null
  revision?: number | null
}

export interface GuiTemplateRequest {
  relativePath: string
  targets: 'mobile' | 'pc' | 'both'
  pcResolution?: { width: number, height: number }
}

export interface GuiTemplateResult {
  documents: Array<{ path: string, source: string, document: Mir3UiDocument }>
}

export interface GuiDraftChangeSet {
  files: Array<{
    devRelativePath: string
    source: string
    expectedSha256?: string | null
    isNew?: boolean
  }>
  draftId?: string | null
  expectedRevision?: number
}

export interface GuiDraftPrepareResult {
  draftId: string
  revision: number
}

export interface GuiDraftConfirmation {
  draftId: string
  confirmationToken: string
  diff: string
  diffHash?: string | null
}

export interface GuiDraftApplyResult {
  id?: string
  snapshotId?: string
  appliedPaths?: string[]
}

export const MOBILE_VIEWPORT = { width: 1136, height: 640 } as const

export const PC_VIEWPORTS = [
  { width: 1920, height: 1080 },
  { width: 1600, height: 1024 },
  { width: 1600, height: 900 },
  { width: 1440, height: 900 },
  { width: 1366, height: 768 },
  { width: 1280, height: 800 },
  { width: 1280, height: 768 },
  { width: 1152, height: 864 },
  { width: 1024, height: 768 },
  { width: 800, height: 600 },
] as const

export function boundNumber(value: number): BoundValue<number> {
  return { value, source: 'default', writable: false }
}
