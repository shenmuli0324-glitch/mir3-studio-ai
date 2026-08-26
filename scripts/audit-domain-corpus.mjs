import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const registry = JSON.parse(readFileSync(join(root, 'src-tauri', 'resources', 'mir3-domain-packs', 'registry.json'), 'utf8'))
const projectRoots = process.argv.slice(2).map(path => resolve(path))

if (projectRoots.length < 3) {
  process.stderr.write('DOMAIN_CORPUS_INSUFFICIENT: pass at least three disposable real project roots\n')
  process.exit(1)
}

const reports = []
for (const projectRoot of projectRoots) {
  if (!existsSync(join(projectRoot, '客户端')) || !existsSync(join(projectRoot, '引擎'))) {
    process.stderr.write(`DOMAIN_CORPUS_LAYOUT_INVALID: ${projectRoot}\n`)
    process.exit(1)
  }
  const files = []
  walk(projectRoot, projectRoot, files)
  const engineVersionPath = join(projectRoot, '引擎', 'mir_version.txt')
  const engineVersion = existsSync(engineVersionPath)
    ? readFileSync(engineVersionPath, 'utf8').trim()
    : 'unknown'
  const coverage = {}
  for (const pack of registry.packs) {
    coverage[pack.systemId] = files.filter(file => matches(pack, file)).length
  }
  reports.push({
    project: basename(projectRoot),
    root: projectRoot,
    engineVersion,
    files: files.length,
    detectedSystems: Object.values(coverage).filter(count => count > 0).length,
    coverage,
  })
}

const signatures = new Set(reports.map(report => JSON.stringify({ version: report.engineVersion, coverage: report.coverage })))
if (signatures.size < 3) {
  process.stderr.write('DOMAIN_CORPUS_NOT_DIVERSE: the three projects must have different versions or materially different domain-file coverage\n')
  process.stderr.write(`${JSON.stringify(reports, null, 2)}\n`)
  process.exit(1)
}

process.stdout.write(`${JSON.stringify({ schemaVersion: 1, projects: reports }, null, 2)}\n`)

function walk(projectRoot, directory, files) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isSymbolicLink() || ignoredDirectory(entry.name))
      continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      walk(projectRoot, path, files)
      continue
    }
    if (!entry.isFile())
      continue
    try {
      if (statSync(path).size >= 0)
        files.push(path.slice(projectRoot.length + 1).replaceAll('\\', '/').toLowerCase())
    }
    catch {
      // 文件在扫描期间被外部工具替换时跳过，正式 Kernel 扫描会报告相应诊断。
    }
  }
}

function ignoredDirectory(name) {
  return ['.git', 'node_modules', 'cache', 'logs', 'log', 'temp', 'tmp'].includes(name.toLowerCase())
}

function matches(pack, path) {
  const extension = path.includes('.') ? path.slice(path.lastIndexOf('.') + 1) : ''
  return pack.fileProjection.keywords.some((keyword) => {
    const normalized = keyword.toLowerCase()
    if (normalized.startsWith('.'))
      return extension === normalized.slice(1)
    return path.includes(normalized)
  })
}
