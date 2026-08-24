import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const resources = join(root, 'src-tauri', 'resources')
const pluginRoots = readdirSync(resources)
  .filter(name => name.endsWith('-plugin'))
  .map(name => join(resources, name))
const failures = []

for (const pluginRoot of pluginRoots) {
  const manifestPath = join(pluginRoot, 'package.json')
  if (!existsSync(manifestPath)) {
    failures.push(`${pluginRoot}: missing package.json`)
    continue
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  const serverEntry = manifest.exports?.['.']?.default
  const clientEntry = manifest.exports?.['./client']?.default
  const requiredFiles = ['lib', 'README.md', 'CHANGELOG.md']
  if (!/^\d+\.\d+\.\d+$/.test(manifest.version || ''))
    failures.push(`${manifest.name}: version must be stable SemVer`)
  if (!serverEntry || !clientEntry)
    failures.push(`${manifest.name}: Harness server/client exports are required`)
  if (manifest.dsh?.client?.platform !== 'web' || !Array.isArray(manifest.dsh?.client?.inject))
    failures.push(`${manifest.name}: dsh.client web injection metadata is required`)
  for (const file of requiredFiles) {
    if (!existsSync(join(pluginRoot, file)))
      failures.push(`${manifest.name}: missing local ${file}`)
    if (!manifest.files?.includes(file))
      failures.push(`${manifest.name}: package files must include ${file}`)
  }
  if (serverEntry) {
    const server = readFileSync(join(pluginRoot, serverEntry), 'utf8')
    if (!server.includes('export default') || !server.includes('apply'))
      failures.push(`${manifest.name}: server entry must default-export a Harness apply plugin`)
  }
  if (clientEntry) {
    const client = readFileSync(join(pluginRoot, clientEntry), 'utf8')
    for (const contract of ['window.__ModuleLoader__.load', 'module.exports', 'return module.exports']) {
      if (!client.includes(contract))
        failures.push(`${manifest.name}: client entry is missing ${contract}`)
    }
  }
  const changelogPath = join(pluginRoot, 'CHANGELOG.md')
  if (existsSync(changelogPath)) {
    const changelog = readFileSync(changelogPath, 'utf8')
    if (!changelog.includes(`## ${manifest.version}`))
      failures.push(`${manifest.name}: CHANGELOG.md has no entry for ${manifest.version}`)
  }
}

if (!pluginRoots.length)
  failures.push('No bundled Harness plugins found')
if (failures.length) {
  process.stderr.write(`${failures.join('\n')}\n`)
  process.exit(1)
}
process.stdout.write(`Harness plugin contract audit passed (${pluginRoots.length} plugin)\n`)
