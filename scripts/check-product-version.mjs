import { execFileSync } from 'node:child_process'
import { resolve } from 'node:path'
import process from 'node:process'
import { readProductVersions } from './lib/product-version.mjs'

const root = resolve(import.meta.dirname, '..')

function readGit(path, revision) {
  return execFileSync('git', ['show', `${revision}:${path}`], { cwd: root, encoding: 'utf8' })
}

function changedFiles(args) {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8' })
    .split(/\r?\n/)
    .map(value => value.trim())
    .filter(Boolean)
}

function isFunctional(path) {
  return [
    'src/',
    'src-tauri/src/',
    'src-tauri/crates/',
    'src-tauri/resources/',
    'package.json',
    'pnpm-lock.yaml',
    'vite.config.ts',
  ].some(prefix => path === prefix || path.startsWith(prefix))
}

const values = readProductVersions(root)
const expected = values.get('package.json')
const mismatches = [...values].filter(([, version]) => version !== expected)
if (mismatches.length) {
  for (const [path, version] of mismatches)
    process.stderr.write(`${path}: expected ${expected}, received ${version || 'missing'}\n`)
  process.exit(1)
}

const base = process.env.MIR3_VERSION_BASE
if (base) {
  const baseVersion = JSON.parse(readGit('package.json', base)).version
  const functional = changedFiles(['diff', '--name-only', `${base}...HEAD`]).filter(isFunctional)
  if (functional.length && baseVersion === expected) {
    process.stderr.write(`Functional changes require a product version bump above ${baseVersion}:\n${functional.join('\n')}\n`)
    process.exit(1)
  }
}
else {
  const status = changedFiles(['status', '--porcelain'])
  const functional = status
    .map(line => line.slice(3))
    .filter(path => !path.includes(' -> '))
    .filter(isFunctional)
  if (functional.length) {
    const headVersion = JSON.parse(readGit('package.json', 'HEAD')).version
    if (headVersion === expected) {
      process.stderr.write(`Uncommitted functional changes require: pnpm version:bump -- patch\n${functional.join('\n')}\n`)
      process.exit(1)
    }
  }
}

process.stdout.write(`MIR3 Studio AI version ${expected} is synchronized\n`)
