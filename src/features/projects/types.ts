export type ProjectStatus = 'valid' | 'warning' | 'missing'
export type ScanPhase = 'idle' | 'running' | 'completed' | 'cancelled' | 'error'
export type KnowledgeStatus = 'PROPOSED' | 'ACTIVE' | 'CONTESTED' | 'SUPERSEDED' | 'REVOKED'

export interface Mir3Project {
  id: string
  name: string
  root: string
  clientRoot: string
  engineRoot: string
  activeWorkspaceRoot: string
  engineVersion?: string | null
  clientVersion?: string | null
  status: ProjectStatus
  warnings: string[]
  lastScanAt?: number | null
  createdAt: number
  updatedAt: number
}

export interface ScanSummary {
  projectId: string
  scannedFiles: number
  indexedTextFiles: number
  removedFiles: number
  categories: Record<string, number>
  completedAt: number
  cancelled: boolean
}

export interface ScanState {
  projectId?: string | null
  phase: ScanPhase
  summary?: ScanSummary | null
  error?: string | null
}

export interface IndexStats {
  totalFiles: number
  indexedTextFiles: number
  categories: Record<string, number>
  lastScanAt?: number | null
}

export interface KnowledgeRecord {
  id: string
  status: KnowledgeStatus
  kind: string
  summary: string
  body: string
  engineVersion?: string | null
  evidence: string[]
  createdAt: number
  updatedAt: number
}
