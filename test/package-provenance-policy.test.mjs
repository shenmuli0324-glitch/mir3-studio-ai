import { describe, expect, it } from 'vitest'
import {
  inspectPackageSourceStatus,
  isAllowedUntrackedPath,
  packageIgnoredScanPathspecs,
} from '../scripts/package-provenance-policy.mjs'

describe('native package provenance source policy', () => {
  it('allows only explicit user artifacts and generated package outputs', () => {
    expect(isAllowedUntrackedPath('artifacts/')).toBe(true)
    expect(isAllowedUntrackedPath('artifacts/user-report.json')).toBe(true)
    expect(isAllowedUntrackedPath('.cache/mir3-runtime/locked-runtime.tgz')).toBe(true)
    expect(isAllowedUntrackedPath('dist/index.html')).toBe(true)
    expect(isAllowedUntrackedPath('node_modules/vite/package.json')).toBe(true)
    expect(isAllowedUntrackedPath('src-tauri/binaries/mir3-mcp-aarch64-apple-darwin')).toBe(true)
    expect(isAllowedUntrackedPath('src-tauri/gen/schemas/desktop-schema.json')).toBe(true)
    expect(isAllowedUntrackedPath('src-tauri/resources/runtime-baseline/manifest.json')).toBe(true)
    expect(isAllowedUntrackedPath('src-tauri/resources/build-provenance.json')).toBe(true)
    expect(isAllowedUntrackedPath('src-tauri/target/release/bundle/app.dmg')).toBe(true)
    expect(isAllowedUntrackedPath('artifacts-copy/injected.json')).toBe(false)
    expect(isAllowedUntrackedPath('src-tauri/resources/injected.json')).toBe(false)
    expect(isAllowedUntrackedPath('src/generated.ts')).toBe(false)
  })

  it('parses NUL-delimited status and fails closed for untracked build inputs', () => {
    const status = inspectPackageSourceStatus([
      '?? artifacts/',
      '?? src-tauri/resources/build-provenance.json',
      '?? src-tauri/resources/extra-domain.json',
      '?? src/extra-entry.ts',
      '!! .env.production.local',
      '!! scripts/release-hook.local',
      '!! src-tauri/resources/node/injected-runtime',
      '!! src-tauri/resources/runtime-baseline/manifest.json',
      '!! node_modules/vite/package.json',
      ' M package.json',
      '',
    ].join('\0'))
    expect(status.trackedChanges).toEqual([' M package.json'])
    expect(status.unsafeUntracked).toEqual([
      'src-tauri/resources/extra-domain.json',
      'src/extra-entry.ts',
      '.env.production.local',
      'scripts/release-hook.local',
      'src-tauri/resources/node/injected-runtime',
    ])
  })

  it('scans the whole repository while excluding only explicit reproducible roots', () => {
    const pathspecs = packageIgnoredScanPathspecs()
    expect(pathspecs[0]).toBe('.')
    expect(pathspecs).toContain(':(exclude)artifacts')
    expect(pathspecs).toContain(':(exclude)node_modules')
    expect(pathspecs).toContain(':(exclude)src-tauri/resources/build-provenance.json')
    expect(pathspecs).not.toContain(':(exclude).env.production.local')
    expect(pathspecs).not.toContain(':(exclude)scripts')
    expect(pathspecs).not.toContain(':(exclude)src')
  })
})
