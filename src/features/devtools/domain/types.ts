export interface DomainManifest {
  kind: 'domain'
  systemId: string
  version: string
  kernelApiRange: string
  supportedEngineRange: string
  engineCompatibility: {
    strategy: string
    versionAliases: string[]
    requiredEvidence: string[]
    unknownVersionPolicy: 'readonly'
    incompatibleVersionPolicy: 'readonly'
  }
  manifestSchemaVersion: number
  resourceSchemaVersion: number
  capabilitySchemaVersion: number
  memorySchemaVersion: number
  category: string
  complexity: number
  renderer: string
  fileProjection: {
    keywords: string[]
    editableExtensions: string[]
    structuredExtensions: string[]
    readonlyExtensions: string[]
  }
  dependencies: string[]
  capabilities: Array<{
    id: string
    version: string
    parameterSchema: Record<string, unknown>
    readSystems: string[]
    writeSystems: string[]
    preconditions: string[]
    steps: Array<{ type: 'domain-operation', operation: string }>
    reversible: boolean
    previewRequired: boolean
    validationRequired: boolean
    confirmationRequired: boolean
  }>
}

export interface DomainSystemDescription {
  manifest: DomainManifest
  ownedFiles: number
  sharedFiles: number
  writableFiles: number
  readonlyFiles: number
  diagnostics: string[]
}

export interface DomainFileRecord {
  path: string
  role: string
  category: string
  extension?: string | null
  size: number
  modifiedAt: number
  resourceId: string
  ownership: 'owned' | 'shared' | 'dependency' | 'unknown'
  access: 'editable' | 'structured' | 'readonly'
  systems: string[]
}

export interface DomainTextProjection {
  kind: 'text'
  content: string
  truncated: boolean
}

export interface DomainXlsSheetProjection {
  name: string
  rowCount: number
  columnCount: number
  rows: string[][]
}

export interface DomainXlsProjection {
  kind: 'xls'
  sha256: string
  sheets: DomainXlsSheetProjection[]
  truncated: boolean
}

export interface DomainRecordProjection {
  kind: 'record'
  fields: Record<string, unknown>
  source: {
    path: string
    sheet: string | null
    row: number | null
    headers: string[]
  }
}

export interface DomainMapSpriteRef {
  library: number
  image: number
}

export interface DomainMapCell {
  x: number
  y: number
  background: DomainMapSpriteRef
  middle: DomainMapSpriteRef
  front: DomainMapSpriteRef
  walkable: boolean
  frontBlocked: boolean
  middleAnimationFrames: number
  frontAnimationFrames: number
  doorIndex: number
  doorOffset: number
  light: number
}

export interface DomainMapProjection {
  kind: 'map'
  header: {
    format: string
    width: number
    height: number
    sourceSha256: string
    capabilities: Record<string, boolean>
    diagnostics: Array<{ code: string, message: string }>
  }
  initialChunk: {
    chunkX: number
    chunkY: number
    startX: number
    startY: number
    width: number
    height: number
    cells: DomainMapCell[]
  }
}

export type DomainResourceProjection = DomainTextProjection | DomainXlsProjection | DomainMapProjection | DomainRecordProjection

export interface DomainResourceRecord {
  id: string
  systemId: string
  resourceType: string
  label: string
  files: DomainFileRecord[]
  dependencySystems: string[]
  writable: boolean
  projection: DomainResourceProjection | null
  diagnostics: string[]
  fields: Record<string, unknown>
  source: {
    path: string
    sheet: string | null
    row: number | null
    headers: string[]
  }
  dependencies: Array<{
    field: string
    value: string
    systemId: string
    required: boolean
    resolvedResourceId: string | null
    diagnostics: string[]
  }>
  mappingsApplied: string[]
}

export interface DomainValidationReport {
  systemId: string
  valid: boolean
  ownedFiles: number
  writableFiles: number
  readonlyFiles: number
  missingDependencies: string[]
  diagnostics: string[]
}

export interface DomainDependencyGraph {
  systemId: string
  direct: string[]
  transitive: string[]
  missing: string[]
  cycles: string[][]
}

export interface SafeTextOpen {
  projectId: string
  relativePath: string
  content: string
  encoding: string
  bom: string
  newline?: string | null
  mixedNewlines: boolean
  sha256: string
  draftId?: string | null
  revision: number
}

export interface SafeTextPatchResult {
  draftId: string
  revision: number
  sha256: string
  preview: DomainDraftPreview
}

export interface DomainDraft {
  id: string
  intent: string
  revision: number
  status: 'open' | 'applied' | 'discarded'
  createdAt: number
  updatedAt: number
}

export interface DomainDraftPreview {
  draft: DomainDraft
  changes: Array<{
    path: string
    deleted: boolean
    baseSha256?: string | null
    newSha256?: string | null
    unifiedDiff?: string | null
  }>
  diffHash: string
}

export interface DomainDraftConfirmation {
  preview: DomainDraftPreview
  confirmationToken: string
}

export interface LegacyDraftCloneRequest {
  legacyDraftId: string
  systemId: string
  pluginVersion: string
  expectedSources: Record<string, string>
  intent: string
}

export interface DomainSnapshot {
  id: string
  draftId?: string | null
  createdAt: number
}

export interface CompositeDraftReviewItem {
  draftId: string
  systemId: string
  pluginVersion: string
  confirmation: DomainDraftConfirmation
  validation: DomainValidationReport
}

export interface CompositeDraftReview {
  compositeId: string
  drafts: CompositeDraftReviewItem[]
}

export interface CompositeDraftApplyInput {
  draftId: string
  confirmationToken: string
}

export interface CompositeDraftApplyResult {
  compositeId: string
  draftIds: string[]
  snapshot: DomainSnapshot
}

export interface SystemSessionBinding {
  taskId: string
  systemId: string
  sessionId: string
  pluginVersion: string
  draftId?: string | null
  status: string
  updatedAt: number
}

export interface TaskReceipt {
  id: string
  taskId: string
  systemId: string
  summary: string
  status: string
  draftId?: string | null
  pluginVersions: Record<string, string>
  evidence: Record<string, unknown>
  createdAt: number
}

export interface TaskScopeLease {
  token: string
  taskId: string
  readSystems: string[]
  writeSystems: string[]
  draftIds: string[]
  pluginVersions: Record<string, string>
  expiresAt: number
}

export interface UserCapability {
  id: string
  version: string
  systemId: string
  scope: 'project' | 'personal' | 'team'
  name: string
  description: string
  parameterSchema: Record<string, unknown>
  steps: Array<{ type: 'domain-operation', operation: string, [key: string]: unknown }>
  readSystems: string[]
  writeSystems: string[]
  status: 'draft' | 'active' | 'disabled' | 'deprecated'
  sourceTaskId: string
  createdAt: number
  updatedAt: number
}

export interface CapabilityResolution {
  capability: UserCapability
  resolvedScope: UserCapability['scope']
  sourceProjectId: string
  shadowedScopes: Array<UserCapability['scope']>
}

export interface GlobalCapabilityCompileRequest {
  receiptIds: string[]
  id: string
  name: string
  description: string
}

export interface CapabilityRollbackRequest {
  capabilityId: string
  scope: UserCapability['scope']
  fromVersion: string
  toVersion: string
}

export interface DomainMemory {
  id: string
  systemId: string
  scope: 'project' | 'personal' | 'team'
  kind: string
  summary: string
  body: Record<string, unknown>
  status: 'candidate' | 'active' | 'disabled' | 'contested' | 'revoked'
  sourceTaskId: string
  pluginVersion: string
  createdAt: number
  updatedAt: number
}

export interface DomainPackRelease {
  version: string
  hash: string
  directory: string
}

export interface DomainPackState {
  schemaVersion: number
  systemId: string
  enabled: boolean
  candidate?: DomainPackRelease | null
  current?: DomainPackRelease | null
  previous?: DomainPackRelease | null
  lkg?: DomainPackRelease | null
  changelog: string
}

export interface DomainPackRemoteCandidate {
  systemId: string
  version: string
  currentVersion?: string | null
  archiveSize: number
  archiveSha256: string
}

export interface DomainPackUpdateCheck {
  schemaVersion: number
  updates: DomainPackRemoteCandidate[]
}
