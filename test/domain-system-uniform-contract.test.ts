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

  it('loads only the current domain file projection through the shared Working Copy', () => {
    const view = source('src/features/devtools/domain/domain-system-view.tsx')

    expect(view).toContain('queryDomainFiles(')
    expect(view).toContain('openDomainWorkingText(')
    expect(view).toContain('openDomainWorkingCopy(')
    expect(view).toContain('saveDomainWorkingCopy(')
    expect(view).toContain('restoreDomainSaveNode(')
    expect(view).toContain('queryUnclaimedDomainFiles(project!.id, deferredSearch, 100)')
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
    expect(view).toContain('queryDomainReferences(')
    expect(view).toContain('file.ownership === \'shared\'')
  })

  it('loads the complete XLS working copy before rendering editable cells', () => {
    const view = source('src/features/devtools/domain/domain-system-view.tsx')
    const editor = source('src/features/devtools/domain/domain-workbook-editor.tsx')
    const loadingGuard = editor.indexOf('if (loading)')
    const missingGuard = editor.indexOf('if (error || !data)')
    const viewport = editor.indexOf('domainWorkbookViewport(sheet, rowPage, columnPage)')

    expect(loadingGuard).toBeGreaterThan(-1)
    expect(missingGuard).toBeGreaterThan(loadingGuard)
    expect(viewport).toBeGreaterThan(missingGuard)
    expect(view).toContain('loadDomainWorkingWorkbook(project!.id, selectedFile!.path, activeWorkingCopyId)')
    expect(view).toContain('workingCopy?.revision ?? 0')
    expect(editor).toContain('readOnly={!editable || busy}')
    expect(view).toContain('updateXlsEdits(unsyncedXlsEdits(xlsEditsRef.current))')
    expect(view).toContain('savingFlowRef.current = true')
  })

  it('keeps evidence-backed exact bindings and permits intentionally empty systems', () => {
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
          bindings?: Array<{ id: string, pathPattern: string, evidence: { kind: string, ref: string, version: string } }>
          unknownFormatPolicy?: string
        }
      }
      expect(manifest.systemId).toBe(systemId)
      expect(manifest.fileProjection?.ownedSelectors).toEqual([])
      expect(manifest.fileProjection?.contentFingerprints).toEqual([])
      expect(manifest.fileProjection?.bindings).toBeInstanceOf(Array)
      for (const binding of manifest.fileProjection?.bindings ?? []) {
        expect(binding.id).not.toBe('')
        expect(binding.pathPattern).not.toBe('')
        expect(['officialDoc', 'officialForum']).toContain(binding.evidence.kind)
        expect(binding.evidence.ref).not.toBe('')
        expect(binding.evidence.version).not.toBe('')
      }
      expect(manifest.fileProjection?.unknownFormatPolicy).toBe('readonly')
    }
    const equipment = JSON.parse(source('src-tauri/resources/mir3-domain-packs/equipment/domain.json'))
    expect(equipment.fileProjection.bindings.map((binding: { pathPattern: string }) => binding.pathPattern)).toContain('cfg_equip.xls')
    const rebirth = JSON.parse(source('src-tauri/resources/mir3-domain-packs/rebirth/domain.json'))
    expect(rebirth.fileProjection.bindings).toEqual([])
  })

  it('issues a system conversation lease with only the current system writable', () => {
    const panel = source('src/features/system-ai/system-ai-panel.tsx')
    const scope = source('src/features/system-ai/system-task-scope.ts')
    const mcp = source('src-tauri/crates/mir3-mcp/src/main.rs')

    expect(panel).toContain('buildSystemTaskScopeContract(manifest, taskId, activeWorkingCopyId, manifests)')
    expect(panel).toContain('const prepared = await ensureWorkingCopy()')
    expect(panel).toMatch(/issueTaskScope\([\s\S]*?contract\.readSystems,\s*\[contract\.systemId\]/)
    expect(panel).toMatch(/writeSystems=\$\{manifest\.systemId\}/)
    expect(scope).toContain('lease.writeSystems.length !== 1')
    expect(scope).toContain('lease.writeSystems[0] !== contract.systemId')
    expect(mcp).toContain('fn capability_invoke_rejects_tampering_scope_escalation_and_revision_spoofing()')
    expect(mcp).toContain('"systemId":"shop"')
    expect(mcp).toContain('assert!(tool_error(&scope_escalation).contains("SCOPE_"))')
  })
})
