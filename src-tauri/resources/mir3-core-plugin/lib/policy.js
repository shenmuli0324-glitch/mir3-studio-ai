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

export {
  GLOBAL_SESSION_PREFIX,
  isGlobalSession,
  isMir3ManagedSession,
  isProtectedTarget,
  isSystemSession,
  isWithin,
  SYSTEM_SESSION_PREFIX,
}
