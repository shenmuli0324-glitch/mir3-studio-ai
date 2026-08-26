import { Buffer } from 'node:buffer'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { closeSync, existsSync, lstatSync, mkdirSync, openSync, readdirSync, readFileSync, readSync, realpathSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, delimiter, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import process from 'node:process'

const repositoryRoot = resolve(import.meta.dirname, '..')
const ignoredDirectoryNames = ['.git', 'node_modules', 'cache', 'logs', 'log', 'temp', 'tmp']
const arguments_ = parseArguments(process.argv.slice(2))
const projectRoots = arguments_.roots.map(validateProjectCopy)
assertDistinctRoots(projectRoots)

const startedAt = new Date()
const inventories = projectRoots.map(inventoryProjectCopy)
const corpusIdentity = sha256Json(inventories.map(inventory => ({
  fileCount: inventory.fileCount,
  totalBytes: inventory.totalBytes,
  treeSha256: inventory.treeSha256,
})))
const reportPath = resolveReportPath(arguments_.output, corpusIdentity, projectRoots)
const audit = run('node', [join(repositoryRoot, 'scripts', 'audit-domain-corpus.mjs'), ...projectRoots])
let auditReport = null
try {
  auditReport = JSON.parse(audit.stdout)
}
catch (error) {
  writeFailureAndExit('DOMAIN_CORPUS_AUDIT_OUTPUT_INVALID', error, audit)
}
if (audit.status !== 0)
  writeFailureAndExit('DOMAIN_CORPUS_AUDIT_FAILED', null, audit, auditReport)

const cargo = run('cargo', [
  'test',
  '-p',
  'mir3-domain',
  'external_real_project_corpus',
  '--',
  '--nocapture',
  '--test-threads=1',
], {
  cwd: join(repositoryRoot, 'src-tauri'),
  env: {
    ...process.env,
    MIR3_DOMAIN_CORPUS_ROOTS: projectRoots.join(delimiter),
  },
})
process.stdout.write(cargo.stdout)
process.stderr.write(cargo.stderr)
const postInventories = projectRoots.map(inventoryProjectCopy)
const restoration = inventories.map((before, index) => ({
  root: before.root,
  beforeTreeSha256: before.treeSha256,
  afterTreeSha256: postInventories[index].treeSha256,
  restored: before.fileCount === postInventories[index].fileCount
    && before.totalBytes === postInventories[index].totalBytes
    && before.treeSha256 === postInventories[index].treeSha256,
}))
const restored = restoration.every(project => project.restored)

const report = acceptanceReport({
  status: cargo.status === 0 && restored ? 'passed' : 'failed',
  auditReport,
  cargo,
  restoration,
})
writeReport(reportPath, report)
process.stdout.write(`Domain corpus acceptance report: ${reportPath}\n`)
if (cargo.status !== 0)
  throw new Error(`DOMAIN_CORPUS_MATRIX_FAILED: cargo exited with ${cargo.status ?? 'unknown'}`)
if (!restored)
  throw new Error('DOMAIN_CORPUS_RESTORE_MISMATCH: at least one project copy differs after the write/restore matrix')

function parseArguments(values) {
  const roots = []
  let output = null
  let confirmed = false
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index]
    if (value === '--confirm-disposable-copies') {
      confirmed = true
      continue
    }
    if (value === '--output') {
      output = values[index + 1]
      index += 1
      if (!output)
        throw new Error('DOMAIN_CORPUS_OUTPUT_REQUIRED: --output requires a path')
      continue
    }
    if (value.startsWith('--'))
      throw new Error(`DOMAIN_CORPUS_ARGUMENT_UNKNOWN: ${value}`)
    roots.push(value)
  }
  if (!confirmed) {
    throw new Error(
      'DOMAIN_CORPUS_DISPOSABLE_CONFIRMATION_REQUIRED: pass --confirm-disposable-copies; the write matrix applies and restores Drafts inside these copies',
    )
  }
  if (roots.length !== 3)
    throw new Error(`DOMAIN_CORPUS_EXACTLY_THREE_REQUIRED: received ${roots.length}`)
  return { roots, output }
}

function validateProjectCopy(value) {
  const candidate = resolve(value)
  if (!existsSync(candidate) || !statSync(candidate).isDirectory())
    throw new Error(`DOMAIN_CORPUS_ROOT_INVALID: ${candidate}`)
  const root = realpathSync(candidate)
  for (const directory of ['客户端', '引擎']) {
    const required = join(root, directory)
    if (!existsSync(required) || !statSync(required).isDirectory())
      throw new Error(`DOMAIN_CORPUS_LAYOUT_INVALID: ${required}`)
  }
  return root
}

function assertDistinctRoots(roots) {
  if (new Set(roots).size !== roots.length)
    throw new Error('DOMAIN_CORPUS_ROOTS_DUPLICATE: each project copy must have a distinct canonical root')
  for (const root of roots) {
    for (const other of roots) {
      if (root !== other && isWithin(root, other))
        throw new Error(`DOMAIN_CORPUS_ROOTS_NESTED: ${other} is inside ${root}`)
    }
  }
}

function inventoryProjectCopy(root) {
  const files = []
  walk(root, root, files)
  files.sort((left, right) => left.path.localeCompare(right.path, 'en'))
  const treeHasher = createHash('sha256')
  let totalBytes = 0
  for (const file of files) {
    treeHasher.update(`${file.path}\0${file.size}\0${file.sha256}\n`)
    totalBytes += file.size
  }
  const engineMarkers = engineMarkerPaths(root)
    .filter(path => existsSync(path) && lstatSync(path).isFile())
    .map((path) => {
      const content = readFileSync(path)
      return {
        path: normalizePath(relative(root, path)),
        sha256: sha256(content),
        size: content.byteLength,
        detectedValue: detectedEngineValue(path, content),
      }
    })
  return {
    name: basename(root),
    root,
    fileCount: files.length,
    totalBytes,
    treeSha256: treeHasher.digest('hex'),
    engineMarkers,
    files,
  }
}

function walk(root, directory, files) {
  const entries = readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name, 'en'))
  for (const entry of entries) {
    if (ignoredDirectory(entry.name))
      continue
    const path = join(directory, entry.name)
    if (entry.isSymbolicLink())
      continue
    if (entry.isDirectory()) {
      walk(root, path, files)
      continue
    }
    if (!entry.isFile())
      continue
    const stats = statSync(path)
    files.push({
      path: normalizePath(relative(root, path)),
      size: stats.size,
      sha256: hashFile(path),
    })
  }
}

function ignoredDirectory(name) {
  return ignoredDirectoryNames.includes(name.toLowerCase())
}

function engineMarkerPaths(root) {
  return [
    '引擎/mir_version.txt',
    '引擎/version.txt',
    '引擎/Config.json',
    '引擎/Config.ini',
    '客户端/version.txt',
    '客户端/Config.json',
  ].map(path => join(root, path))
}

function detectedEngineValue(path, content) {
  if (content.byteLength > 1024 * 1024)
    return null
  const text = content.toString('utf8').trim()
  if (path.toLowerCase().endsWith('.json')) {
    try {
      const parsed = JSON.parse(text)
      return stringValue(parsed.version) ?? stringValue(parsed.engineVersion) ?? stringValue(parsed.engine_version)
    }
    catch {
      return null
    }
  }
  if (path.toLowerCase().endsWith('.ini')) {
    const line = text
      .split(/\r?\n/u)
      .find((candidate) => {
        const key = candidate.slice(0, candidate.indexOf('=')).trim().toLowerCase()
        return ['version', 'engineversion', 'engine_version'].includes(key)
      })
    if (!line)
      return null
    return stringValue(line.slice(line.indexOf('=') + 1))
  }
  return text.slice(0, 256) || null
}

function acceptanceReport({ status, auditReport, cargo, restoration = null }) {
  const finishedAt = new Date()
  return {
    schemaVersion: 1,
    status,
    disposableCopiesConfirmed: true,
    repository: {
      commit: gitOutput(['rev-parse', 'HEAD']),
      tree: gitOutput(['rev-parse', 'HEAD^{tree}']),
      trackedDirty: gitOutput(['status', '--porcelain=v1', '--untracked-files=no']).length > 0,
    },
    runner: {
      path: 'scripts/run-domain-corpus-acceptance.mjs',
      sha256: hashFile(new URL(import.meta.url)),
    },
    inventoryPolicy: {
      algorithm: 'sha256-content-tree-v1',
      ignoredDirectoryNames,
      symlinks: 'not-followed',
    },
    startedAt: startedAt.toISOString(),
    finishedAt: finishedAt.toISOString(),
    durationMillis: finishedAt.getTime() - startedAt.getTime(),
    corpusIdentity,
    projects: inventories,
    domainAudit: auditReport,
    restoration,
    matrix: matrixReport(cargo),
  }
}

function matrixReport(cargo) {
  if (!cargo)
    return { status: 'not-run' }
  return {
    command: 'cargo test -p mir3-domain external_real_project_corpus -- --nocapture --test-threads=1',
    exitCode: cargo.status,
    stdoutSha256: sha256(cargo.stdout),
    stderrSha256: sha256(cargo.stderr),
    stdoutBytes: Buffer.byteLength(cargo.stdout),
    stderrBytes: Buffer.byteLength(cargo.stderr),
  }
}

function writeFailureAndExit(code, error, command, auditReport = null) {
  const report = acceptanceReport({
    status: 'failed',
    auditReport,
    cargo: null,
  })
  report.failure = { code, message: error ? String(error) : null }
  report.auditExecution = {
    exitCode: command.status,
    stdoutSha256: sha256(command.stdout),
    stderrSha256: sha256(command.stderr),
  }
  writeReport(reportPath, report)
  process.stdout.write(command.stdout)
  process.stderr.write(command.stderr)
  process.stderr.write(`Domain corpus acceptance report: ${reportPath}\n`)
  throw new Error(`${code}: ${error ?? `command exited with ${command.status ?? 'unknown'}`}`)
}

function resolveReportPath(value, identity, roots) {
  const fallback = join(tmpdir(), 'mir3-domain-corpus-acceptance', identity, 'report.json')
  if (!value)
    return fallback
  const output = resolve(value)
  for (const root of roots) {
    if (isWithin(root, output))
      throw new Error(`DOMAIN_CORPUS_REPORT_INSIDE_PROJECT: ${output}`)
  }
  if (isWithin(repositoryRoot, output) && !gitIgnored(output)) {
    throw new Error(
      `DOMAIN_CORPUS_REPORT_NOT_IGNORED: repository-local output must be ignored by Git: ${output}`,
    )
  }
  return output
}

function gitIgnored(path) {
  const result = spawnSync('git', ['check-ignore', '--quiet', '--', path], { cwd: repositoryRoot })
  return result.status === 0
}

function writeReport(path, report) {
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`, { flag: 'w', mode: 0o600 })
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repositoryRoot,
    env: options.env ?? process.env,
    encoding: 'utf8',
    maxBuffer: 128 * 1024 * 1024,
  })
  return {
    status: result.status,
    stdout: result.stdout ?? '',
    stderr: result.error ? `${result.stderr ?? ''}${result.error}\n` : result.stderr ?? '',
  }
}

function gitOutput(args) {
  const result = run('git', args)
  if (result.status !== 0)
    throw new Error(`GIT_PROVENANCE_FAILED: git ${args.join(' ')}: ${result.stderr}`)
  return result.stdout.trim()
}

function hashFile(pathOrUrl) {
  const path = pathOrUrl instanceof URL ? pathOrUrl : resolve(pathOrUrl)
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

function isWithin(parent, candidate) {
  const result = relative(parent, candidate)
  return result === '' || (!result.startsWith(`..${sep}`) && result !== '..' && !isAbsolute(result))
}

function stringValue(value) {
  if (typeof value === 'string' && value.trim())
    return value.trim()
  if (typeof value === 'number' && Number.isFinite(value))
    return String(value)
  return null
}
