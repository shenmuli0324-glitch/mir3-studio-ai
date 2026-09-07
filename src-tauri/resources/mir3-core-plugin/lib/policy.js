import { isAbsolute, relative, resolve, sep } from 'node:path'
import process from 'node:process'

const SYSTEM_SESSION_PREFIX = 'mir3-system-'
const GUI_SESSION_PREFIX = 'mir3-gui-'
const GLOBAL_SESSION_PREFIX = 'global-'
const DEVELOPMENT_TEXT_EXTENSIONS = new Set(['lua', 'map', 'txt'])
const DEVELOPMENT_STRUCTURED_EXTENSIONS = new Set(['xls'])

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

function isGuiSession(session) {
  return typeof session?.id === 'string' && session.id.startsWith(GUI_SESSION_PREFIX)
}

function isMir3ManagedSession(session) {
  return isSystemSession(session) || isGuiSession(session) || isGlobalSession(session)
}

function targetPath(target) {
  return target?.canonicalPath || target?.displayPath || target?.path || ''
}

function fileExtension(value) {
  const name = String(value ?? '').replaceAll('\\', '/').split('/').pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : ''
}

function isDevelopmentTextPath(value) {
  return DEVELOPMENT_TEXT_EXTENSIONS.has(fileExtension(value))
}

function isDevelopmentStructuredPath(value) {
  return DEVELOPMENT_STRUCTURED_EXTENSIONS.has(fileExtension(value))
}

function isDevelopmentFilePath(value, projectRoot, sessionCwd = projectRoot) {
  if (!projectRoot || !sessionCwd)
    return false
  const target = resolve(sessionCwd, value)
  if (!isWithin(projectRoot, target))
    return false
  const projectRelative = relative(normalizeForCompare(projectRoot), normalizeForCompare(target))
    .replaceAll('\\', '/')
    .toLowerCase()
  const inDevelopmentRoot = projectRelative.startsWith('客户端/dev/')
    || projectRelative.startsWith('引擎/')
  return inDevelopmentRoot
    && (isDevelopmentTextPath(target) || isDevelopmentStructuredPath(target))
}

function developmentReadViolation(projectRoot, session, requestedPath) {
  const sessionViolation = sessionScopeViolation(projectRoot, session)
  if (sessionViolation)
    return sessionViolation
  if (typeof requestedPath !== 'string' || requestedPath.trim() === '')
    return 'MIR3_NON_DEVELOPMENT_FILE_SKIPPED'
  const target = resolve(session.header.cwd, requestedPath)
  if (!isWithin(projectRoot, target))
    return 'MIR3_PROJECT_READ_OUTSIDE_SCOPE'
  if (!isDevelopmentFilePath(target, projectRoot, session.header.cwd))
    return 'MIR3_NON_DEVELOPMENT_FILE_SKIPPED'
  if (isDevelopmentStructuredPath(target))
    return 'MIR3_XLS_MCP_REQUIRED'
  return null
}

function developmentToolViolation(projectRoot, exec) {
  const session = exec?.agent?.session
  if (!isMir3ManagedSession(session))
    return null
  const scopeViolation = sessionScopeViolation(projectRoot, session)
  if (scopeViolation)
    return scopeViolation
  if (exec.name === 'bash' || exec.name === 'pwsh' || exec.name === 'shell')
    return 'MIR3_GENERIC_SHELL_DISABLED'
  if (exec.name === 'glob' || exec.name === 'grep')
    return 'MIR3_DEVELOPMENT_SEARCH_REQUIRED'
  if (exec.name === 'read_image')
    return 'MIR3_NON_DEVELOPMENT_FILE_SKIPPED'
  if (exec.name === 'read')
    return developmentReadViolation(projectRoot, session, exec.arguments?.file_path)
  if (exec.name === 'str_replace_editor')
    return developmentReadViolation(projectRoot, session, exec.arguments?.path)
  return null
}

function filterDevelopmentReferences(projectRoot, agent, candidates) {
  if (!Array.isArray(candidates) || sessionScopeViolation(projectRoot, agent?.session))
    return []
  if (!isMir3ManagedSession(agent?.session))
    return candidates
  return candidates.filter((candidate) => {
    const target = resolve(agent.session.header.cwd, candidate?.path ?? '')
    if (!isWithin(projectRoot, target))
      return false
    return candidate?.kind === 'directory'
      || (candidate?.kind === 'file'
        && isDevelopmentFilePath(candidate.path, projectRoot, agent.session.header.cwd))
  })
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
  developmentReadViolation,
  developmentToolViolation,
  developmentWriteViolation,
  filterDevelopmentReferences,
  GLOBAL_SESSION_PREFIX,
  GUI_SESSION_PREFIX,
  isDevelopmentFilePath,
  isDevelopmentStructuredPath,
  isDevelopmentTextPath,
  isGlobalSession,
  isGuiSession,
  isMir3ManagedSession,
  isProtectedTarget,
  isSystemSession,
  isWithin,
  managedWriteViolation,
  sessionScopeViolation,
  SYSTEM_SESSION_PREFIX,
}
