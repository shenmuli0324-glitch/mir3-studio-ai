import { readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const packagePath = join(root, 'package.json')
const current = JSON.parse(readFileSync(packagePath, 'utf8')).version
const requested = process.argv.slice(2).find(value => value !== '--') || 'patch'

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value)
  if (!match)
    throw new Error(`Product version must be stable SemVer (x.y.z), received ${value}`)
  return match.slice(1).map(Number)
}

function nextVersion(value, bump) {
  if (/^\d+\.\d+\.\d+$/.test(bump))
    return bump
  const [major, minor, patch] = parseVersion(value)
  if (bump === 'major')
    return `${major + 1}.0.0`
  if (bump === 'minor')
    return `${major}.${minor + 1}.0`
  if (bump === 'patch')
    return `${major}.${minor}.${patch + 1}`
  throw new Error(`Unknown version bump ${bump}; use patch, minor, major, or x.y.z`)
}

function updateJson(path, update) {
  const value = JSON.parse(readFileSync(path, 'utf8'))
  update(value)
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)
}

function replaceRequired(path, pattern, replacement, label) {
  const before = readFileSync(path, 'utf8')
  const after = before.replace(pattern, replacement)
  if (after === before)
    throw new Error(`Unable to update ${label} in ${path}`)
  writeFileSync(path, after)
}

const version = nextVersion(current, requested)
if (version === current)
  throw new Error(`Product version is already ${version}`)

updateJson(packagePath, value => value.version = version)
updateJson(join(root, 'src', 'brand.config.json'), value => value.version = version)
updateJson(join(root, 'src-tauri', 'tauri.conf.json'), value => value.version = version)

replaceRequired(
  join(root, 'src-tauri', 'Cargo.toml'),
  /(\[package\][\s\S]*?\nversion = ")[^"]+("\n)/,
  `$1${version}$2`,
  'Cargo package version',
)
replaceRequired(
  join(root, 'src-tauri', 'Cargo.lock'),
  /(\[\[package\]\]\nname = "mir3-studio-ai"\nversion = ")[^"]+("\n)/,
  `$1${version}$2`,
  'Cargo lock version',
)
replaceRequired(join(root, 'README.md'), /当前版本为 `[^`]+`/, `当前版本为 \`${version}\``, 'Chinese README current version')
replaceRequired(join(root, 'README.md'), /\| 版本 \| [^|]+ \|/, `| 版本 | ${version} |`, 'Chinese README version table')
replaceRequired(join(root, 'README.en.md'), /Version `[^`]+`/, `Version \`${version}\``, 'English README current version')
replaceRequired(join(root, 'README.en.md'), /\| Version \| [^|]+ \|/, `| Version | ${version} |`, 'English README version table')

process.stdout.write(`MIR3 Studio AI version: ${current} -> ${version}\n`)
