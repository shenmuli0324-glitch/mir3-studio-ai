import { extname, isAbsolute, relative, resolve, sep } from 'node:path'
import process from 'node:process'
import { effectiveSandboxMode, setSandboxMode } from '@deepseek-ai/dsh-sandbox-policy'

const PROTECTED_EXTENSIONS = new Set(['.txt', '.lua', '.xls'])

function normalizeForCompare(value) {
  const normalized = resolve(value)
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

function isWithin(root, candidate) {
  const result = relative(normalizeForCompare(root), normalizeForCompare(candidate))
  return result === '' || (!result.startsWith(`..${sep}`) && result !== '..' && !isAbsolute(result))
}

function targetPath(target) {
  return target?.canonicalPath || target?.displayPath || target?.path || ''
}

function isProtectedTarget(root, target) {
  const path = targetPath(target)
  return Boolean(path) && isWithin(root, path) && PROTECTED_EXTENSIONS.has(extname(path).toLowerCase())
}

function installPolicy(ctx) {
  const projectRoot = process.env.MIR3_ACTIVE_PROJECT_ROOT
  if (!projectRoot)
    return

  const previousModes = new Map()

  function protectSession(session) {
    const cwd = session?.header?.cwd
    if (!cwd || !isWithin(projectRoot, cwd) || previousModes.has(session.id))
      return
    previousModes.set(session.id, effectiveSandboxMode(session.events))
    setSandboxMode(session, 'read-only')
  }

  for (const session of ctx.sessions.list())
    protectSession(session)

  const disposeCreated = ctx.on('session/created', protectSession, { global: true })
  const denyWrite = (target, _exec, next) => {
    if (isProtectedTarget(projectRoot, target)) {
      throw new Error('MIR3_SAFE_FILES_DRAFT_REQUIRED: use mir3_draft_patch; direct TXT/Lua/XLS writes are disabled while Safe Files is enabled')
    }
    return next()
  }
  const disposeWrite = ctx.on('fs/write-intent', denyWrite, { global: true })
  const disposeEdit = ctx.on('fs/edit-intent', denyWrite, { global: true })

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
  try {
    return installPolicy(ctx)
  }
  catch (error) {
    ctx.logger?.error?.(`MIR3 Safe Files policy disabled: ${String(error)}`)
    return undefined
  }
}

const plugin = { name: 'mir3-safe-files', inject: ['sessions', 'sandboxPolicy'], apply }

export { apply }
export default plugin
