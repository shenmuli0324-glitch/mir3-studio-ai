import { Buffer } from 'node:buffer'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { copyFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'
import process from 'node:process'

const repository = resolve(import.meta.dirname, '..')
const lockPath = join(repository, 'runtime-baseline.lock.json')
const product = JSON.parse(readFileSync(join(repository, 'package.json'), 'utf8'))
const outputDir = join(repository, 'src-tauri', 'resources', 'runtime-baseline')
const cacheDir = join(repository, '.cache', 'mir3-runtime')
const lock = JSON.parse(readFileSync(lockPath, 'utf8'))

function rustHost() {
  const result = spawnSync('rustc', ['-vV'], { encoding: 'utf8' })
  if (result.status !== 0)
    throw new Error(result.stderr || 'Unable to inspect rustc host target')
  return result.stdout.split('\n').find(line => line.startsWith('host:'))?.slice(5).trim()
}

const target = process.env.TAURI_ENV_TARGET_TRIPLE || rustHost()
const platformEntry = Object.entries(lock.platforms).find(([, value]) => value.target === target)
if (!platformEntry)
  throw new Error(`No MIR3 runtime baseline is locked for target ${target}`)

const [platform, platformLock] = platformEntry
const allowUnvalidated = process.env.MIR3_BASELINE_ALLOW_UNVALIDATED === '1'
if (lock.policy.requireApprovedPlatformForRelease && platformLock.validation !== 'approved' && !allowUnvalidated) {
  throw new Error(
    `Runtime baseline ${lock.baselineId} for ${platform} is ${platformLock.validation}; `
    + 'release packaging requires approved validation. Use MIR3_BASELINE_ALLOW_UNVALIDATED=1 only for a development test installer.',
  )
}

function digest(buffer) {
  return createHash('sha256').update(buffer).digest('hex')
}

async function ensureArtifact(label, artifact) {
  const filename = basename(new URL(artifact.url).pathname)
  const cached = join(cacheDir, `${artifact.sha256}-${filename}`)
  mkdirSync(cacheDir, { recursive: true })
  if (existsSync(cached)) {
    const buffer = readFileSync(cached)
    if (digest(buffer) !== artifact.sha256)
      rmSync(cached)
  }
  if (!existsSync(cached)) {
    process.stdout.write(`Downloading locked ${label} baseline: ${artifact.url}\n`)
    const response = await fetch(artifact.url, {
      redirect: 'follow',
      headers: { 'user-agent': `MIR3-Studio-AI-baseline-builder/${product.version}` },
    })
    if (!response.ok)
      throw new Error(`Failed to download ${label}: HTTP ${response.status}`)
    const buffer = Buffer.from(await response.arrayBuffer())
    const actual = digest(buffer)
    if (actual !== artifact.sha256)
      throw new Error(`${label} SHA-256 mismatch: expected ${artifact.sha256}, got ${actual}`)
    const temporary = `${cached}.${process.pid}.tmp`
    mkdirSync(dirname(temporary), { recursive: true })
    writeFileSync(temporary, buffer)
    renameSync(temporary, cached)
  }
  const destination = join(outputDir, filename)
  copyFileSync(cached, destination)
  return {
    archive: filename,
    sha256: artifact.sha256,
    size: statSync(destination).size,
    source: artifact.url,
  }
}

rmSync(outputDir, { recursive: true, force: true })
mkdirSync(outputDir, { recursive: true })
writeFileSync(join(outputDir, '.gitkeep'), '\n')

const artifacts = {
  node: await ensureArtifact('Node.js', platformLock.node),
  core: await ensureArtifact('MIR3 AI Core', platformLock.core),
  pnpm: await ensureArtifact('pnpm', lock.pnpm),
}

const manifest = {
  schemaVersion: lock.schemaVersion,
  baselineId: lock.baselineId,
  platform,
  target,
  validation: platformLock.validation,
  core: lock.core,
  node: { version: lock.node.version },
  pnpm: { version: lock.pnpm.version },
  artifacts,
}
writeFileSync(join(outputDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
process.stdout.write(
  `Prepared MIR3 runtime baseline ${lock.baselineId} for ${platform} (${platformLock.validation})\n`,
)
