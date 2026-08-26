import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const resources = join(root, 'src-tauri', 'resources')
const pluginRoots = readdirSync(resources)
  .filter(name => name.endsWith('-plugin'))
  .map(name => join(resources, name))
const domainRoot = join(resources, 'mir3-domain-packs')
const expectedDomainIds = [
  'map',
  'npc',
  'monster',
  'equipment',
  'item',
  'level',
  'rebirth',
  'title',
  'buff',
  'skill',
  'enhance',
  'crafting',
  'gem',
  'refine',
  'quest',
  'checkin',
  'online_reward',
  'limited_event',
  'launch_event',
  'first_charge',
  'cumulative_charge',
  'vip',
  'shop',
  'recycle',
  'guild',
  'sabac',
  'ranking',
  'resource_production',
  'manor',
  'hero_soul',
  'talent',
  'season',
  'cross_server',
]
const failures = []

function parseJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'))
  }
  catch (error) {
    failures.push(`${label}: invalid JSON (${error.message})`)
    return null
  }
}

function stableSemVer(value) {
  return /^\d+\.\d+\.\d+$/.test(value || '')
}

function requireFile(rootPath, name, label, declaredFiles) {
  if (!existsSync(join(rootPath, name)))
    failures.push(`${label}: missing local ${name}`)
  if (declaredFiles && !declaredFiles.includes(name))
    failures.push(`${label}: package files must include ${name}`)
}

for (const pluginRoot of pluginRoots) {
  const manifestPath = join(pluginRoot, 'package.json')
  if (!existsSync(manifestPath)) {
    failures.push(`${pluginRoot}: missing package.json`)
    continue
  }
  const manifest = parseJson(manifestPath, pluginRoot)
  if (!manifest)
    continue
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
    if (manifest.name === '@mir3-studio/dsh-mir3-core') {
      const policyPath = join(pluginRoot, 'lib', 'policy.js')
      const serverPolicy = `${server}\n${existsSync(policyPath) ? readFileSync(policyPath, 'utf8') : ''}`
      for (const contract of [
        'inject: [\'sessions\', \'sandboxPolicy\']',
        'isMir3ManagedSession',
        'ctx.on(\'session/created\'',
        'ctx.on(\'fs/write-intent\'',
        'ctx.on(\'fs/edit-intent\'',
        'exec?.agent?.session',
        'MIR3_SYSTEM_SESSION_SCOPE_UNAVAILABLE',
        'MIR3_SYSTEM_SESSION_DRAFT_REQUIRED',
        'isWithin(projectRoot, path)',
      ]) {
        if (!serverPolicy.includes(contract))
          failures.push(`${manifest.name}: scoped system-session policy is missing ${contract}`)
      }
      for (const peer of ['@deepseek-ai/cordis', '@deepseek-ai/dsh-client-runtime', '@deepseek-ai/dsh-sandbox-policy', '@deepseek-ai/dsh-session']) {
        if (manifest.peerDependencies?.[peer] !== '*')
          failures.push(`${manifest.name}: required Harness peer dependency is missing ${peer}`)
      }
    }
  }
  if (clientEntry) {
    const client = readFileSync(join(pluginRoot, clientEntry), 'utf8')
    for (const contract of ['window.__ModuleLoader__.load', 'module.exports', 'return module.exports']) {
      if (!client.includes(contract))
        failures.push(`${manifest.name}: client entry is missing ${contract}`)
    }
    if (manifest.name === '@mir3-studio/dsh-mir3-core') {
      if (manifest.version !== '1.0.3')
        failures.push(`${manifest.name}: compatibility adapter must be version 1.0.3`)
      for (const contract of [
        'const PROTOCOL_VERSION = 2',
        'const SYSTEM_SESSION_PREFIX = \'mir3-system-\'',
        'const GLOBAL_SESSION_PREFIX = \'global-\'',
        'event.source !== window.parent',
        'event.origin !== parentOrigin',
        'ctx.sessions.create',
        'ctx.sessions.open',
        'ctx.workspaces.archiveSession',
        'GLOBAL_SESSION_PROMPT_FAILED',
        'case \'mir3/systemSession.resume\'',
        'case \'mir3/systemSession.prompt\'',
        'case \'mir3/systemSession.cancel\'',
        'case \'mir3/systemSession.respond\'',
        'case \'mir3/systemSession.snapshot\'',
        'case \'mir3/systemSession.complete\'',
        'case \'mir3/globalSession.prompt\'',
        'case \'mir3/globalSession.cancel\'',
        'case \'mir3/globalSession.complete\'',
        'domainResults: projectDomainResults',
        'returnTo: projectReturnTarget',
        'SYSTEM_SESSION_SCOPE_UNVERIFIED',
        'GLOBAL_SESSION_SCOPE_UNVERIFIED',
        'typeof message.sessionId === \'string\'',
        'Object.hasOwn(message, \'payload\')',
        'SESSION_IDENTITY_MISMATCH',
        'sessionOwners',
        'ordinarySessionCanary',
        'harness-canary-',
        'nextOutboundSequence',
        'acceptInboundSequence',
      ]) {
        if (!client.includes(contract))
          failures.push(`${manifest.name}: protocol v2 client is missing ${contract}`)
      }
      if (/postMessage\([^)]*,\s*['"]\*['"]\s*\)/.test(client))
        failures.push(`${manifest.name}: wildcard postMessage origin is forbidden`)
      if (/document\.addEventListener\(\s*['"]click['"]/.test(client))
        failures.push(`${manifest.name}: DOM click interception is forbidden`)
    }
  }
  const changelogPath = join(pluginRoot, 'CHANGELOG.md')
  if (existsSync(changelogPath)) {
    const changelog = readFileSync(changelogPath, 'utf8')
    if (!changelog.includes(`## ${manifest.version}`))
      failures.push(`${manifest.name}: CHANGELOG.md has no entry for ${manifest.version}`)
  }
}

if (pluginRoots.length !== 1 || pluginRoots[0] !== join(resources, 'mir3-core-plugin'))
  failures.push(`Exactly one bundled Harness compatibility adapter is required; found ${pluginRoots.map(path => path.slice(resources.length + 1)).join(', ') || 'none'}`)

if (!existsSync(domainRoot)) {
  failures.push('No bundled MIR3 domain-pack directory found')
}
else {
  const registryPath = join(domainRoot, 'registry.json')
  const registry = existsSync(registryPath) ? parseJson(registryPath, 'domain registry') : null
  if (!existsSync(registryPath))
    failures.push('domain registry: missing registry.json')
  if (registry?.schemaVersion !== 1)
    failures.push('domain registry: schemaVersion must be 1')
  if (!Array.isArray(registry?.packs) || registry.packs.length !== 33)
    failures.push(`domain registry: expected 33 packs, got ${registry?.packs?.length ?? 0}`)

  const directories = readdirSync(domainRoot, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => entry.name)
    .sort()
  const expected = [...expectedDomainIds].sort()
  if (JSON.stringify(directories) !== JSON.stringify(expected))
    failures.push(`domain packs: directory IDs differ from the required 33 systems (${directories.join(', ')})`)

  const registryById = new Map()
  for (const entry of registry?.packs || []) {
    if (typeof entry.systemId !== 'string' || registryById.has(entry.systemId)) {
      failures.push(`domain registry: duplicate or invalid systemId ${entry.systemId}`)
      continue
    }
    registryById.set(entry.systemId, entry)
  }

  const capabilityIds = new Set()
  for (const systemId of directories) {
    const packRoot = join(domainRoot, systemId)
    const packagePath = join(packRoot, 'package.json')
    const domainPath = join(packRoot, 'domain.json')
    const label = `domain ${systemId}`
    if (!existsSync(packagePath) || !existsSync(domainPath)) {
      failures.push(`${label}: package.json and domain.json are required`)
      continue
    }
    const packageManifest = parseJson(packagePath, label)
    const domain = parseJson(domainPath, label)
    if (!packageManifest || !domain)
      continue

    for (const file of ['domain.json', 'README.md', 'CHANGELOG.md'])
      requireFile(packRoot, file, label, packageManifest.files)
    if (packageManifest.kind !== 'domain' || packageManifest.mir3Domain?.kind !== 'domain' || domain.kind !== 'domain')
      failures.push(`${label}: kind must be domain in package and domain manifests`)
    if (packageManifest.name !== `@mir3-studio/domain-${systemId}`)
      failures.push(`${label}: package name does not match systemId`)
    if (packageManifest.private !== true)
      failures.push(`${label}: package must be private and loaded only by MIR3 System Kernel`)
    if (domain.systemId !== systemId || packageManifest.mir3Domain?.systemId !== systemId)
      failures.push(`${label}: systemId must match its directory`)
    if (!stableSemVer(domain.version) || packageManifest.version !== domain.version)
      failures.push(`${label}: package/domain version must be matching stable SemVer`)
    if (domain.kernelApiRange !== '^1.0.0' || packageManifest.mir3Domain?.kernelApiRange !== domain.kernelApiRange)
      failures.push(`${label}: kernelApiRange must be ^1.0.0`)
    if (typeof domain.supportedEngineRange !== 'string' || !domain.supportedEngineRange.trim() || domain.supportedEngineRange === '*')
      failures.push(`${label}: supportedEngineRange must be explicit and non-wildcard`)
    if (domain.engineCompatibility?.strategy !== 'evidence-gated-auto-generalization-v1'
      || !Array.isArray(domain.engineCompatibility?.versionAliases)
      || domain.engineCompatibility.versionAliases.length === 0
      || !Array.isArray(domain.engineCompatibility?.requiredEvidence)
      || domain.engineCompatibility.requiredEvidence.length < 3
      || domain.engineCompatibility?.unknownVersionPolicy !== 'readonly'
      || domain.engineCompatibility?.incompatibleVersionPolicy !== 'readonly') {
      failures.push(`${label}: engine compatibility must declare aliases, evidence, and read-only failure policies`)
    }
    if (JSON.stringify(packageManifest.mir3Domain?.engineCompatibility) !== JSON.stringify(domain.engineCompatibility))
      failures.push(`${label}: package engine compatibility differs from domain manifest`)
    for (const key of ['manifestSchemaVersion', 'resourceSchemaVersion', 'capabilitySchemaVersion', 'memorySchemaVersion']) {
      if (domain[key] !== 1)
        failures.push(`${label}: ${key} must be 1`)
    }
    if (!Number.isInteger(domain.complexity) || domain.complexity < 1 || domain.complexity > 5)
      failures.push(`${label}: complexity must be an integer from 1 through 5`)
    if (domain.documentation?.readme !== 'README.md' || domain.documentation?.changelog !== 'CHANGELOG.md' || packageManifest.mir3Domain?.changelog !== 'CHANGELOG.md')
      failures.push(`${label}: local README and changelog references are required`)
    if (!Array.isArray(domain.requiredKernelPrimitives) || !domain.requiredKernelPrimitives.includes('draft-v1'))
      failures.push(`${label}: requiredKernelPrimitives must include draft-v1`)
    for (const key of ['ownedSelectors', 'dependencySelectors', 'excludes', 'contentFingerprints', 'pathAliases', 'roles']) {
      if (!Array.isArray(domain.fileProjection?.[key]))
        failures.push(`${label}: fileProjection.${key} must be an array`)
    }
    if (!domain.fileProjection?.contentFingerprints?.length || !domain.fileProjection?.pathAliases?.length)
      failures.push(`${label}: file projection fingerprints and path aliases cannot be empty`)
    if (!Array.isArray(domain.resources?.resourceTypes) || !domain.resources.resourceTypes.length)
      failures.push(`${label}: at least one resource type is required`)
    if (!Array.isArray(domain.presentation?.views) || !domain.presentation.views.includes(domain.renderer))
      failures.push(`${label}: presentation views must include the primary renderer`)
    const validatorKinds = new Set((domain.validators || []).map(validator => validator.kind))
    for (const kind of ['syntax', 'schema', 'uniqueness', 'range', 'reference-integrity', 'client-engine-consistency', 'runtime-diagnostics']) {
      if (!validatorKinds.has(kind))
        failures.push(`${label}: validator ${kind} is required`)
    }
    if (!Array.isArray(domain.dependencies))
      failures.push(`${label}: dependencies must be an array`)
    for (const dependency of domain.dependencies || []) {
      if (!expectedDomainIds.includes(dependency) || dependency === systemId)
        failures.push(`${label}: invalid dependency ${dependency}`)
    }
    if (!Array.isArray(domain.capabilities) || !domain.capabilities.length)
      failures.push(`${label}: at least one official capability is required`)
    if (!Array.isArray(domain.operations) || domain.operations.length !== domain.capabilities?.length)
      failures.push(`${label}: every capability must have one declarative operation`)
    const operations = new Map((domain.operations || []).map(operation => [operation.id, operation]))
    for (const capability of domain.capabilities || []) {
      if (!stableSemVer(capability.version) || capabilityIds.has(capability.id))
        failures.push(`${label}: capability ${capability.id} must have a unique ID and stable SemVer`)
      capabilityIds.add(capability.id)
      if (!Array.isArray(capability.writeSystems) || capability.writeSystems.some(writeSystem => writeSystem !== systemId))
        failures.push(`${label}: capability ${capability.id} may write only its own system`)
      if (!capability.reversible || !capability.previewRequired || !capability.validationRequired || (capability.writeSystems.length && !capability.confirmationRequired))
        failures.push(`${label}: capability ${capability.id} must be reversible and writes must be gated by preview, validation, and confirmation`)
      const operation = operations.get(capability.id)
      if (!operation || !operation.parameterSchema || !Array.isArray(operation.readSystems) || !Array.isArray(operation.writeSystems) || !Array.isArray(operation.preconditions) || !operation.preconditions.length || !Array.isArray(operation.steps) || !operation.steps.length || !operation.reversible)
        failures.push(`${label}: operation ${capability.id} is missing schema, scopes, preconditions, reversible steps, or preview policy`)
      if (!Object.keys(operation?.parameterSchema?.properties || {}).length)
        failures.push(`${label}: operation ${capability.id} parameter schema cannot be empty`)
      if (!operation?.previewPolicy?.previewRequired || !operation?.previewPolicy?.validationRequired || (operation?.writeSystems?.length && !operation?.previewPolicy?.confirmationRequired))
        failures.push(`${label}: operation ${capability.id} writes must require preview, validation, and confirmation`)
      if (operation?.writeSystems?.length && !operation?.parameterSchema?.required?.includes('expectedRevision'))
        failures.push(`${label}: mutating operation ${capability.id} must require expectedRevision`)
      const serialized = JSON.stringify(capability.steps || []).toLowerCase()
      if (['shell', 'exec', 'command', 'absolutepath', 'arbitraryscript'].some(token => serialized.includes(token)))
        failures.push(`${label}: capability ${capability.id} contains a forbidden executable step`)
    }

    const registryEntry = registryById.get(systemId)
    if (JSON.stringify(registryEntry) !== JSON.stringify(domain))
      failures.push(`${label}: domain.json must exactly match its registry entry`)
    const changelog = existsSync(join(packRoot, 'CHANGELOG.md')) ? readFileSync(join(packRoot, 'CHANGELOG.md'), 'utf8') : ''
    if (!changelog.includes(`## ${domain.version}`))
      failures.push(`${label}: CHANGELOG.md has no entry for ${domain.version}`)
  }

  for (const systemId of expectedDomainIds) {
    if (!registryById.has(systemId))
      failures.push(`domain registry: missing ${systemId}`)
  }
}

if (failures.length) {
  process.stderr.write(`${failures.join('\n')}\n`)
  process.exit(1)
}
process.stdout.write(`Plugin contract audit passed (${pluginRoots.length} Harness plugin, ${expectedDomainIds.length} domain packs)\n`)
