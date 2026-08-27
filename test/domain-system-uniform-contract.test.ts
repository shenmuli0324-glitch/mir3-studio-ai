import { readdirSync, readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const root = new URL('../', import.meta.url)

function source(relativePath: string) {
  return readFileSync(new URL(relativePath, root), 'utf8')
}

describe('33-system simplified workspace contract', () => {
  it('routes every registered system through the one shared DomainSystemView', () => {
    const registry = source('src/features/devtools/devtool-registry.ts')
    const view = source('src/views/devtools-view.tsx')
    const ids = [...registry.matchAll(/tool\('([^']+)'/g)].map(match => match[1])

    expect(ids).toHaveLength(33)
    expect(new Set(ids).size).toBe(33)
    expect(view).toContain('import { DomainSystemView } from \'@/features/devtools/domain/domain-system-view\'')
    expect(view).not.toMatch(/MapToolView|PlannedToolView|NpcToolView|SpecializedDomain/)
    expect(view).not.toMatch(/switch\s*\(\s*(?:activeToolId|tool\.id)/)
  })

  it('loads only the current domain file projection and opens source directly', () => {
    const view = source('src/features/devtools/domain/domain-system-view.tsx')

    expect(view).toContain('queryDomainFiles(')
    expect(view).toContain('openDomainText(')
    expect(view).not.toContain('queryUnclaimedDomainFiles')
    expect(view).not.toContain('queryDomainResources')
    expect(view).not.toContain('getDomainResource')
    expect(view).not.toContain('resolveDomainDependencies')
    expect(view).not.toContain('validateDomainSystem')
    expect(view).not.toContain('<ResourceRenderer')
    expect(view).not.toMatch(/type ResourceTab\s*=/)
    expect(view).toContain('<DomainFileSidebar')
    expect(view).toContain('<DirectoryTree')
    expect(view).toContain('<FileSourceWorkspace')
    expect(view).toContain('buildFileTree(')
    expect(view).toContain('file.ownership === \'owned\'')
    expect(view).toContain('file.ownership === \'shared\'')
  })

  it('does not evaluate XLS rows before a sheet has loaded', () => {
    const view = source('src/features/devtools/domain/domain-system-view.tsx')
    const loadingGuard = view.indexOf('if (sheetLoading)')
    const missingGuard = view.indexOf('if (sheetError || !sheet)')
    const preview = view.indexOf('xlsTsvPreview(sheet)')

    expect(loadingGuard).toBeGreaterThan(-1)
    expect(missingGuard).toBeGreaterThan(loadingGuard)
    expect(preview).toBeGreaterThan(missingGuard)
    expect(view).not.toContain('xlsTsvPreview(sheet!)')
  })

  it('keeps owned-selector evidence in every domain package', () => {
    const packRoot = new URL('src-tauri/resources/mir3-domain-packs/', root)
    const registry = JSON.parse(source('src-tauri/resources/mir3-domain-packs/registry.json')) as {
      packs: Array<{ systemId: string }>
    }
    const directories = readdirSync(packRoot, { withFileTypes: true })
      .filter(entry => entry.isDirectory())
      .map(entry => entry.name)
      .sort()

    expect(registry.packs).toHaveLength(33)
    expect(directories).toEqual(registry.packs.map(pack => pack.systemId).sort())
    for (const { systemId } of registry.packs) {
      const manifest = JSON.parse(source(`src-tauri/resources/mir3-domain-packs/${systemId}/domain.json`)) as {
        systemId: string
        fileProjection?: {
          ownedSelectors?: string[]
          contentFingerprints?: unknown[]
          unknownFormatPolicy?: string
        }
      }
      expect(manifest.systemId).toBe(systemId)
      expect(manifest.fileProjection?.ownedSelectors?.length).toBeGreaterThan(0)
      expect(manifest.fileProjection?.contentFingerprints?.length).toBeGreaterThan(0)
      expect(manifest.fileProjection?.unknownFormatPolicy).toBe('readonly')
    }
  })

  it('issues a system conversation lease with only the current system writable', () => {
    const panel = source('src/features/system-ai/system-ai-panel.tsx')
    const scope = source('src/features/system-ai/system-task-scope.ts')
    const mcp = source('src-tauri/crates/mir3-mcp/src/main.rs')

    expect(panel).toContain('buildSystemTaskScopeContract(manifest, taskId, draftId, manifests)')
    expect(panel).toMatch(/issueTaskScope\([\s\S]*?contract\.readSystems,\s*\[contract\.systemId\]/)
    expect(panel).toMatch(/writeSystems=\$\{manifest\.systemId\}/)
    expect(scope).toContain('lease.writeSystems.length !== 1')
    expect(scope).toContain('lease.writeSystems[0] !== contract.systemId')
    expect(mcp).toContain('fn capability_invoke_rejects_tampering_scope_escalation_and_revision_spoofing()')
    expect(mcp).toContain('"systemId":"shop"')
    expect(mcp).toContain('assert!(tool_error(&scope_escalation).contains("SCOPE_"))')
  })
})
