import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  checkGeneratedDomainPacks,
  createGeneratedDomainPackGroups,
  writeGeneratedDomainPacks,
} from '../scripts/generate-domain-packs.mjs'

const temporaryRoots = []

afterEach(() => {
  for (const root of temporaryRoots.splice(0))
    rmSync(root, { recursive: true, force: true })
})

describe('domain pack generated ownership', () => {
  it('produces identical managed output on consecutive generations', () => {
    const first = serializeGroups(createGeneratedDomainPackGroups())
    const second = serializeGroups(createGeneratedDomainPackGroups())
    expect(second).toEqual(first)
  })

  it('checks generated files without writing them and rejects managed drift', () => {
    const roots = fixtureRoots()
    writeGeneratedDomainPacks(roots)
    const before = snapshotTree(roots.base)
    expect(checkGeneratedDomainPacks(roots)).toBe(274)
    expect(snapshotTree(roots.base)).toEqual(before)

    for (const relativePath of [
      'packs/registry.json',
      'packs/map/README.md',
      'sdk/domain.json',
      'sdk/.generated-ownership.json',
    ]) {
      const target = join(roots.base, relativePath)
      const original = readFileSync(target, 'utf8')
      writeFileSync(target, `${original}\nmanual tamper\n`)
      expect(() => checkGeneratedDomainPacks(roots)).toThrow('DOMAIN_PACK_GENERATED_DRIFT')
      writeFileSync(target, original)
    }
  })

  it('leaves manual files untouched and refuses to adopt files removed from ownership', () => {
    const roots = fixtureRoots()
    writeGeneratedDomainPacks(roots)
    const manualPack = join(roots.packRoot, 'MANUAL-NOTES.md')
    const manualSdkReadme = join(roots.sdkExampleRoot, 'README.md')
    writeFileSync(manualPack, 'manual pack notes\n')
    writeFileSync(manualSdkReadme, 'manual SDK example guide\n')
    writeGeneratedDomainPacks(roots)
    expect(readFileSync(manualPack, 'utf8')).toBe('manual pack notes\n')
    expect(readFileSync(manualSdkReadme, 'utf8')).toBe('manual SDK example guide\n')

    const ownershipPath = join(roots.packRoot, '.generated-ownership.json')
    const ownership = JSON.parse(readFileSync(ownershipPath, 'utf8'))
    ownership.files = ownership.files.filter(path => path !== 'registry.json')
    writeFileSync(ownershipPath, `${JSON.stringify(ownership, null, 2)}\n`)
    expect(() => writeGeneratedDomainPacks(roots)).toThrow('GENERATED_FILE_UNMANAGED')
  })
})

function fixtureRoots() {
  const base = mkdtempSync(join(tmpdir(), 'mir3-domain-generation-'))
  temporaryRoots.push(base)
  const packRoot = join(base, 'packs')
  const sdkExampleRoot = join(base, 'sdk')
  mkdirSync(packRoot, { recursive: true })
  mkdirSync(sdkExampleRoot, { recursive: true })
  return { base, packRoot, sdkExampleRoot }
}

function serializeGroups(groups) {
  return groups.map(group => ({ name: group.name, files: [...group.files.entries()] }))
}

function snapshotTree(root) {
  const snapshot = {}
  for (const path of walkFiles(root)) {
    const stat = statSync(path, { bigint: true })
    snapshot[path.slice(root.length + 1)] = {
      content: readFileSync(path, 'utf8'),
      modifiedAt: stat.mtimeNs.toString(),
    }
  }
  return snapshot
}

function walkFiles(root) {
  const files = []
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name)
    if (entry.isDirectory())
      files.push(...walkFiles(path))
    else if (entry.isFile())
      files.push(path)
  }
  return files.sort()
}
