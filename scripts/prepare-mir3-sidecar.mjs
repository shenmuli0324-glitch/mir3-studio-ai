import { spawnSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'

const mode = process.argv[2] === 'release' ? 'release' : 'debug'
const repository = resolve(import.meta.dirname, '..')
const manifest = join(repository, 'src-tauri', 'Cargo.toml')

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repository, encoding: 'utf8', stdio: 'pipe' })
  if (result.status !== 0) {
    process.stderr.write(result.stdout || '')
    process.stderr.write(result.stderr || '')
    process.exit(result.status ?? 1)
  }
  return result.stdout
}

const rustc = run('rustc', ['-vV'])
const hostTarget = rustc.split('\n').find(line => line.startsWith('host:'))?.slice('host:'.length).trim()
const target = process.env.TAURI_ENV_TARGET_TRIPLE || hostTarget
if (!hostTarget || !target) {
  throw new Error('Unable to detect Rust host target')
}

const packages = ['mir3-mcp']
const cargoArgs = ['build', '--manifest-path', manifest]
for (const packageName of packages)
  cargoArgs.push('-p', packageName)
if (target !== hostTarget)
  cargoArgs.push('--target', target)
if (mode === 'release')
  cargoArgs.push('--release')
const cargoCommand = target !== hostTarget && target.endsWith('-pc-windows-msvc')
  ? 'cargo-xwin'
  : (process.env.CARGO || 'cargo')
run(cargoCommand, cargoArgs)

const extension = target.includes('-windows-') ? '.exe' : ''
for (const packageName of packages) {
  const source = target === hostTarget
    ? join(repository, 'src-tauri', 'target', mode, `${packageName}${extension}`)
    : join(repository, 'src-tauri', 'target', target, mode, `${packageName}${extension}`)
  const destination = join(repository, 'src-tauri', 'binaries', `${packageName}-${target}${extension}`)
  mkdirSync(dirname(destination), { recursive: true })
  copyFileSync(source, destination)
  process.stdout.write(`Prepared ${packageName} sidecar: ${destination}\n`)
}
