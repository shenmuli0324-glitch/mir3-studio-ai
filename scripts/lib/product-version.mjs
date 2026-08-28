import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

export const PRODUCT_VERSION_TARGETS = [
  { path: 'package.json', kind: 'json', property: 'version' },
  { path: 'src/brand.config.json', kind: 'json', property: 'version' },
  { path: 'src-tauri/tauri.conf.json', kind: 'json', property: 'version' },
  {
    path: 'src-tauri/Cargo.toml',
    kind: 'text',
    readPattern: /\[package\][\s\S]*?\r?\nversion = "([^"]+)"/,
    replacePattern: /(\[package\][\s\S]*?\r?\nversion = ")[^"]+("\r?\n)/,
    replacement: '$1{{version}}$2',
  },
  {
    path: 'src-tauri/Cargo.lock',
    kind: 'text',
    readPattern: /\[\[package\]\]\r?\nname = "mir3-studio-ai"\r?\nversion = "([^"]+)"/,
    replacePattern: /(\[\[package\]\]\r?\nname = "mir3-studio-ai"\r?\nversion = ")[^"]+("\r?\n)/,
    replacement: '$1{{version}}$2',
  },
  {
    path: 'README.md',
    kind: 'text',
    readPattern: /当前版本为 `([^`]+)`/,
    replacePattern: /当前版本为 `[^`]+`/,
    replacement: '当前版本为 `{{version}}`',
    additionalReplacements: [
      { pattern: /\| 版本 \| [^|]+ \|/, replacement: '| 版本 | {{version}} |' },
    ],
  },
  {
    path: 'README.en.md',
    kind: 'text',
    readPattern: /Version `([^`]+)`/,
    replacePattern: /Version `[^`]+`/,
    replacement: 'Version `{{version}}`',
    additionalReplacements: [
      { pattern: /\| Version \| [^|]+ \|/, replacement: '| Version | {{version}} |' },
    ],
  },
]

export function parseStableVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value)
  if (!match)
    throw new Error(`Product version must be stable SemVer (x.y.z), received ${value}`)
  return match.slice(1).map(Number)
}

export function nextProductVersion(value, bump) {
  if (/^\d+\.\d+\.\d+$/.test(bump))
    return bump
  const [major, minor, patch] = parseStableVersion(value)
  if (bump === 'major')
    return `${major + 1}.0.0`
  if (bump === 'minor')
    return `${major}.${minor + 1}.0`
  if (bump === 'patch')
    return `${major}.${minor}.${patch + 1}`
  throw new Error(`Unknown version bump ${bump}; use patch, minor, major, or x.y.z`)
}

export function readProductVersions(root) {
  return new Map(PRODUCT_VERSION_TARGETS.map((target) => {
    const content = readFileSync(join(root, target.path), 'utf8')
    return [target.path, readTargetVersion(content, target)]
  }))
}

export function updateProductVersions(root, version) {
  for (const target of PRODUCT_VERSION_TARGETS) {
    const path = join(root, target.path)
    const before = readFileSync(path, 'utf8')
    const after = updateTargetVersion(before, target, version)
    if (after === before)
      throw new Error(`Unable to update product version in ${path}`)
    writeFileSync(path, after)
  }
}

export function readTargetVersion(content, target) {
  if (target.kind === 'json')
    return JSON.parse(content)[target.property]
  return target.readPattern.exec(content)?.[1]
}

export function updateTargetVersion(content, target, version) {
  if (target.kind === 'json') {
    const value = JSON.parse(content)
    value[target.property] = version
    const newline = content.includes('\r\n') ? '\r\n' : '\n'
    return `${JSON.stringify(value, null, 2).replaceAll('\n', newline)}${newline}`
  }
  let updated = replaceRequiredContent(content, target.replacePattern, renderReplacement(target.replacement, version), target.path)
  for (const replacement of target.additionalReplacements ?? [])
    updated = replaceRequiredContent(updated, replacement.pattern, renderReplacement(replacement.replacement, version), target.path)
  return updated
}

export function replaceRequiredContent(content, pattern, replacement, label) {
  const updated = content.replace(pattern, replacement)
  if (updated === content)
    throw new Error(`Unable to update product version in ${label}`)
  return updated
}

function renderReplacement(template, version) {
  return template.replace('{{version}}', version)
}
