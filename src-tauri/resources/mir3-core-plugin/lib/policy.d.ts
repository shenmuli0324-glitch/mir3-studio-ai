export interface Mir3SessionLike {
  id?: string
  header?: { cwd?: string }
}

export interface Mir3FileTarget {
  canonicalPath?: string
  displayPath?: string
  path?: string
}

export interface Mir3FileReferenceCandidate {
  path: string
  kind: 'file' | 'directory'
}

export interface Mir3AgentLike {
  session?: Mir3SessionLike
}

export interface Mir3ToolExecutionLike {
  name?: string
  arguments?: Record<string, unknown>
  agent?: Mir3AgentLike
}

export function isSystemSession(session: Mir3SessionLike): boolean
export function isGuiSession(session: Mir3SessionLike): boolean
export function isGlobalSession(session: Mir3SessionLike): boolean
export function isMir3ManagedSession(session: Mir3SessionLike): boolean
export function isDevelopmentFilePath(path: string, projectRoot: string | undefined, sessionCwd?: string): boolean
export function isDevelopmentTextPath(path: string): boolean
export function isDevelopmentStructuredPath(path: string): boolean
export function isProtectedTarget(projectRoot: string, target: Mir3FileTarget): boolean
export function isWithin(root: string, candidate: string): boolean
export function managedWriteViolation(projectRoot: string | undefined, session: Mir3SessionLike, target: Mir3FileTarget): 'MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE' | 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED' | null
export function sessionScopeViolation(projectRoot: string | undefined, session: Mir3SessionLike): 'MIR3_PROJECT_SCOPE_UNAVAILABLE' | 'MIR3_PROJECT_SESSION_OUTSIDE_SCOPE' | null
export function developmentWriteViolation(projectRoot: string | undefined, session: Mir3SessionLike, target: Mir3FileTarget): 'MIR3_PROJECT_SCOPE_UNAVAILABLE' | 'MIR3_PROJECT_SESSION_OUTSIDE_SCOPE' | 'MIR3_PROJECT_WRITE_OUTSIDE_SCOPE' | 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED' | null
export function developmentReadViolation(projectRoot: string | undefined, session: Mir3SessionLike, requestedPath: unknown): 'MIR3_PROJECT_SCOPE_UNAVAILABLE' | 'MIR3_PROJECT_SESSION_OUTSIDE_SCOPE' | 'MIR3_PROJECT_READ_OUTSIDE_SCOPE' | 'MIR3_NON_DEVELOPMENT_FILE_SKIPPED' | 'MIR3_XLS_MCP_REQUIRED' | null
export function developmentToolViolation(projectRoot: string | undefined, exec: Mir3ToolExecutionLike): string | null
export function filterDevelopmentReferences(projectRoot: string | undefined, agent: Mir3AgentLike, candidates: Mir3FileReferenceCandidate[]): Mir3FileReferenceCandidate[]

export const SYSTEM_SESSION_PREFIX: string
export const GUI_SESSION_PREFIX: string
export const GLOBAL_SESSION_PREFIX: string
