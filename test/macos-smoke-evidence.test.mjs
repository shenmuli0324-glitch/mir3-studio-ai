import { Buffer } from 'node:buffer'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { REQUIRED_CORE_CHECKS, writeMacosSmokeEvidence } from '../scripts/macos-smoke-evidence.mjs'

describe('durable macOS native smoke evidence', () => {
  it('atomically overwrites evidence bound to the package without copying private runtime data', () => {
    const fixture = createFixture()
    try {
      const first = writeFixtureEvidence(fixture, 1_700_000_100_000)
      const second = writeFixtureEvidence(fixture, 1_700_000_200_000)
      const persistedBytes = readFileSync(fixture.evidencePath)
      const persisted = JSON.parse(persistedBytes.toString('utf8'))

      expect(first.passedAt).toBe(1_700_000_100_000)
      expect(second).toEqual(persisted)
      expect(persisted).toEqual({
        schemaVersion: 1,
        appVersion: '9.8.7',
        buildId: JSON.parse(readFileSync(fixture.provenancePath, 'utf8')).build.buildId,
        sourceCommit: 'a'.repeat(40),
        sourceTree: 'b'.repeat(40),
        dmgSha256: sha256(fixture.dmgBytes),
        provenanceSha256: sha256(readFileSync(fixture.provenancePath)),
        coreTag: 'v1.2.3',
        coreCommit: 'c'.repeat(40),
        protocolVersion: 2,
        requiredChecks: REQUIRED_CORE_CHECKS,
        domainPackCount: 33,
        passedAt: 1_700_000_200_000,
      })
      expect(persistedBytes.toString('utf8')).not.toMatch(/private-user|mir3-private-root|3081/u)
      expect(readdirSync(fixture.root).sort()).toEqual(['package.dmg', 'package.dmg.provenance.json', 'package.dmg.smoke.json'])
    }
    finally {
      rmSync(fixture.root, { recursive: true, force: true })
    }
  })

  it('fails closed on DMG tampering and preserves the previous passed evidence', () => {
    const fixture = createFixture()
    try {
      writeFixtureEvidence(fixture, 1_700_000_100_000)
      const previousEvidence = readFileSync(fixture.evidencePath)
      writeFileSync(fixture.dmgPath, 'tampered-dmg')

      expect(() => writeFixtureEvidence(fixture, 1_700_000_200_000)).toThrowError(
        /SMOKE_EVIDENCE_DMG_MISMATCH/u,
      )
      expect(readFileSync(fixture.evidencePath)).toEqual(previousEvidence)
    }
    finally {
      rmSync(fixture.root, { recursive: true, force: true })
    }
  })

  it('fails closed when package provenance identity is modified', () => {
    const fixture = createFixture()
    try {
      const provenance = JSON.parse(readFileSync(fixture.provenancePath, 'utf8'))
      provenance.build.git.commit = 'd'.repeat(40)
      writeFileSync(fixture.provenancePath, `${JSON.stringify(provenance, null, 2)}\n`)

      expect(() => writeFixtureEvidence(fixture, 1_700_000_100_000)).toThrowError(
        /SMOKE_EVIDENCE_BUILD_ID_MISMATCH/u,
      )
      expect(() => readFileSync(fixture.evidencePath)).toThrow()
    }
    finally {
      rmSync(fixture.root, { recursive: true, force: true })
    }
  })
})

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), 'mir3-macos-smoke-evidence-'))
  mkdirSync(root, { recursive: true })
  const dmgPath = join(root, 'package.dmg')
  const provenancePath = `${dmgPath}.provenance.json`
  const evidencePath = `${dmgPath}.smoke.json`
  const dmgBytes = Buffer.from('signed-dmg-fixture')
  writeFileSync(dmgPath, dmgBytes)
  const buildIdentity = {
    schemaVersion: 1,
    git: {
      commit: 'a'.repeat(40),
      tree: 'b'.repeat(40),
    },
    versions: {
      product: '9.8.7',
    },
  }
  const provenance = {
    schemaVersion: 1,
    build: {
      ...buildIdentity,
      buildId: sha256(JSON.stringify(buildIdentity)),
    },
    artifacts: {
      dmg: {
        relativePath: 'bundle/package.dmg',
        sha256: sha256(dmgBytes),
        size: dmgBytes.length,
      },
    },
    verification: {
      codesign: 'passed',
      diskImage: 'passed',
    },
  }
  writeFileSync(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`)
  return { dmgBytes, dmgPath, evidencePath, provenancePath, root }
}

function writeFixtureEvidence(fixture, passedAt) {
  return writeMacosSmokeEvidence({
    appVersion: '9.8.7',
    coreCanary: {
      schemaVersion: 1,
      status: 'passed',
      appVersion: '9.8.7',
      coreTag: 'v1.2.3',
      coreCommit: 'c'.repeat(40),
      protocolVersion: 2,
      checks: [...REQUIRED_CORE_CHECKS],
      passedAt: 1_700_000_000_000,
      privateRoot: '/tmp/mir3-private-root',
      port: 3081,
      username: 'private-user',
    },
    dmgPath: fixture.dmgPath,
    evidencePath: fixture.evidencePath,
    expectedDmgRelativePath: 'bundle/package.dmg',
    packCount: 33,
    passedAt,
    provenancePath: fixture.provenancePath,
  })
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}
