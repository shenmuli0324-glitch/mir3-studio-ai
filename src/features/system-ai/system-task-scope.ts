import type { DomainManifest, TaskScopeLease } from '@/features/devtools/domain/types'

export interface SystemTaskScopeContract {
  taskId: string
  systemId: string
  pluginVersion: string
  readSystems: string[]
  draftIds: string[]
  pluginVersions: Record<string, string>
}

export function buildSystemTaskScopeContract(
  manifest: DomainManifest,
  taskId: string,
  draftId: string | null | undefined,
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
  return {
    taskId,
    systemId: manifest.systemId,
    pluginVersion: manifest.version,
    readSystems,
    draftIds: draftId ? [draftId] : [],
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
  if (!sameUniqueStrings(lease.draftIds, contract.draftIds))
    throw new Error('SYSTEM_SCOPE_LEASE_DRAFT_MISMATCH: Draft binding changed during issuance')
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
    '- All changes must use the scoped MIR3 MCP Draft tools; preview and validate the Draft before asking the user to apply it.',
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
