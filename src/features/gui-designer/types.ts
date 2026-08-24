export type GuiDevice = 'mobile' | 'pc'
export type GuiMode = 'visual' | 'code' | 'split'
export type GuiLeftPanel = 'files' | 'scenes' | 'layers' | 'components'
export type GuiNodeKind
  = | 'Panel'
    | 'Image'
    | 'Text'
    | 'Button'
    | 'Node'
    | 'TextAtlas'
    | 'RichText'
    | 'ScrollText'
    | 'ItemShow'
    | 'CheckBox'
    | 'TextInput'
    | 'Slider'
    | 'ProgressTimer'
    | 'LoadingBar'
    | 'Effect'
    | 'UIModel'
    | 'SpineAnim'
    | 'PageView'
    | 'ListView'
    | 'ScrollView'
    | 'QuickCell'
    | 'MenuItem'
    | 'TableView'
    | 'Unsupported'
export type GuiValueSource = 'literal' | 'default' | 'dynamic'
export type GuiCompatibility = 'supported' | 'approximate' | 'dynamic' | 'unknown'

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
  pressedImage?: BoundValue<string> | null
  disabledImage?: BoundValue<string> | null
  fontSize?: BoundValue<number> | null
  color?: BoundValue<string> | null
  opacity?: BoundValue<number> | null
}

export interface GuiTransform {
  scaleX: BoundValue<number>
  scaleY: BoundValue<number>
  rotation: BoundValue<number>
  skewX: BoundValue<number>
  skewY: BoundValue<number>
}

export interface GuiContainer {
  direction?: BoundValue<number> | null
  gravity?: BoundValue<number> | null
  itemsMargin?: BoundValue<number> | null
  innerWidth?: BoundValue<number> | null
  innerHeight?: BoundValue<number> | null
}

export interface GuiScale9 {
  enabled: BoundValue<boolean>
  left: BoundValue<number>
  bottom: BoundValue<number>
  right: BoundValue<number>
  top: BoundValue<number>
}

export interface GuiRawLuaLiteral {
  luaLiteral: string
}

export type GuiPropertyValue = string | number | boolean | GuiRawLuaLiteral | null

export interface GuiSourceBinding {
  createCall?: SourceSpan | null
  statement?: SourceSpan | null
  properties?: Record<string, SourceSpan>
  safeInsertion?: SourceSpan | null
}

export interface GuiSourceRef {
  devRelativePath: string
  line?: number | null
  column?: number | null
  templateNodeId?: string | null
}

export interface GuiDataProvenance {
  kind: 'staticConfig' | 'sceneMock' | 'runtimeDerived' | 'missing' | 'userSnapshot'
  key: string
  description: string
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
  transform?: GuiTransform | null
  visible?: BoundValue<boolean> | null
  ignoreContentAdaptWithSize?: BoundValue<boolean> | null
  clippingEnabled?: BoundValue<boolean> | null
  scale9?: GuiScale9 | null
  container?: GuiContainer | null
  properties?: Record<string, BoundValue<GuiPropertyValue>>
  assetSlots?: Record<string, BoundValue<string>>
  paint?: GuiPaint | null
  compatibility: GuiCompatibility
  compatibilityReasonCode?: string | null
  compatibilityReason?: string | null
  binding?: GuiSourceBinding | null
  sourceRef?: GuiSourceRef | null
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
  provenance?: GuiDataProvenance[]
  diagnostics: GuiDiagnostic[]
}

export interface GuiDocumentEntry {
  path: string
  kind: 'editable' | 'readonly'
  platform: 'mobile' | 'pc' | 'shared'
  peerPath?: string | null
}

export type GuiDevEntryType = 'directory' | 'file'
export type GuiDevPolicy = 'editable' | 'readonly' | 'asset' | 'info'

export interface GuiDevTreeEntry {
  path: string
  name: string
  entryType: GuiDevEntryType
  policy: GuiDevPolicy
  hidden: boolean
  size: number
  hasChildren: boolean
  descriptionId: string
}

export interface GuiDevTreePage {
  parentPath: string
  entries: GuiDevTreeEntry[]
  nextCursor?: string | null
  metadataVersion: string
}

export interface GuiReadonlyDocument {
  devRelativePath: string
  source: string
  sha256: string
  encoding: string
  newline: string
  readOnly: true
}

export interface GuiAssetMeta {
  logicalPath: string
  mimeType: string
  byteLength: number
  sha256: string
  width: number
  height: number
}

export interface GuiDesignerStatus {
  projectId?: string | null
  available: boolean
  devRoot?: string | null
  guiExportAvailable?: boolean
  resourceAvailable?: boolean
  reason?: string | null
}

export type GuiRuntimeBackend = 'sidecar' | 'unavailable'
export type GuiRuntimeDataSource = 'builtInMock' | 'projectStatic'

export interface GuiRuntimeTableCapability {
  name: string
  available: boolean
}

export interface GuiRuntimeCapabilities {
  available: boolean
  backend: GuiRuntimeBackend
  dataSource: GuiRuntimeDataSource
  projectStaticAvailable: boolean
  tables: GuiRuntimeTableCapability[]
  limits: Record<string, number>
  diagnostics: GuiDiagnostic[]
}

export interface GuiRuntimeSceneCatalogEntry {
  id: string
  name: string
  category: string
  layoutPath: string
  platform: 'mobile' | 'pc' | 'shared'
  compatibility: GuiCompatibility
}

export interface GuiRuntimeSceneCatalog {
  scenes: GuiRuntimeSceneCatalogEntry[]
}

export interface GuiRuntimeSceneResult {
  sessionId: string
  sequence: number
  scene?: Mir3UiDocument | null
  fallback: boolean
  diagnostics: GuiDiagnostic[]
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
