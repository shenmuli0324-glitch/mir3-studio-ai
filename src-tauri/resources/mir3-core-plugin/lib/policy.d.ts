export interface Mir3SessionLike {
  id?: string
  header?: { cwd?: string }
}

export interface Mir3FileTarget {
  canonicalPath?: string
  displayPath?: string
  path?: string
}

export function isSystemSession(session: Mir3SessionLike): boolean
export function isGlobalSession(session: Mir3SessionLike): boolean
export function isMir3ManagedSession(session: Mir3SessionLike): boolean
export function isProtectedTarget(projectRoot: string, target: Mir3FileTarget): boolean
export function isWithin(root: string, candidate: string): boolean
export function managedWriteViolation(projectRoot: string | undefined, session: Mir3SessionLike, target: Mir3FileTarget): 'MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE' | 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED' | null

export const SYSTEM_SESSION_PREFIX: string
export const GLOBAL_SESSION_PREFIX: string
