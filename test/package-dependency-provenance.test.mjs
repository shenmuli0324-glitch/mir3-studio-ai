import { chmodSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  assertDependencyInputTree,
  hashDependencyInputTree,
} from '../scripts/package-dependency-provenance.mjs'

describe('native package dependency provenance', () => {
  it('hashes pnpm content, symlinks, file modes and dependency caches', () => {
    const root = createFixture()
    try {
      const baseline = hashDependencyInputTree(root)
      expect(baseline.fileCount).toBe(4)
      expect(baseline.symlinkCount).toBe(1)
      expect(baseline.excludedRelativePaths).toEqual([])
      expect(() => assertDependencyInputTree(baseline, hashDependencyInputTree(root))).not.toThrow()
      writeFileSync(join(root, '.cache', 'volatile.json'), 'changed')
      expect(hashDependencyInputTree(root).sha256).not.toBe(baseline.sha256)
      writeFileSync(join(root, '.cache', 'volatile.json'), 'initial')

      writeFileSync(join(root, '.pnpm', 'pkg', 'index.js'), 'export default 2\n')
      const contentChanged = hashDependencyInputTree(root)
      expect(contentChanged.sha256).not.toBe(baseline.sha256)
      expect(() => assertDependencyInputTree(baseline, contentChanged))
        .toThrowError(/PACKAGE_DEPENDENCY_INPUT_CHANGED/u)

      writeFileSync(join(root, '.pnpm', 'pkg', 'index.js'), 'export default 1\n')
      chmodSync(join(root, '.pnpm', 'pkg', 'index.js'), 0o755)
      expect(hashDependencyInputTree(root).sha256).not.toBe(baseline.sha256)

      symlinkSync('../outside-dependency', join(root, 'escape'))
      expect(() => hashDependencyInputTree(root))
        .toThrowError(/PACKAGE_DEPENDENCY_SYMLINK_ESCAPE/u)
      unlinkSync(join(root, 'escape'))
    }
    finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('fails when the embedded dependency identity is missing', () => {
    const root = createFixture()
    try {
      expect(() => assertDependencyInputTree(undefined, hashDependencyInputTree(root)))
        .toThrowError(/PACKAGE_DEPENDENCY_INPUT_CHANGED/u)
    }
    finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('rejects a symlinked dependency root before traversing outside the workspace', () => {
    const root = createFixture()
    const parent = mkdtempSync(join(tmpdir(), 'mir3-package-root-link-'))
    const linkedRoot = join(parent, 'node_modules')
    try {
      symlinkSync(root, linkedRoot)
      expect(() => hashDependencyInputTree(linkedRoot))
        .toThrowError(/PACKAGE_DEPENDENCY_ROOT_SYMLINK_FORBIDDEN/u)
    }
    finally {
      rmSync(parent, { recursive: true, force: true })
      rmSync(root, { recursive: true, force: true })
    }
  })
})

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'mir3-package-dependencies-'))
  mkdirSync(join(root, '.pnpm', 'pkg'), { recursive: true })
  mkdirSync(join(root, '.cache'), { recursive: true })
  mkdirSync(join(root, '.vite'), { recursive: true })
  writeFileSync(join(root, '.pnpm', 'pkg', 'index.js'), 'export default 1\n', { mode: 0o644 })
  writeFileSync(join(root, '.modules.yaml'), 'layoutVersion: 5\n')
  writeFileSync(join(root, '.cache', 'volatile.json'), 'initial')
  writeFileSync(join(root, '.vite', 'manifest.json'), 'initial')
  symlinkSync('.pnpm/pkg', join(root, 'pkg'))
  return root
}
