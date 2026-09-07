import process from 'node:process'
import { effectiveSandboxMode, setSandboxMode } from '@deepseek-ai/dsh-sandbox-policy'
import { developmentToolViolation, developmentWriteViolation, filterDevelopmentReferences, isMir3ManagedSession, isProtectedTarget, managedWriteViolation, sessionScopeViolation } from './policy.js'

function installSystemSessionPolicy(ctx) {
  const projectRoot = process.env.MIR3_ACTIVE_PROJECT_ROOT
  const previousModes = new Map()

  function protectSession(session) {
    const violation = sessionScopeViolation(projectRoot, session)
    if (violation) {
      ctx.logger?.error?.(`${violation}: ${session.id}`)
      throw new Error(`${violation}: Session cwd must stay inside the active MIR3 project`)
    }
    if (!isMir3ManagedSession(session) || previousModes.has(session.id))
      return
    previousModes.set(session.id, effectiveSandboxMode(session.events))
    setSandboxMode(session, 'read-only')
  }

  for (const session of ctx.sessions.list())
    protectSession(session)

  const disposeCreated = ctx.on('session/created', protectSession, { global: true })
  const denySystemWrite = (target, exec, next) => {
    const session = exec?.agent?.session
    const violation = developmentWriteViolation(projectRoot, session, target)
    if (!violation)
      return next()
    if (violation === 'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED')
      throw new Error('MIR3_SYSTEM_SESSION_DRAFT_REQUIRED: direct project writes are disabled; use the scoped MIR3 MCP tools')
    throw new Error(`${violation}: development writes must stay inside the active MIR3 project`)
  }
  const disposeWrite = ctx.on('fs/write-intent', denySystemWrite, { global: true })
  const disposeEdit = ctx.on('fs/edit-intent', denySystemWrite, { global: true })
  const denyUnscopedDiscovery = (exec, next) => {
    const violation = developmentToolViolation(projectRoot, exec)
    if (!violation)
      return next()
    if (violation === 'MIR3_DEVELOPMENT_SEARCH_REQUIRED')
      throw new Error(`${violation}: use mir3_resource_query or an official read capability`)
    if (violation === 'MIR3_XLS_MCP_REQUIRED')
      throw new Error(`${violation}: BIFF .xls must be read through mir3_resource_get`)
    if (violation === 'MIR3_GENERIC_SHELL_DISABLED')
      throw new Error(`${violation}: managed MIR3 sessions must inspect and modify project data through scoped MIR3 MCP tools`)
    if (violation === 'MIR3_PROJECT_SCOPE_UNAVAILABLE' || violation === 'MIR3_PROJECT_SESSION_OUTSIDE_SCOPE')
      throw new Error(`${violation}: managed MIR3 session scope is unavailable`)
    throw new Error(`${violation}: only .lua, .map, and .txt can use generic text readers`)
  }
  const disposeToolGate = ctx.on('tools/pre-execute', denyUnscopedDiscovery, { global: true })
  const originalReferenceList = ctx.fileReferences.list
  ctx.fileReferences.list = async (agent, query, signal) => {
    const candidates = await originalReferenceList.call(ctx.fileReferences, agent, query, signal)
    return filterDevelopmentReferences(projectRoot, agent, candidates)
  }

  return () => {
    if (typeof disposeCreated === 'function')
      disposeCreated()
    if (typeof disposeWrite === 'function')
      disposeWrite()
    if (typeof disposeEdit === 'function')
      disposeEdit()
    if (typeof disposeToolGate === 'function')
      disposeToolGate()
    ctx.fileReferences.list = originalReferenceList
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

const plugin = { name: 'mir3-core', inject: ['sessions', 'sandboxPolicy', 'fileReferences'], apply }

export { apply, developmentToolViolation, developmentWriteViolation, filterDevelopmentReferences, installSystemSessionPolicy, isMir3ManagedSession, isProtectedTarget, managedWriteViolation, sessionScopeViolation }
export default plugin
