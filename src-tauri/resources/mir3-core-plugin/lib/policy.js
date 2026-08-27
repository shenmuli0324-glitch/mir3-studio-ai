import { isAbsolute, relative, resolve, sep } from 'node:path'
import process from 'node:process'

const SYSTEM_SESSION_PREFIX = 'mir3-system-'
const GLOBAL_SESSION_PREFIX = 'global-'

function normalizeForCompare(value) {
  const normalized = resolve(value)
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

function isWithin(root, candidate) {
  const result = relative(normalizeForCompare(root), normalizeForCompare(candidate))
  return result === '' || (!result.startsWith(`..${sep}`) && result !== '..' && !isAbsolute(result))
}

function isSystemSession(session) {
  return typeof session?.id === 'string' && session.id.startsWith(SYSTEM_SESSION_PREFIX)
}

function isGlobalSession(session) {
  return typeof session?.id === 'string' && session.id.startsWith(GLOBAL_SESSION_PREFIX)
}

function isMir3ManagedSession(session) {
  return isSystemSession(session) || isGlobalSession(session)
}

function targetPath(target) {
  return target?.canonicalPath || target?.displayPath || target?.path || ''
}

function isProtectedTarget(projectRoot, target) {
  const path = targetPath(target)
  return Boolean(path) && isWithin(projectRoot, path)
}

function managedWriteViolation(projectRoot, session, target) {
  if (!isMir3ManagedSession(session))
    return null
  if (!projectRoot || !session?.header?.cwd || !isWithin(projectRoot, session.header.cwd))
    return 'MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE'
  if (isProtectedTarget(projectRoot, target))
    return 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED'
  return null
}

function sessionScopeViolation(projectRoot, session) {
  if (!projectRoot || !session?.header?.cwd)
    return 'MIR3_PROJECT_SCOPE_UNAVAILABLE'
  if (!isWithin(projectRoot, session.header.cwd))
    return 'MIR3_PROJECT_SESSION_OUTSIDE_SCOPE'
  return null
}

function developmentWriteViolation(projectRoot, session, target) {
  const sessionViolation = sessionScopeViolation(projectRoot, session)
  if (sessionViolation)
    return sessionViolation
  const path = targetPath(target)
  if (path && !isWithin(projectRoot, path))
    return 'MIR3_PROJECT_WRITE_OUTSIDE_SCOPE'
  if (isMir3ManagedSession(session) && path && isWithin(projectRoot, path))
    return 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED'
  return null
}

export {
  developmentWriteViolation,
  GLOBAL_SESSION_PREFIX,
  isGlobalSession,
  isMir3ManagedSession,
  isProtectedTarget,
  isSystemSession,
  isWithin,
  managedWriteViolation,
  sessionScopeViolation,
  SYSTEM_SESSION_PREFIX,
}
