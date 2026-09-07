import type { DomainManifest, TaskScopeLease } from '@/features/devtools/domain/types'

export interface SystemTaskScopeContract {
  taskId: string
  systemId: string
  pluginVersion: string
  readSystems: string[]
  /** 对外的工作副本标识；后端租约暂以 draftIds 字段传输同一批 ID。 */
  workingCopyIds: string[]
  /** @deprecated 仅用于与旧租约协议兼容。 */
  draftIds: string[]
  pluginVersions: Record<string, string>
}

export function buildSystemTaskScopeContract(
  manifest: DomainManifest,
  taskId: string,
  workingCopyId: string | null | undefined,
  manifests: DomainManifest[],
): SystemTaskScopeContract {
  const readSystems = uniqueStrings([manifest.systemId, ...manifest.dependencies])
  const pluginVersions: Record<string, string> = {}
  for (const systemId of readSystems) {
    const scopedManifest = manifests.find(item => item.systemId === systemId)
    if (!scopedManifest)
      throw new Error(`SYSTEM_SCOPE_DOMAIN_MISSING: ${systemId}`)
    pluginVersions[systemId] = scopedManifest.version
  }
  if (pluginVersions[manifest.systemId] !== manifest.version) {
    throw new Error(
      `SYSTEM_SCOPE_PLUGIN_VERSION_MISMATCH: ${manifest.systemId} expected ${manifest.version}`,
    )
  }
  const workingCopyIds = workingCopyId ? [workingCopyId] : []
  return {
    taskId,
    systemId: manifest.systemId,
    pluginVersion: manifest.version,
    readSystems,
    workingCopyIds,
    draftIds: workingCopyIds,
    pluginVersions,
  }
}

export function assertSystemTaskScopeLease(
  lease: TaskScopeLease,
  contract: SystemTaskScopeContract,
): TaskScopeLease {
  if (!lease.token || lease.taskId !== contract.taskId)
    throw new Error('SYSTEM_SCOPE_LEASE_IDENTITY_MISMATCH: task identity changed during issuance')
  if (!sameUniqueStrings(lease.readSystems, contract.readSystems))
    throw new Error('SYSTEM_SCOPE_LEASE_READ_MISMATCH: readable systems differ from the Manifest scope')
  if (lease.writeSystems.length !== 1 || lease.writeSystems[0] !== contract.systemId) {
    throw new Error(
      `SYSTEM_SCOPE_LEASE_WRITE_MISMATCH: only ${contract.systemId} may be writable`,
    )
  }
  if (!sameUniqueStrings(lease.draftIds, contract.workingCopyIds))
    throw new Error('SYSTEM_SCOPE_LEASE_WORKING_COPY_MISMATCH: working copy binding changed during issuance')
  if (!sameVersionMap(lease.pluginVersions, contract.pluginVersions))
    throw new Error('SYSTEM_SCOPE_LEASE_VERSION_MISMATCH: pinned plugin versions changed during issuance')
  if (lease.pluginVersions[contract.systemId] !== contract.pluginVersion) {
    throw new Error(
      `SYSTEM_SCOPE_LEASE_VERSION_MISMATCH: ${contract.systemId} must remain pinned to ${contract.pluginVersion}`,
    )
  }
  if (!Number.isSafeInteger(lease.expiresAt) || lease.expiresAt <= Date.now())
    throw new Error('SYSTEM_SCOPE_LEASE_EXPIRED: issued lease is not active')
  return lease
}

export function buildSystemTaskRenewalContract(
  manifest: DomainManifest,
  taskId: string,
  previous: TaskScopeLease,
): SystemTaskScopeContract {
  const readSystems = uniqueStrings([manifest.systemId, ...manifest.dependencies])
  const pluginVersions = { ...previous.pluginVersions }
  if (!sameUniqueStrings(Object.keys(pluginVersions), readSystems))
    throw new Error('SYSTEM_SCOPE_RENEWAL_VERSION_MISMATCH: pinned versions do not match the Manifest scope')
  if (pluginVersions[manifest.systemId] !== manifest.version) {
    throw new Error(
      `SYSTEM_SCOPE_RENEWAL_VERSION_MISMATCH: ${manifest.systemId} must remain pinned to ${manifest.version}`,
    )
  }
  return {
    taskId,
    systemId: manifest.systemId,
    pluginVersion: manifest.version,
    readSystems,
    workingCopyIds: [...previous.draftIds],
    draftIds: [...previous.draftIds],
    pluginVersions,
  }
}

export function systemTaskSafetyInstructions(manifest: DomainManifest): string {
  const dependencies = uniqueStrings(manifest.dependencies)
  return [
    '[MIR3 System Safety Rules]',
    `- Only ${manifest.systemId} is writable. Dependencies (${dependencies.join(',') || 'none'}) are reference-only.`,
    '- Modify only files/resources owned by the current system and explicitly marked writable.',
    '- Unknown, generated, shared-without-ownership, and dependency files are read-only.',
    '- Never write project files through shell, terminal, generic filesystem, or editor tools.',
    '- Use only scoped MIR3 working-copy tools for changes and pass workingCopyId, never legacy draftId.',
    '- Reuse the Working Copy supplied in the scope; do not open a second copy for the same system.',
    '- Inspect and validate the Working Copy after edits. Studio owns Save, save nodes, conflict checks, and restore; ask the user to Save, never to Apply a Draft.',
  ].join('\n')
}

function sameUniqueStrings(left: string[], right: string[]): boolean {
  if (left.length !== right.length)
    return false
  return new Set(left).size === left.length && left.every(value => right.includes(value))
}

function sameVersionMap(left: Record<string, string>, right: Record<string, string>): boolean {
  const leftKeys = Object.keys(left)
  const rightKeys = Object.keys(right)
  return sameUniqueStrings(leftKeys, rightKeys) && leftKeys.every(key => left[key] === right[key])
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)]
}
