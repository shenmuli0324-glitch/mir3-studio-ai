export interface Mir3SessionLike {
  id?: string
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

export const SYSTEM_SESSION_PREFIX: string
export const GLOBAL_SESSION_PREFIX: string
