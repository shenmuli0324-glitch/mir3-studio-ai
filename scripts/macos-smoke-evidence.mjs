import { Buffer } from 'node:buffer'
import { createHash, randomUUID } from 'node:crypto'
import { closeSync, fsyncSync, openSync, readFileSync, readSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import process from 'node:process'

export const REQUIRED_CORE_CHECKS = [
  'bridge-v2',
  'ordinary-session',
  'archived-system-session',
  'mcp-sidecar',
  'domain-capability',
]

export function writeMacosSmokeEvidence({
  appVersion,
  coreCanary,
  dmgPath,
  evidencePath,
  expectedDmgRelativePath,
  packCount,
  passedAt,
  provenancePath,
}) {
  const provenanceBytes = readRequiredFile(provenancePath, 'SMOKE_EVIDENCE_PROVENANCE_READ_FAILED')
  const provenance = parseJson(provenanceBytes, 'SMOKE_EVIDENCE_PROVENANCE_INVALID')
  const dmg = hashRequiredFile(dmgPath, 'SMOKE_EVIDENCE_DMG_READ_FAILED')
  const evidence = createMacosSmokeEvidence({
    appVersion,
    coreCanary,
    dmgSha256: dmg.sha256,
    dmgSize: dmg.size,
    expectedDmgRelativePath,
    packCount,
    passedAt,
    provenance,
    provenanceSha256: sha256(provenanceBytes),
  })
  writeJsonAtomic(evidencePath, evidence)
  return evidence
}

export function createMacosSmokeEvidence({
  appVersion,
  coreCanary,
  dmgSha256,
  dmgSize,
  expectedDmgRelativePath,
  packCount,
  passedAt,
  provenance,
  provenanceSha256,
}) {
  assertNonEmptyString(appVersion, 'SMOKE_EVIDENCE_APP_VERSION_INVALID')
  assertSha256(dmgSha256, 'SMOKE_EVIDENCE_DMG_HASH_INVALID')
  assertSha256(provenanceSha256, 'SMOKE_EVIDENCE_PROVENANCE_HASH_INVALID')
  if (!Number.isSafeInteger(dmgSize) || dmgSize < 1)
    throw new Error('SMOKE_EVIDENCE_DMG_SIZE_INVALID: DMG size must be a positive safe integer')
  if (!Number.isSafeInteger(passedAt) || passedAt < 1)
    throw new Error('SMOKE_EVIDENCE_PASSED_AT_INVALID: passedAt must be a positive safe integer')
  if (packCount !== 33)
    throw new Error(`SMOKE_EVIDENCE_PACK_COUNT_INVALID: expected 33 initialized domain packs, got ${packCount}`)

  assertPackageProvenance({ appVersion, dmgSha256, dmgSize, expectedDmgRelativePath, provenance })
  assertCoreCanary({ appVersion, coreCanary, passedAt })

  return {
    schemaVersion: 1,
    appVersion,
    buildId: provenance.build.buildId,
    sourceCommit: provenance.build.git.commit,
    sourceTree: provenance.build.git.tree,
    dmgSha256,
    provenanceSha256,
    coreTag: coreCanary.coreTag,
    coreCommit: coreCanary.coreCommit,
    protocolVersion: coreCanary.protocolVersion,
    requiredChecks: [...REQUIRED_CORE_CHECKS],
    domainPackCount: packCount,
    passedAt,
  }
}

function assertPackageProvenance({ appVersion, dmgSha256, dmgSize, expectedDmgRelativePath, provenance }) {
  if (!provenance || typeof provenance !== 'object' || provenance.schemaVersion !== 1)
    throw new Error('SMOKE_EVIDENCE_PROVENANCE_INVALID: package provenance schemaVersion must be 1')
  if (provenance.build?.schemaVersion !== 1 || provenance.build?.versions?.product !== appVersion)
    throw new Error('SMOKE_EVIDENCE_VERSION_MISMATCH: package provenance does not match the smoke app version')
  if (!validGitObjectId(provenance.build?.git?.commit) || !validGitObjectId(provenance.build?.git?.tree))
    throw new Error('SMOKE_EVIDENCE_SOURCE_INVALID: package provenance source commit/tree is invalid')
  assertEmbeddedBuildIdentity(provenance.build)

  const artifact = provenance.artifacts?.dmg
  if (artifact?.relativePath !== expectedDmgRelativePath)
    throw new Error('SMOKE_EVIDENCE_DMG_PATH_MISMATCH: package provenance points to a different DMG')
  if (artifact.sha256 !== dmgSha256 || artifact.size !== dmgSize)
    throw new Error('SMOKE_EVIDENCE_DMG_MISMATCH: current DMG content differs from package provenance')
  if (provenance.verification?.codesign !== 'passed' || provenance.verification?.diskImage !== 'passed')
    throw new Error('SMOKE_EVIDENCE_PACKAGE_UNVERIFIED: package signature or disk image verification is missing')
}

function assertEmbeddedBuildIdentity(build) {
  const { buildId, ...identity } = build
  if (!isSha256(buildId) || sha256(JSON.stringify(identity)) !== buildId)
    throw new Error('SMOKE_EVIDENCE_BUILD_ID_MISMATCH: package build identity was modified')
}

function assertCoreCanary({ appVersion, coreCanary, passedAt }) {
  if (!coreCanary
    || coreCanary.schemaVersion !== 1
    || coreCanary.status !== 'passed'
    || coreCanary.appVersion !== appVersion
    || coreCanary.protocolVersion !== 2) {
    throw new Error('SMOKE_EVIDENCE_CORE_CANARY_INVALID: durable Core canary identity is invalid')
  }
  assertNonEmptyString(coreCanary.coreTag, 'SMOKE_EVIDENCE_CORE_TAG_INVALID')
  assertNonEmptyString(coreCanary.coreCommit, 'SMOKE_EVIDENCE_CORE_COMMIT_INVALID')
  if (!Number.isSafeInteger(coreCanary.passedAt) || coreCanary.passedAt < 1 || coreCanary.passedAt > passedAt)
    throw new Error('SMOKE_EVIDENCE_CORE_TIME_INVALID: durable Core canary time is invalid')
  if (!Array.isArray(coreCanary.checks)
    || REQUIRED_CORE_CHECKS.some(check => !coreCanary.checks.includes(check))) {
    throw new Error('SMOKE_EVIDENCE_CORE_CHECKS_MISSING: durable Core canary is missing required runtime gates')
  }
}

function writeJsonAtomic(path, value) {
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`
  let descriptor
  try {
    descriptor = openSync(temporary, 'wx', 0o644)
    writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`, { encoding: 'utf8' })
    fsyncSync(descriptor)
    closeSync(descriptor)
    descriptor = undefined
    renameSync(temporary, path)
  }
  catch (error) {
    if (descriptor !== undefined)
      closeSync(descriptor)
    rmSync(temporary, { force: true })
    throw new Error(`SMOKE_EVIDENCE_WRITE_FAILED: ${error}`)
  }
}

function readRequiredFile(path, code) {
  try {
    return readFileSync(path)
  }
  catch (error) {
    throw new Error(`${code}: ${error}`)
  }
}

function parseJson(bytes, code) {
  try {
    return JSON.parse(bytes.toString('utf8'))
  }
  catch (error) {
    throw new Error(`${code}: ${error}`)
  }
}

function hashRequiredFile(path, code) {
  let descriptor
  try {
    const size = statSync(path).size
    descriptor = openSync(path, 'r')
    const hasher = createHash('sha256')
    const buffer = Buffer.allocUnsafe(1024 * 1024)
    while (true) {
      const bytesRead = readSync(descriptor, buffer, 0, buffer.length, null)
      if (bytesRead === 0)
        break
      hasher.update(buffer.subarray(0, bytesRead))
    }
    closeSync(descriptor)
    return { sha256: hasher.digest('hex'), size }
  }
  catch (error) {
    if (descriptor !== undefined)
      closeSync(descriptor)
    throw new Error(`${code}: ${error}`)
  }
}

function assertNonEmptyString(value, code) {
  if (typeof value !== 'string' || value.length === 0)
    throw new Error(`${code}: expected a non-empty string`)
}

function assertSha256(value, code) {
  if (!isSha256(value))
    throw new Error(`${code}: expected a lowercase SHA-256 digest`)
}

function isSha256(value) {
  return typeof value === 'string' && /^[a-f0-9]{64}$/u.test(value)
}

function validGitObjectId(value) {
  return typeof value === 'string' && /^[a-f0-9]{40,64}$/u.test(value)
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}
