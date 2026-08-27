import type { DomainManifest, TaskScopeLease } from '../src/features/devtools/domain/types'
import { describe, expect, it } from 'vitest'
import { assertSystemTaskScopeLease, buildSystemTaskRenewalContract, buildSystemTaskScopeContract, systemTaskSafetyInstructions } from '../src/features/system-ai/system-task-scope'

const npc = manifest('npc', '1.3.1', ['item', 'shop'])
const manifests = [npc, manifest('item', '1.4.0'), manifest('shop', '1.5.0')]

describe('system task scope', () => {
  it('keeps dependencies readable while only the current system is writable', () => {
    const contract = buildSystemTaskScopeContract(npc, 'task-npc', 'draft-npc', manifests)
    expect(contract).toEqual({
      taskId: 'task-npc',
      systemId: 'npc',
      pluginVersion: '1.3.1',
      readSystems: ['npc', 'item', 'shop'],
      draftIds: ['draft-npc'],
      pluginVersions: { npc: '1.3.1', item: '1.4.0', shop: '1.5.0' },
    })
    expect(assertSystemTaskScopeLease(lease(contract), contract).writeSystems).toEqual(['npc'])
  })

  it('fails closed when a lease adds dependency write access or changes the Draft version', () => {
    const contract = buildSystemTaskScopeContract(npc, 'task-npc', 'draft-npc', manifests)
    expect(() => assertSystemTaskScopeLease({
      ...lease(contract),
      writeSystems: ['npc', 'item'],
    }, contract)).toThrow('SYSTEM_SCOPE_LEASE_WRITE_MISMATCH')
    expect(() => assertSystemTaskScopeLease({
      ...lease(contract),
      pluginVersions: { ...contract.pluginVersions, npc: '1.3.2' },
    }, contract)).toThrow('SYSTEM_SCOPE_LEASE_VERSION_MISMATCH')
  })

  it('renews with the original Drafts and refuses a current-system version change', () => {
    const contract = buildSystemTaskScopeContract(npc, 'task-npc', 'draft-npc', manifests)
    const previous = lease(contract)
    expect(buildSystemTaskRenewalContract(npc, 'task-npc', previous).draftIds).toEqual(['draft-npc'])
    expect(() => buildSystemTaskRenewalContract(
      { ...npc, version: '1.3.2' },
      'task-npc',
      previous,
    )).toThrow('SYSTEM_SCOPE_RENEWAL_VERSION_MISMATCH')
  })

  it('states that unknown and dependency files cannot be edited directly', () => {
    const instructions = systemTaskSafetyInstructions(npc)
    expect(instructions).toContain('Only npc is writable')
    expect(instructions).toContain('Dependencies (item,shop) are reference-only')
    expect(instructions).toContain('Unknown, generated, shared-without-ownership, and dependency files are read-only')
    expect(instructions).toContain('Never write project files through shell, terminal, generic filesystem, or editor tools')
    expect(instructions).toContain('scoped MIR3 MCP Draft tools')
  })
})

function manifest(systemId: string, version: string, dependencies: string[] = []): DomainManifest {
  return {
    kind: 'domain',
    systemId,
    version,
    kernelApiRange: '^1.0.0',
    supportedEngineRange: '*',
    engineCompatibility: {
      strategy: 'content-fingerprint',
      versionAliases: [],
      requiredEvidence: [],
      unknownVersionPolicy: 'readonly',
      incompatibleVersionPolicy: 'readonly',
    },
    manifestSchemaVersion: 1,
    resourceSchemaVersion: 1,
    capabilitySchemaVersion: 1,
    memorySchemaVersion: 1,
    category: 'entity',
    complexity: 2,
    renderer: 'source-v1',
    fileProjection: {
      keywords: [],
      editableExtensions: [],
      structuredExtensions: [],
      readonlyExtensions: [],
    },
    capabilities: [],
    dependencies,
  }
}

function lease(contract: ReturnType<typeof buildSystemTaskScopeContract>): TaskScopeLease {
  return {
    token: 'scope-token',
    taskId: contract.taskId,
    readSystems: contract.readSystems,
    writeSystems: [contract.systemId],
    draftIds: contract.draftIds,
    pluginVersions: contract.pluginVersions,
    expiresAt: Date.now() + 60_000,
  }
}
