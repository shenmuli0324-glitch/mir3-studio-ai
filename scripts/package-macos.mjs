import { execFileSync, spawnSync } from 'node:child_process'
import { readFileSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const product = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const tauri = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))

if (process.platform !== 'darwin')
  throw new Error('package:mac can only run on macOS')

const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64'
const bundleRoot = join(root, 'src-tauri', 'target', 'release', 'bundle')
const appPath = join(bundleRoot, 'macos', `${tauri.productName}.app`)
const dmgPath = join(bundleRoot, 'dmg', `${tauri.productName}_${product.version}_${architecture}.dmg`)
const environment = {
  ...process.env,
  APPLE_SIGNING_IDENTITY: process.env.APPLE_SIGNING_IDENTITY || '-',
}

if (!process.argv.includes('--verify-only'))
  runPnpm(['tauri', 'build', '--bundles', 'app,dmg'], environment)
run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath])
run('hdiutil', ['verify', dmgPath])

const signature = combinedOutput('codesign', ['-dv', '--verbose=4', appPath])
const sha256 = output('shasum', ['-a', '256', dmgPath]).split(/\s+/)[0]
const appSize = output('du', ['-sh', appPath]).split(/\s+/)[0]
const dmgSize = statSync(dmgPath).size
const notarizationConfigured = hasNotarizationCredentials(environment)

process.stdout.write(`\nmacOS package verified\n`)
process.stdout.write(`App: ${appPath} (${appSize})\n`)
process.stdout.write(`DMG: ${dmgPath} (${dmgSize} bytes)\n`)
process.stdout.write(`SHA-256: ${sha256}\n`)
process.stdout.write(`Signing: ${signature.includes('Signature=adhoc') ? 'ad-hoc' : 'configured identity'}\n`)
process.stdout.write(`Notarization credentials: ${notarizationConfigured ? 'configured' : 'not configured'}\n`)

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
