import { execFileSync, spawn } from 'node:child_process'
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative, resolve, sep } from 'node:path'
import process from 'node:process'
import { assertUnlockedMacosConsoleSession } from './macos-console-session.mjs'
import { REQUIRED_CORE_CHECKS, writeMacosSmokeEvidence } from './macos-smoke-evidence.mjs'

const root = resolve(import.meta.dirname, '..')
const product = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const tauri = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))
const appPath = join(root, 'src-tauri', 'target', 'release', 'bundle', 'macos', `${tauri.productName}.app`)
const executable = join(appPath, 'Contents', 'MacOS', 'mir3-studio-ai')
const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64'
const dmgPath = join(root, 'src-tauri', 'target', 'release', 'bundle', 'dmg', `${tauri.productName}_${product.version}_${architecture}.dmg`)
const provenancePath = `${dmgPath}.provenance.json`
const smokeEvidencePath = `${dmgPath}.smoke.json`

if (process.platform !== 'darwin')
  throw new Error('smoke:mac can only run on macOS')

assertUnlockedMacosConsoleSession()

const packagedVersion = output('plutil', ['-extract', 'CFBundleShortVersionString', 'raw', join(appPath, 'Contents', 'Info.plist')])
if (packagedVersion !== product.version)
  throw new Error(`Packaged version mismatch: expected ${product.version}, got ${packagedVersion}`)

const smokeRoot = mkdtempSync(join(tmpdir(), 'mir3-studio-native-smoke-'))
let outputBuffer = ''
let coreProcessId = null
let corePort = null
let passed = false
const launchedAt = Date.now()
const app = spawn(executable, [], {
  cwd: root,
  env: { ...process.env, MIR3_STUDIO_HOME: smokeRoot },
  stdio: ['ignore', 'pipe', 'pipe'],
})

app.stdout.on('data', chunk => appendOutput(chunk))
app.stderr.on('data', chunk => appendOutput(chunk))

try {
  const pidMarker = join(smokeRoot, '.mir3-core.pid')
  const marker = await waitFor(async () => {
    try {
      const [processId, port] = readFileSync(pidMarker, 'utf8').trim().split(/\s+/).map(Number)
      if (Number.isInteger(processId) && processId > 0 && Number.isInteger(port) && port > 0)
        return { processId, port }
    }
    catch {}
    return null
  }, 45_000, 'MIR3 AI Core PID marker')
  coreProcessId = marker.processId
  corePort = marker.port
  await waitFor(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${corePort}/`, { signal: AbortSignal.timeout(2_000) })
      return response.ok
    }
    catch {
      return false
    }
  }, 45_000, `Harness HTTP service on ${corePort}`)
  assertUnlockedMacosConsoleSession()
  const coreCanary = await waitFor(async () => readCoreCanary(smokeRoot, packagedVersion, launchedAt), 90_000, 'durable Core compatibility canary')
  if (/startup failed|HARNESS_NOT_OWNED|启动超时/u.test(outputBuffer))
    throw new Error(`Native startup reported a failure:\n${outputBuffer}`)
  const packRoot = join(smokeRoot, 'domain-packs')
  const packCount = readdirSync(packRoot, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .filter(entry => hasStateFile(join(packRoot, entry.name, 'state.json')))
    .length
  if (packCount !== 33)
    throw new Error(`Expected 33 initialized domain packs, got ${packCount}`)
  writeMacosSmokeEvidence({
    appVersion: packagedVersion,
    coreCanary,
    dmgPath,
    evidencePath: smokeEvidencePath,
    expectedDmgRelativePath: normalizePath(relative(root, dmgPath)),
    packCount,
    passedAt: Date.now(),
    provenancePath,
  })
  passed = true
  process.stdout.write(`Native macOS smoke passed\n`)
  process.stdout.write(`App: ${appPath}\n`)
  process.stdout.write(`Version: ${packagedVersion}\n`)
  process.stdout.write(`Harness: http://127.0.0.1:${corePort}/\n`)
  process.stdout.write(`Domain packs: ${packCount}\n`)
  process.stdout.write(`Core canary: ${coreCanary.status}; protocol v${coreCanary.protocolVersion}; ${coreCanary.checks.length} public runtime gates\n`)
  process.stdout.write(`Smoke evidence: ${smokeEvidencePath}\n`)
  process.stdout.write(`UI visibility/search acceptance: not asserted by this smoke\n`)
}
finally {
  await stopProcess(app.pid, 'native app')
  if (coreProcessId && processMatchesCore(coreProcessId, corePort))
    await stopProcessGroup(coreProcessId)
  if (passed)
    rmSync(smokeRoot, { recursive: true, force: true })
  else
    process.stderr.write(`Native smoke data retained for diagnosis: ${smokeRoot}\n`)
}

function appendOutput(chunk) {
  outputBuffer = `${outputBuffer}${String(chunk)}`.slice(-64 * 1024)
}

function hasStateFile(path) {
  try {
    const state = JSON.parse(readFileSync(path, 'utf8'))
    return state.current && state.lkg
  }
  catch {
    return false
  }
}

function readCoreCanary(root, expectedVersion, minimumPassedAt) {
  try {
    const state = JSON.parse(readFileSync(join(root, '.mir3-core-canary.json'), 'utf8'))
    if (state.schemaVersion !== 1
      || state.status !== 'passed'
      || state.protocolVersion !== 2
      || state.appVersion !== expectedVersion
      || !Number.isSafeInteger(state.passedAt)
      || state.passedAt < minimumPassedAt - 2_000
      || typeof state.coreTag !== 'string'
      || state.coreTag.length === 0
      || typeof state.coreCommit !== 'string'
      || state.coreCommit.length === 0
      || !Array.isArray(state.checks)
      || REQUIRED_CORE_CHECKS.some(check => !state.checks.includes(check))) {
      return null
    }
    return state
  }
  catch {
    return null
  }
}

async function waitFor(check, timeoutMillis, label) {
  const deadline = Date.now() + timeoutMillis
  while (Date.now() < deadline) {
    const result = await check()
    if (result)
      return result
    if (app.exitCode != null)
      throw new Error(`${label} unavailable because the native app exited with ${app.exitCode}:\n${outputBuffer}`)
    await delay(250)
  }
  throw new Error(`${label} did not become ready:\n${outputBuffer}`)
}

async function stopProcess(processId, label) {
  if (!processId || !isRunning(processId))
    return
  process.kill(processId, 'SIGTERM')
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (!isRunning(processId))
      return
    await delay(100)
  }
  process.kill(processId, 'SIGKILL')
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (!isRunning(processId))
      return
    await delay(100)
  }
  throw new Error(`Unable to stop ${label} process ${processId}`)
}

async function stopProcessGroup(processId) {
  try {
    process.kill(-processId, 'SIGTERM')
  }
  catch {
    process.kill(processId, 'SIGTERM')
  }
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (!isRunning(processId))
      return
    await delay(100)
  }
  if (processMatchesCore(processId, corePort)) {
    try {
      process.kill(-processId, 'SIGKILL')
    }
    catch {
      process.kill(processId, 'SIGKILL')
    }
  }
}

function processMatchesCore(processId, port) {
  if (!isRunning(processId))
    return false
  try {
    const command = output('ps', ['-p', String(processId), '-o', 'command='])
    return command.includes('@deepseek-ai/dsh/lib/bin.js') && command.includes(`--port ${port}`)
  }
  catch {
    return false
  }
}

function isRunning(processId) {
  try {
    process.kill(processId, 0)
    return true
  }
  catch {
    return false
  }
}

function output(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  }).trim()
}

function normalizePath(value) {
  return value.split(sep).join('/')
}

function delay(milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds))
}
