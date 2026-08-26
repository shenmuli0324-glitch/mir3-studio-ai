import { extname, isAbsolute, relative, resolve, sep } from 'node:path'
import process from 'node:process'
import { effectiveSandboxMode, setSandboxMode } from '@deepseek-ai/dsh-sandbox-policy'

const SYSTEM_SESSION_PREFIX = 'mir3-system-'
const PROTECTED_EXTENSIONS = new Set(['.txt', '.lua', '.xls'])

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

function targetPath(target) {
  return target?.canonicalPath || target?.displayPath || target?.path || ''
}

function isProtectedTarget(projectRoot, target) {
  const path = targetPath(target)
  return Boolean(path)
    && isWithin(projectRoot, path)
    && PROTECTED_EXTENSIONS.has(extname(path).toLowerCase())
}

function installSystemSessionPolicy(ctx) {
  const projectRoot = process.env.MIR3_ACTIVE_PROJECT_ROOT
  const previousModes = new Map()

  function protectSession(session) {
    if (!isSystemSession(session) || previousModes.has(session.id))
      return
    previousModes.set(session.id, effectiveSandboxMode(session.events))
    setSandboxMode(session, 'read-only')
    if (!projectRoot || !session?.header?.cwd || !isWithin(projectRoot, session.header.cwd))
      ctx.logger?.error?.(`MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE: ${session.id}`)
  }

  for (const session of ctx.sessions.list())
    protectSession(session)

  const disposeCreated = ctx.on('session/created', protectSession, { global: true })
  const denySystemWrite = (target, exec, next) => {
    const session = exec?.agent?.session
    if (!isSystemSession(session))
      return next()
    if (!projectRoot || !session?.header?.cwd || !isWithin(projectRoot, session.header.cwd)) {
      throw new Error('MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE: system AI writes require a verified Studio project scope')
    }
    if (isProtectedTarget(projectRoot, target)) {
      throw new Error('MIR3_SYSTEM_SESSION_DRAFT_REQUIRED: direct TXT/Lua/XLS writes are disabled; use the scoped MIR3 MCP Draft tools')
    }
    return next()
  }
  const disposeWrite = ctx.on('fs/write-intent', denySystemWrite, { global: true })
  const disposeEdit = ctx.on('fs/edit-intent', denySystemWrite, { global: true })

  return () => {
    if (typeof disposeCreated === 'function')
      disposeCreated()
    if (typeof disposeWrite === 'function')
      disposeWrite()
    if (typeof disposeEdit === 'function')
      disposeEdit()
    for (const session of ctx.sessions.list()) {
      if (!previousModes.has(session.id) || effectiveSandboxMode(session.events) !== 'read-only')
        continue
      setSandboxMode(session, previousModes.get(session.id) ?? ctx.sandboxPolicy.defaultMode)
    }
  }
}

function apply(ctx) {
  return installSystemSessionPolicy(ctx)
}

const plugin = { name: 'mir3-core', inject: ['sessions', 'sandboxPolicy'], apply }

export { apply, installSystemSessionPolicy, isProtectedTarget, isSystemSession }
export default plugin
