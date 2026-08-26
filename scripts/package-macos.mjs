import { Buffer } from 'node:buffer'
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { closeSync, lstatSync, openSync, readdirSync, readFileSync, readlinkSync, readSync, statSync, writeFileSync } from 'node:fs'
import { join, relative, resolve, sep } from 'node:path'
import process from 'node:process'
import { assertDependencyInputTree, hashDependencyInputTree } from './package-dependency-provenance.mjs'
import { inspectPackageSourceStatus, packageIgnoredScanPathspecs } from './package-provenance-policy.mjs'

const root = resolve(import.meta.dirname, '..')
const product = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const tauri = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))
const corePlugin = JSON.parse(readFileSync(join(root, 'src-tauri', 'resources', 'mir3-core-plugin', 'package.json'), 'utf8'))
const domainRegistry = JSON.parse(readFileSync(join(root, 'src-tauri', 'resources', 'mir3-domain-packs', 'registry.json'), 'utf8'))

if (process.platform !== 'darwin')
  throw new Error('package:mac can only run on macOS')

const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64'
const bundleRoot = join(root, 'src-tauri', 'target', 'release', 'bundle')
const appPath = join(bundleRoot, 'macos', `${tauri.productName}.app`)
const dmgPath = join(bundleRoot, 'dmg', `${tauri.productName}_${product.version}_${architecture}.dmg`)
const embeddedProvenanceSource = join(root, 'src-tauri', 'resources', 'build-provenance.json')
const embeddedProvenancePath = join(appPath, 'Contents', 'Resources', 'resources', 'build-provenance.json')
const provenancePath = `${dmgPath}.provenance.json`
const dependencyRoot = join(root, 'node_modules')
const verifyOnly = process.argv.includes('--verify-only')
const environment = {
  ...process.env,
  APPLE_SIGNING_IDENTITY: process.env.APPLE_SIGNING_IDENTITY || '-',
}

const source = assertCleanCommittedSource()
const dependencyInput = hashDependencyInputTree(dependencyRoot)
let embeddedProvenance
if (!verifyOnly) {
  embeddedProvenance = createEmbeddedProvenance(source, dependencyInput)
  writeFileSync(embeddedProvenanceSource, `${JSON.stringify(embeddedProvenance, null, 2)}\n`, { mode: 0o644 })
  stopRunningBundleApp(appPath)
  runPnpm(['tauri', 'build', '--bundles', 'app,dmg'], environment)
}
else {
  embeddedProvenance = readJson(embeddedProvenancePath, 'PACKAGED_PROVENANCE_MISSING')
}
assertSourceUnchanged(source)
assertDependencyInputTree(dependencyInput, hashDependencyInputTree(dependencyRoot))
run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath])
run('hdiutil', ['verify', dmgPath])

const signature = combinedOutput('codesign', ['-dv', '--verbose=4', appPath])
const dmgSha256 = output('shasum', ['-a', '256', dmgPath]).split(/\s+/)[0]
const appSize = output('du', ['-sh', appPath]).split(/\s+/)[0]
const dmgSize = statSync(dmgPath).size
const notarizationConfigured = hasNotarizationCredentials(environment)
const packagedProvenance = readJson(embeddedProvenancePath, 'PACKAGED_PROVENANCE_MISSING')
assertEmbeddedProvenance(embeddedProvenance, packagedProvenance, source, dependencyInput)
const appTree = hashTree(appPath)
const signing = signingState(signature)
const notarization = {
  credentialsConfigured: notarizationConfigured,
  appStaple: stapleState(appPath),
  dmgStaple: stapleState(dmgPath),
  submission: 'not-submitted-by-package-script',
}
let provenance
if (verifyOnly) {
  provenance = readJson(provenancePath, 'PACKAGE_PROVENANCE_MISSING')
  verifyPackageProvenance(provenance, {
    embedded: packagedProvenance,
    appTree,
    dmgSha256,
    dmgSize,
    signing,
    notarization,
  })
}
else {
  provenance = {
    schemaVersion: 1,
    build: packagedProvenance,
    artifacts: {
      app: {
        relativePath: normalizePath(relative(root, appPath)),
        treeSha256: appTree.sha256,
        fileCount: appTree.fileCount,
        totalBytes: appTree.totalBytes,
      },
      dmg: {
        relativePath: normalizePath(relative(root, dmgPath)),
        sha256: dmgSha256,
        size: dmgSize,
      },
    },
    signing,
    notarization,
    verification: {
      codesign: 'passed',
      diskImage: 'passed',
      verifiedAt: new Date().toISOString(),
    },
  }
  writeFileSync(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`, { mode: 0o644 })
}
const provenanceSha256 = hashFile(provenancePath)

process.stdout.write(`\nmacOS package verified\n`)
process.stdout.write(`App: ${appPath} (${appSize})\n`)
process.stdout.write(`DMG: ${dmgPath} (${dmgSize} bytes)\n`)
process.stdout.write(`SHA-256: ${dmgSha256}\n`)
process.stdout.write(`Signing: ${signing.mode}\n`)
process.stdout.write(`Notarization: ${notarization.submission}; app=${notarization.appStaple}; dmg=${notarization.dmgStaple}\n`)
process.stdout.write(`Source commit: ${source.commit} (${source.tree})\n`)
process.stdout.write(`Dependency input SHA-256: ${dependencyInput.sha256}\n`)
process.stdout.write(`Provenance: ${provenancePath}\n`)
process.stdout.write(`Provenance SHA-256: ${provenanceSha256}\n`)

function assertCleanCommittedSource() {
  const status = gitOutputRaw(['status', '--porcelain=v1', '-z', '--untracked-files=normal'])
  const inspected = inspectPackageSourceStatus(status)
  if (inspected.trackedChanges.length > 0) {
    throw new Error(
      `PACKAGE_TRACKED_WORKTREE_DIRTY: commit tracked changes before packaging:\n${inspected.trackedChanges.join('\n')}`,
    )
  }
  const ignoredRepositoryInputs = inspectPackageSourceStatus(gitOutputRaw([
    'status',
    '--porcelain=v1',
    '-z',
    '--untracked-files=normal',
    '--ignored=matching',
    '--',
    ...packageIgnoredScanPathspecs(),
  ])).unsafeUntracked
  const unsafeUntracked = [...new Set([
    ...inspected.unsafeUntracked,
    ...ignoredRepositoryInputs,
  ])].sort()
  if (unsafeUntracked.length > 0) {
    throw new Error(
      `PACKAGE_UNTRACKED_BUILD_INPUT: commit or remove untracked files outside the explicit artifacts/output allowlist:\n${unsafeUntracked.join('\n')}`,
    )
  }
  const commit = gitOutput(['rev-parse', 'HEAD'])
  const tree = gitOutput(['rev-parse', 'HEAD^{tree}'])
  const branch = gitOutput(['branch', '--show-current']) || null
  return {
    commit,
    tree,
    branch,
    trackedDirty: false,
    untrackedPolicy: 'ignored',
  }
}

function assertSourceUnchanged(expected) {
  const current = assertCleanCommittedSource()
  if (current.commit !== expected.commit || current.tree !== expected.tree) {
    throw new Error(
      `PACKAGE_SOURCE_COMMIT_CHANGED: expected ${expected.commit}/${expected.tree}, got ${current.commit}/${current.tree}`,
    )
  }
}

function createEmbeddedProvenance(source, dependencyTree) {
  const domainVersions = Object.fromEntries(
    domainRegistry.packs
      .map(pack => [pack.systemId, pack.version])
      .sort(([left], [right]) => left.localeCompare(right, 'en')),
  )
  const identity = {
    schemaVersion: 1,
    git: source,
    inputs: {
      nodeModules: dependencyTree,
    },
    versions: {
      product: product.version,
      tauri: tauri.version,
      corePlugin: corePlugin.version,
      kernelApi: sourceConstant(
        join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'systems.rs'),
        /const DOMAIN_KERNEL_VERSION: &str = "([^"]+)";/u,
        'DOMAIN_KERNEL_VERSION',
      ),
      domainDatabaseSchema: Number(sourceConstant(
        join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'store.rs'),
        /const SCHEMA_VERSION: i64 = (\d+);/u,
        'SCHEMA_VERSION',
      )),
      domainPacks: domainVersions,
    },
    build: {
      builtAt: new Date().toISOString(),
      platform: process.platform,
      architecture,
      node: process.version,
    },
  }
  return {
    ...identity,
    buildId: sha256Json(identity),
  }
}

function assertEmbeddedProvenance(expected, actual, source, dependencyTree) {
  if (actual.schemaVersion !== 1 || actual.buildId !== sha256Json(withoutBuildId(actual)))
    throw new Error('PACKAGED_PROVENANCE_INVALID: embedded build identity hash does not match')
  if (actual.git?.commit !== source.commit || actual.git?.tree !== source.tree)
    throw new Error('PACKAGED_PROVENANCE_COMMIT_MISMATCH: bundle does not match the current commit')
  assertDependencyInputTree(actual.inputs?.nodeModules, dependencyTree)
  if (expected.buildId !== actual.buildId)
    throw new Error('PACKAGED_PROVENANCE_BUILD_MISMATCH: embedded bundle identity differs from this build')
}

function verifyPackageProvenance(manifest, current) {
  if (manifest.schemaVersion !== 1 || manifest.build?.buildId !== current.embedded.buildId)
    throw new Error('PACKAGE_PROVENANCE_BUILD_MISMATCH: sidecar does not match embedded provenance')
  if (manifest.artifacts?.app?.treeSha256 !== current.appTree.sha256
    || manifest.artifacts?.app?.fileCount !== current.appTree.fileCount
    || manifest.artifacts?.app?.totalBytes !== current.appTree.totalBytes) {
    throw new Error('PACKAGE_PROVENANCE_APP_MISMATCH: signed app tree differs from its manifest')
  }
  if (manifest.artifacts?.dmg?.sha256 !== current.dmgSha256
    || manifest.artifacts?.dmg?.size !== current.dmgSize) {
    throw new Error('PACKAGE_PROVENANCE_DMG_MISMATCH: disk image differs from its manifest')
  }
  if (manifest.signing?.mode !== current.signing.mode
    || manifest.signing?.identifier !== current.signing.identifier
    || manifest.signing?.teamIdentifier !== current.signing.teamIdentifier) {
    throw new Error('PACKAGE_PROVENANCE_SIGNING_MISMATCH: current signature differs from its manifest')
  }
  if (manifest.notarization?.appStaple !== current.notarization.appStaple
    || manifest.notarization?.dmgStaple !== current.notarization.dmgStaple
    || manifest.notarization?.submission !== current.notarization.submission) {
    throw new Error('PACKAGE_PROVENANCE_NOTARIZATION_MISMATCH: current staple state differs from its manifest')
  }
  if (manifest.verification?.codesign !== 'passed' || manifest.verification?.diskImage !== 'passed')
    throw new Error('PACKAGE_PROVENANCE_VERIFICATION_MISSING: original native verification did not pass')
}

function withoutBuildId(value) {
  const { buildId: _buildId, ...identity } = value
  return identity
}

function signingState(signature) {
  return {
    mode: signature.includes('Signature=adhoc') ? 'ad-hoc' : 'configured-identity',
    identifier: signatureValue(signature, 'Identifier'),
    teamIdentifier: signatureValue(signature, 'TeamIdentifier'),
    authority: signature
      .split('\n')
      .filter(line => line.startsWith('Authority='))
      .map(line => line.slice('Authority='.length)),
  }
}

function signatureValue(signature, key) {
  const prefix = `${key}=`
  return signature
    .split('\n')
    .find(line => line.startsWith(prefix))
    ?.slice(prefix.length) ?? null
}

function stapleState(path) {
  const result = spawnSync('xcrun', ['stapler', 'validate', path], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  if (result.error?.code === 'ENOENT')
    return 'tool-unavailable'
  return result.status === 0 ? 'stapled' : 'not-stapled'
}

function hashTree(directory) {
  const records = []
  collectTreeRecords(directory, directory, records)
  records.sort((left, right) => left.path.localeCompare(right.path, 'en'))
  const hasher = createHash('sha256')
  let fileCount = 0
  let totalBytes = 0
  for (const record of records) {
    hasher.update(`${record.type}\0${record.path}\0${record.mode}\0${record.size}\0${record.sha256}\n`)
    if (record.type === 'file') {
      fileCount += 1
      totalBytes += record.size
    }
  }
  return { sha256: hasher.digest('hex'), fileCount, totalBytes }
}

function collectTreeRecords(rootDirectory, directory, records) {
  const entries = readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name, 'en'))
  for (const entry of entries) {
    const path = join(directory, entry.name)
    const relativePath = normalizePath(relative(rootDirectory, path))
    const stats = lstatSync(path)
    if (stats.isSymbolicLink()) {
      const target = readlinkSync(path)
      records.push({ type: 'symlink', path: relativePath, mode: stats.mode & 0o7777, size: Buffer.byteLength(target), sha256: sha256(target) })
      continue
    }
    if (stats.isDirectory()) {
      records.push({ type: 'directory', path: relativePath, mode: stats.mode & 0o7777, size: 0, sha256: '' })
      collectTreeRecords(rootDirectory, path, records)
      continue
    }
    if (stats.isFile())
      records.push({ type: 'file', path: relativePath, mode: stats.mode & 0o7777, size: stats.size, sha256: hashFile(path) })
  }
}

function hashFile(path) {
  const descriptor = openSync(path, 'r')
  const hasher = createHash('sha256')
  const buffer = Buffer.allocUnsafe(1024 * 1024)
  try {
    while (true) {
      const bytesRead = readSync(descriptor, buffer, 0, buffer.length, null)
      if (bytesRead === 0)
        break
      hasher.update(buffer.subarray(0, bytesRead))
    }
  }
  finally {
    closeSync(descriptor)
  }
  return hasher.digest('hex')
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}

function sha256Json(value) {
  return sha256(JSON.stringify(value))
}

function normalizePath(value) {
  return value.split(sep).join('/')
}

function readJson(path, code) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'))
  }
  catch (error) {
    throw new Error(`${code}: ${path}: ${error}`)
  }
}

function sourceConstant(path, pattern, label) {
  const match = readFileSync(path, 'utf8').match(pattern)
  if (!match)
    throw new Error(`PACKAGE_PROVENANCE_CONSTANT_MISSING: ${label} in ${path}`)
  return match[1]
}

function gitOutput(args) {
  return output('git', args).trim()
}

function gitOutputRaw(args) {
  return output('git', args)
}

function runPnpm(args, environment) {
  const pnpmScript = process.env.npm_execpath
  if (pnpmScript) {
    run(process.execPath, [pnpmScript, ...args], environment)
    return
  }
  run('pnpm', args, environment)
}

function run(command, args, environment = process.env) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: environment,
    stdio: 'inherit',
  })
  if (result.status !== 0)
    throw new Error(`${command} failed with status ${result.status ?? 'unknown'}`)
}

function output(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  })
}

function combinedOutput(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' })
  if (result.status !== 0)
    throw new Error(`${command} failed with status ${result.status ?? 'unknown'}`)
  return `${result.stdout ?? ''}${result.stderr ?? ''}`
}

function hasNotarizationCredentials(environment) {
  const appleId = environment.APPLE_ID && environment.APPLE_PASSWORD && environment.APPLE_TEAM_ID
  const apiKey = environment.APPLE_API_KEY && environment.APPLE_API_ISSUER && environment.APPLE_API_KEY_PATH
  return Boolean(appleId || apiKey)
}

function stopRunningBundleApp(bundleAppPath) {
  const executablePath = join(bundleAppPath, 'Contents', 'MacOS', 'mir3-studio-ai')
  const result = spawnSync('pgrep', ['-f', '-x', executablePath], { encoding: 'utf8' })
  if (result.status === 1)
    return
  if (result.status !== 0)
    throw new Error(`Unable to inspect an existing bundle app process: ${result.stderr ?? ''}`)
  const processIds = result.stdout
    .split(/\s+/)
    .map(value => Number(value))
    .filter(value => Number.isInteger(value) && value > 0)
  for (const processId of processIds)
    process.kill(processId, 'SIGTERM')
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (processIds.every(processId => !isProcessRunning(processId)))
      return
    spawnSync('sleep', ['0.25'])
  }
  throw new Error(`Please close the previous bundle app before packaging: ${executablePath}`)
}

function isProcessRunning(processId) {
  const result = spawnSync('ps', ['-p', String(processId), '-o', 'state='], { encoding: 'utf8' })
  if (result.status !== 0)
    return false
  return !result.stdout.trim().startsWith('Z')
}
