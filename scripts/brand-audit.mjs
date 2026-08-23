import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const excludedDirectories = new Set(['.git', 'dist', 'node_modules', 'target'])
const legalFiles = new Set([
  'LICENSE',
  'LICENSE.details',
  'THIRD_PARTY_NOTICES.md',
  'src-tauri/licenses/INSTALLER-LICENSE.txt',
  'scripts/brand-audit.mjs',
])
const forbidden = [
  /deepseek-harness-desktop/i,
  /DeepSeek Harness Desktop/i,
  /io\.github\.hairyf\.deepseek-harness-desktop/i,
  /deepseek-harness-desktop-language/i,
  /LEGACY_GENERATED_MARKER/,
  /dsh_home_migrated/,
  /service::migrate/,
]

function filesUnder(directory) {
  const result = []
  for (const name of readdirSync(directory)) {
    if (excludedDirectories.has(name))
      continue
    const path = join(directory, name)
    if (statSync(path).isDirectory())
      result.push(...filesUnder(path))
    else
      result.push(path)
  }
  return result
}

const violations = []
for (const path of filesUnder(root)) {
  const name = relative(root, path).replaceAll('\\', '/')
  if (legalFiles.has(name) || /\.(?:png|ico|icns|jpg|jpeg|gif|webp)$/i.test(name))
    continue
  const content = readFileSync(path, 'utf8')
  for (const pattern of forbidden) {
    if (pattern.test(content))
      violations.push(`${name}: ${pattern}`)
  }
}

const brand = JSON.parse(readFileSync(join(root, 'src/brand.config.json'), 'utf8'))
const tauri = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
if (tauri.productName !== brand.productName || tauri.identifier !== brand.identifier)
  violations.push('tauri.conf.json does not match brand.config.json')
if (tauri.version !== brand.version || pkg.version !== brand.version)
  violations.push('release versions do not match brand.config.json')
if (existsSync(join(root, 'public/favicon.svg')))
  violations.push('public/favicon.svg must not exist')
if (existsSync(join(root, 'docs/images')))
  violations.push('docs/images must not retain screenshots from the previous product')

const iconEntries = readdirSync(join(root, 'src-tauri/icons'))
if (iconEntries.length !== 1 || iconEntries[0] !== 'mir3-studio-ai')
  violations.push('src-tauri/icons must contain only mir3-studio-ai/')

if (violations.length) {
  console.error(violations.join('\n'))
  process.exit(1)
}
console.log('MIR3 brand audit passed')
