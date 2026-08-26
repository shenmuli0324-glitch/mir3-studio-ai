import process from 'node:process'
import { effectiveSandboxMode, setSandboxMode } from '@deepseek-ai/dsh-sandbox-policy'
import { isMir3ManagedSession, isProtectedTarget, isWithin, managedWriteViolation } from './policy.js'

function installSystemSessionPolicy(ctx) {
  const projectRoot = process.env.MIR3_ACTIVE_PROJECT_ROOT
  const previousModes = new Map()

  function protectSession(session) {
    if (!isMir3ManagedSession(session) || previousModes.has(session.id))
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
    const violation = managedWriteViolation(projectRoot, session, target)
    if (!violation)
      return next()
    if (violation === 'MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE') {
      throw new Error('MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE: system AI writes require a verified Studio project scope')
    }
    throw new Error('MIR3_SYSTEM_SESSION_DRAFT_REQUIRED: direct project writes are disabled; use the scoped MIR3 MCP Draft tools')
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

export { apply, installSystemSessionPolicy, isMir3ManagedSession, isProtectedTarget, managedWriteViolation }
export default plugin
