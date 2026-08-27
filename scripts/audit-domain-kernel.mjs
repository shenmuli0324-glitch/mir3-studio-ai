import { Buffer } from 'node:buffer'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'
import { validateDomainFixtureContract, validateDomainManifestContract } from '../src-tauri/resources/mir3-domain-sdk/contract.mjs'

const root = resolve(import.meta.dirname, '..')
const packRoot = join(root, 'src-tauri', 'resources', 'mir3-domain-packs')
const sdkRoot = join(root, 'src-tauri', 'resources', 'mir3-domain-sdk')
const registryPath = join(packRoot, 'registry.json')
const failures = []
const safePrimitives = new Set(['text', 'xls', 'map', 'graph', 'timeline'])
const supportedRenderers = new Set(['table-v1', 'chart-v1', 'calendar-v1', 'ranking-v1', 'flow-v1', 'graph-v1', 'timeline-v1', 'spatial-flow-v1', 'topology-v1', 'map-canvas-v1'])
const operationFamilies = {
  create: /^(?:add|generate|insert|fill)-/,
  clone: /^clone-/,
  batch: /^(?:batch|scale|tune|interpolate)-/,
  replace: /^(?:replace|bind)-/,
  delete: /^(?:delete|remove)-/,
}
const evidenceRows = []
const en = JSON.parse(readFileSync(join(root, 'src', 'i18n', 'locales', 'en-US.json'), 'utf8'))
const zh = JSON.parse(readFileSync(join(root, 'src', 'i18n', 'locales', 'zh-CN.json'), 'utf8'))

for (const file of [
  'package.json',
  'index.mjs',
  'contract.mjs',
  'README.md',
  'CHANGELOG.md',
  'fixtures/contract-corpus.json',
  'fixtures/example-pack/package.json',
  'fixtures/example-pack/domain.json',
  'fixtures/example-pack/schemas/resource.schema.json',
  'fixtures/example-pack/fixtures/valid.json',
  'fixtures/example-pack/fixtures/invalid.json',
  'fixtures/example-pack/fixtures/expected-diagnostics.json',
]) {
  if (!existsSync(join(sdkRoot, file)))
    failures.push(`Domain Plugin SDK is missing ${file}`)
}
if (existsSync(join(sdkRoot, 'package.json'))) {
  const sdkPackage = JSON.parse(readFileSync(join(sdkRoot, 'package.json'), 'utf8'))
  if (sdkPackage.name !== '@mir3-studio/domain-plugin-sdk' || sdkPackage.version !== '1.3.1')
    failures.push('Domain Plugin SDK package identity or SemVer is invalid')
  if (sdkPackage.exports?.['./contract'] !== './contract.mjs')
    failures.push('Domain Plugin SDK contract export is missing')
}
try {
  const corpus = JSON.parse(readFileSync(join(sdkRoot, 'fixtures/contract-corpus.json'), 'utf8'))
  for (const accepted of corpus.accepted) {
    const packRoot = join(sdkRoot, 'fixtures', accepted.packRoot)
    const manifest = JSON.parse(readFileSync(join(packRoot, 'domain.json'), 'utf8'))
    validateDomainManifestContract(manifest)
    validateDomainFixtureContract({
      valid: JSON.parse(readFileSync(join(packRoot, manifest.fixtures.valid), 'utf8')),
      invalid: JSON.parse(readFileSync(join(packRoot, manifest.fixtures.invalid), 'utf8')),
      expectedDiagnostics: JSON.parse(readFileSync(join(packRoot, manifest.fixtures.expectedDiagnostics), 'utf8')),
    })
    if (manifest.systemId !== accepted.systemId || manifest.version !== accepted.version)
      throw new Error(`DOMAIN_SDK_CORPUS_IDENTITY_INVALID: ${accepted.name}`)
    for (const range of corpus.acceptedEngineRanges) {
      const mutated = structuredClone(manifest)
      mutated.supportedEngineRange = range
      validateDomainManifestContract(mutated)
    }
    for (const range of corpus.rejectedEngineRanges) {
      const mutated = structuredClone(manifest)
      mutated.supportedEngineRange = range
      let rejectedBySdk = false
      try {
        validateDomainManifestContract(mutated)
      }
      catch {
        rejectedBySdk = true
      }
      if (!rejectedBySdk)
        throw new Error(`DOMAIN_SDK_REJECTED_ENGINE_RANGE_ACCEPTED: ${range}`)
    }
    for (const rejected of corpus.rejected) {
      const mutated = structuredClone(manifest)
      setJsonPointer(mutated, rejected.pointer, rejected.value)
      let rejectedBySdk = false
      try {
        validateDomainManifestContract(mutated)
      }
      catch {
        rejectedBySdk = true
      }
      if (!rejectedBySdk)
        throw new Error(`DOMAIN_SDK_REJECTED_CORPUS_ACCEPTED: ${rejected.name}`)
    }
  }
}
catch (error) {
  failures.push(`Domain Plugin SDK example contract failed: ${error.message}`)
}
const generatorSource = readFileSync(join(root, 'scripts', 'generate-domain-packs.mjs'), 'utf8')
if (!generatorSource.includes('defineDomainManifest(createPack(definition))') || !generatorSource.includes('defineDomainFixtures(fixtures('))
  failures.push('Domain pack generator does not consume the public SDK entrypoints')

function setJsonPointer(target, pointer, value) {
  const segments = pointer.split('/').slice(1).map(segment => segment.replaceAll('~1', '/').replaceAll('~0', '~'))
  let parent = target
  for (const segment of segments.slice(0, -1))
    parent = parent[segment]
  parent[segments.at(-1)] = structuredClone(value)
}

if (!existsSync(registryPath)) {
  failures.push('Domain registry is missing')
}

const registry = existsSync(registryPath)
  ? JSON.parse(readFileSync(registryPath, 'utf8'))
  : { packs: [] }
const expectedIds = [
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
const ids = registry.packs.map(pack => pack.systemId)
const idSet = new Set(ids)

if (registry.schemaVersion !== 1)
  failures.push(`Domain registry schema must be 1, got ${registry.schemaVersion}`)
if (ids.length !== 33 || idSet.size !== 33)
  failures.push(`Domain registry must contain 33 unique packs, got ${ids.length}/${idSet.size}`)
if (expectedIds.some(id => !idSet.has(id)) || ids.some(id => !expectedIds.includes(id)))
  failures.push('Domain registry IDs do not match the product 33-system contract')

const capabilityIds = new Set()
const operationSchemaFingerprints = new Set()
const semanticFingerprints = new Set()
const runtimeRules = new Set()
const resourceTypes = new Set()
const usedPrimitives = new Set()
for (const pack of registry.packs) {
  const directory = join(packRoot, pack.systemId)
  const required = [
    'package.json',
    'domain.json',
    'README.md',
    'CHANGELOG.md',
    'schemas/resource.schema.json',
    'fixtures/valid.json',
    'fixtures/invalid.json',
    'fixtures/expected-diagnostics.json',
  ]
  for (const file of required) {
    if (!existsSync(join(directory, file)))
      failures.push(`${pack.systemId}: missing ${file}`)
  }
  if (pack.kind !== 'domain' || !/^\d+\.\d+\.\d+$/.test(pack.version || '') || pack.kernelApiRange !== '^1.0.0')
    failures.push(`${pack.systemId}: invalid kind/version/kernelApiRange`)
  if (pack.supportedEngineRange === '*'
    || pack.engineCompatibility?.strategy !== 'evidence-gated-auto-generalization-v1'
    || JSON.stringify(pack.engineCompatibility?.versionAliases) !== JSON.stringify(['semver', 'v-prefixed-semver', 'major-minor'])
    || JSON.stringify(pack.engineCompatibility?.requiredEvidence) !== JSON.stringify(['project-directory-layout', 'owned-selector-or-content-fingerprint', 'resource-schema-validation'])
    || pack.engineCompatibility?.unknownVersionPolicy !== 'readonly'
    || pack.engineCompatibility?.incompatibleVersionPolicy !== 'readonly') {
    failures.push(`${pack.systemId}: engine compatibility must be evidence-gated and fail read-only`)
  }
  if (!pack.renderer || !Array.isArray(pack.dependencies) || !Array.isArray(pack.capabilities))
    failures.push(`${pack.systemId}: renderer, dependencies, and capabilities are required`)
  if (!supportedRenderers.has(pack.renderer))
    failures.push(`${pack.systemId}: unsupported renderer ${pack.renderer}`)
  if (!pack.fileProjection?.keywords?.length)
    failures.push(`${pack.systemId}: file projection keywords are required`)
  if (!pack.fileProjection?.ownedSelectors?.length)
    failures.push(`${pack.systemId}: ownedSelectors are required for real-file discovery`)
  if (!pack.fileProjection?.contentFingerprints?.length)
    failures.push(`${pack.systemId}: content fingerprints are required`)
  if (!pack.fileProjection?.pathAliases?.some(alias => alias.from === 'client' && alias.to === '客户端')
    || !pack.fileProjection?.pathAliases?.some(alias => alias.from === 'engine' && alias.to === '引擎')) {
    failures.push(`${pack.systemId}: client/engine path aliases are incomplete`)
  }
  for (const role of ['client', 'engine', 'shared', 'generated', 'readonly']) {
    if (!pack.fileProjection?.roles?.includes(role))
      failures.push(`${pack.systemId}: file projection omits ${role} role`)
  }
  const dependencySelectorIds = new Set((pack.fileProjection?.dependencySelectors || []).map(selector => selector.systemId))
  for (const dependency of pack.dependencies || []) {
    if (!dependencySelectorIds.has(dependency))
      failures.push(`${pack.systemId}: dependency selector omits ${dependency}`)
  }
  if (pack.fileProjection?.unknownFormatPolicy !== 'readonly')
    failures.push(`${pack.systemId}: unknown formats must be read-only`)
  if (!safePrimitives.has(pack.presentation?.safePrimitive))
    failures.push(`${pack.systemId}: unsupported safe primitive ${pack.presentation?.safePrimitive}`)
  if (!pack.resources?.schema || !pack.resources?.uniqueKey?.length)
    failures.push(`${pack.systemId}: resource schema and unique key are required`)
  if (!pack.resources?.stableResourceId?.includes('sha256(')
    || !pack.resources?.stableResourceId?.includes(pack.systemId)
    || !pack.resources?.stableResourceId?.includes('normalizedRelativePath')) {
    failures.push(`${pack.systemId}: stable resource ID must bind system identity and normalized real path`)
  }
  if (!pack.resources?.mappings?.includes('file-projection')
    || !pack.resources?.mappings?.includes(`${pack.systemId}.field-mapping-v1`)) {
    failures.push(`${pack.systemId}: file/resource bidirectional mapping contract is incomplete`)
  }
  const resourceSchema = existsSync(join(directory, 'schemas/resource.schema.json'))
    ? JSON.parse(readFileSync(join(directory, 'schemas/resource.schema.json'), 'utf8'))
    : { properties: {} }
  const schemaFields = Object.keys(resourceSchema.properties || {}).sort()
  const fieldMappings = pack.resources?.fieldMappings || []
  const mappedFields = fieldMappings.map(mapping => mapping.field).sort()
  if (JSON.stringify(mappedFields) !== JSON.stringify(schemaFields)
    || new Set(mappedFields).size !== mappedFields.length
    || fieldMappings.some(mapping => !Array.isArray(mapping.aliases)
      || !mapping.aliases.includes(mapping.field)
      || !['string', 'integer', 'number', 'boolean'].includes(mapping.valueType)
      || resourceSchema.properties?.[mapping.field]?.type !== mapping.valueType)) {
    failures.push(`${pack.systemId}: executable field mappings must cover the exact resource schema with matching scalar types`)
  }
  const normalizedAliases = fieldMappings
    .flatMap(mapping => mapping.aliases.map(alias => `${alias.toLowerCase().replaceAll(/[^a-z0-9]/g, '')}:${mapping.field}`))
  const aliasOwners = new Map()
  for (const entry of normalizedAliases) {
    const separator = entry.indexOf(':')
    const alias = entry.slice(0, separator)
    const field = entry.slice(separator + 1)
    const existing = aliasOwners.get(alias)
    if (existing && existing !== field)
      failures.push(`${pack.systemId}: field alias ${alias} is ambiguous between ${existing} and ${field}`)
    aliasOwners.set(alias, field)
  }
  for (const resourceType of pack.resources?.resourceTypes || []) {
    if (resourceTypes.has(resourceType))
      failures.push(`${pack.systemId}: duplicate resource type ${resourceType}`)
    resourceTypes.add(resourceType)
  }
  for (const dependency of pack.dependencies || []) {
    if (!idSet.has(dependency))
      failures.push(`${pack.systemId}: unknown dependency ${dependency}`)
  }
  const operations = new Map((pack.operations || []).map(operation => [operation.id, operation]))
  if (operations.size !== (pack.operations || []).length || operations.size !== pack.capabilities.length)
    failures.push(`${pack.systemId}: operations and capabilities must be a one-to-one set`)
  const readable = pack.capabilities.filter(capability => capability.writeSystems.length === 0)
  const writable = pack.capabilities.filter(capability => capability.writeSystems.length > 0)
  if (!readable.length || !writable.length)
    failures.push(`${pack.systemId}: each pack needs both read/inspect and structured write operations`)
  for (const family of ['create', 'clone', 'batch', 'replace']) {
    if (!pack.capabilities.some(capability => operationFamilies[family].test(capability.id)))
      failures.push(`${pack.systemId}: required ${family} operation family is missing`)
  }
  for (const capability of pack.capabilities || []) {
    if (capabilityIds.has(capability.id))
      failures.push(`${pack.systemId}: duplicate capability ${capability.id}`)
    capabilityIds.add(capability.id)
    if (!capability.previewRequired || !capability.validationRequired || !capability.confirmationRequired)
      failures.push(`${pack.systemId}: capability ${capability.id} bypasses a safety gate`)
    if (capability.parameterSchema?.type !== 'object' || capability.parameterSchema?.additionalProperties !== false)
      failures.push(`${pack.systemId}: capability ${capability.id} has no closed object parameter schema`)
    if (capability.parameterSchema?.properties?.operation?.const !== capability.id)
      failures.push(`${pack.systemId}: capability ${capability.id} does not bind its operation discriminator`)
    if ((capability.parameterSchema?.required || []).length === 0 || Object.keys(capability.parameterSchema?.properties || {}).length < 2)
      failures.push(`${pack.systemId}: capability ${capability.id} has no meaningful parameters`)
    const schemaFingerprint = JSON.stringify(capability.parameterSchema)
    if (operationSchemaFingerprints.has(schemaFingerprint))
      failures.push(`${pack.systemId}: capability ${capability.id} reuses another capability parameter schema`)
    operationSchemaFingerprints.add(schemaFingerprint)
    if (!Array.isArray(capability.steps) || capability.steps.length < 2)
      failures.push(`${pack.systemId}: capability ${capability.id} must declare resolve and execution steps`)
    for (const step of capability.steps || []) {
      if (!safePrimitives.has(step.primitive) || !step.action)
        failures.push(`${pack.systemId}: capability ${capability.id} has an unsafe or incomplete step`)
      else
        usedPrimitives.add(step.primitive)
    }
    const operation = operations.get(capability.id)
    if (!operation || JSON.stringify(operation.parameterSchema) !== JSON.stringify(capability.parameterSchema) || JSON.stringify(operation.steps) !== JSON.stringify(capability.steps))
      failures.push(`${pack.systemId}: capability ${capability.id} is not backed by the declared operation contract`)
    if (operation?.writeSystems?.some(systemId => systemId !== pack.systemId)
      || operation?.readSystems?.some(systemId => systemId !== pack.systemId && !pack.dependencies.includes(systemId))) {
      failures.push(`${pack.systemId}: operation ${capability.id} escapes its declared domain/dependency scope`)
    }
    if (capability.writeSystems.length) {
      if (!capability.parameterSchema?.required?.includes('expectedRevision'))
        failures.push(`${pack.systemId}: write capability ${capability.id} does not require expectedRevision`)
      if (!operation?.preconditions?.some(precondition => precondition.endsWith('.draft-open'))
        || !operation?.preconditions?.some(precondition => precondition.endsWith('.revision-match'))) {
        failures.push(`${pack.systemId}: write capability ${capability.id} lacks Draft/revision preconditions`)
      }
      if (!operation?.previewPolicy?.previewRequired || !operation?.previewPolicy?.validationRequired || !operation?.previewPolicy?.confirmationRequired)
        failures.push(`${pack.systemId}: write operation ${capability.id} bypasses preview/validation/confirmation`)
    }
  }
  if (existsSync(join(directory, 'domain.json'))) {
    const local = JSON.parse(readFileSync(join(directory, 'domain.json'), 'utf8'))
    try {
      validateDomainManifestContract(local)
    }
    catch (error) {
      failures.push(`${pack.systemId}: public SDK contract failed (${error.message})`)
    }
    if (JSON.stringify(local) !== JSON.stringify(pack))
      failures.push(`${pack.systemId}: domain.json differs from registry.json`)
  }
  if (existsSync(join(directory, 'package.json'))) {
    const manifest = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'))
    if (manifest.mir3Domain?.kind !== 'domain' || manifest.mir3Domain?.systemId !== pack.systemId)
      failures.push(`${pack.systemId}: package domain metadata is invalid`)
    if (!/^\d+\.\d+\.\d+$/.test(manifest.version || ''))
      failures.push(`${pack.systemId}: package version is not stable SemVer`)
    if (manifest.version !== pack.version)
      failures.push(`${pack.systemId}: package version differs from registry version`)
    if (!readFileSync(join(directory, 'CHANGELOG.md'), 'utf8').includes(`## ${manifest.version}`))
      failures.push(`${pack.systemId}: CHANGELOG has no ${manifest.version} entry`)
    for (const file of ['domain.json', 'schemas/', 'fixtures/', 'README.md', 'CHANGELOG.md']) {
      if (!manifest.files?.includes(file))
        failures.push(`${pack.systemId}: package files omit ${file}`)
    }
    if (manifest.mir3Domain?.resourceSchema !== 'schemas/resource.schema.json')
      failures.push(`${pack.systemId}: package metadata does not expose the resource schema`)
    if (manifest.mir3Domain?.kernelApiRange !== pack.kernelApiRange
      || manifest.mir3Domain?.supportedEngineRange !== pack.supportedEngineRange
      || JSON.stringify(manifest.mir3Domain?.engineCompatibility) !== JSON.stringify(pack.engineCompatibility)) {
      failures.push(`${pack.systemId}: package compatibility declaration differs from domain.json`)
    }
  }
  auditSemanticContract(pack, directory, failures, semanticFingerprints, runtimeRules)
  for (const suffix of ['title', 'description']) {
    const key = `studio.devtools.tool.${pack.systemId}.${suffix}`
    if (typeof en[key] !== 'string' || !en[key].trim() || typeof zh[key] !== 'string' || !zh[key].trim())
      failures.push(`${pack.systemId}: bilingual flat i18n key ${key} is missing`)
  }
  evidenceRows.push({
    systemId: pack.systemId,
    version: pack.version,
    engine: pack.supportedEngineRange,
    files: pack.fileProjection.ownedSelectors.length,
    fingerprints: pack.fileProjection.contentFingerprints.length,
    resources: pack.resources.resourceTypes.length,
    mappings: pack.resources.mappings.length,
    renderer: pack.renderer,
    readOperations: readable.length,
    writeOperations: writable.length,
    validators: pack.validators.length,
    fixtures: 3,
    i18n: 2,
    operationFamilies: Object.fromEntries(Object.entries(operationFamilies).map(([family, pattern]) => [family, pack.capabilities.some(capability => pattern.test(capability.id))])),
  })
}

if (semanticFingerprints.size !== 33)
  failures.push(`All 33 packs need distinct resource/validator semantics, got ${semanticFingerprints.size}`)
if (runtimeRules.size !== 33)
  failures.push(`All 33 packs need distinct runtime rules, got ${runtimeRules.size}`)
if (resourceTypes.size !== 33)
  failures.push(`All 33 packs need distinct resource types, got ${resourceTypes.size}`)
if (operationSchemaFingerprints.size !== 194)
  failures.push(`All 194 capabilities need explicit parameter schemas, got ${operationSchemaFingerprints.size}`)
if (capabilityIds.size !== 194)
  failures.push(`Domain registry must expose exactly 194 official capabilities, got ${capabilityIds.size}`)
for (const primitive of safePrimitives) {
  if (!usedPrimitives.has(primitive))
    failures.push(`No domain operation exercises the ${primitive} safe primitive`)
}

const directories = existsSync(packRoot)
  ? readdirSync(packRoot, { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => entry.name)
  : []
for (const directory of directories) {
  if (!idSet.has(directory))
    failures.push(`Unregistered domain pack directory: ${directory}`)
}

const devtoolRegistry = readFileSync(join(root, 'src', 'features', 'devtools', 'devtool-registry.ts'), 'utf8')
const frontendIds = [...devtoolRegistry.matchAll(/tool\('([^']+)'/g)].map(match => match[1])
if (frontendIds.length !== 33 || expectedIds.some(id => !frontendIds.includes(id)))
  failures.push('Frontend devtool registry does not expose the same 33 systems')

const mcp = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-mcp', 'src', 'main.rs'), 'utf8')
const packLifecycle = readFileSync(join(root, 'src-tauri', 'src', 'service', 'plugin', 'system.rs'), 'utf8')
const domainSystems = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'systems.rs'), 'utf8')
const domainResources = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'resources.rs'), 'utf8')
const runtimeValidators = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'runtime_validators.rs'), 'utf8')
const domainMcp = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-mcp', 'src', 'main.rs'), 'utf8')
const corpusRunner = readFileSync(join(root, 'scripts', 'run-domain-corpus-acceptance.mjs'), 'utf8')
const domainFixtures = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'fixtures.rs'), 'utf8')
const domainDrafts = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'draft.rs'), 'utf8')
const domainGovernance = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'governance.rs'), 'utf8')
const domainStore = readFileSync(join(root, 'src-tauri', 'crates', 'mir3-domain', 'src', 'store.rs'), 'utf8')
const systemAi = readFileSync(join(root, 'src', 'features', 'system-ai', 'system-ai-panel.tsx'), 'utf8')
const globalTaskHandoff = readFileSync(join(root, 'src', 'features', 'system-ai', 'global-task-handoff.ts'), 'utf8')
const globalTaskRecovery = readFileSync(join(root, 'src', 'features', 'system-ai', 'global-task-recovery.ts'), 'utf8')
const rendererSource = readFileSync(join(root, 'src', 'features', 'devtools', 'domain', 'renderers', 'resource-renderer.tsx'), 'utf8')
const pluginBridge = readFileSync(join(root, 'src-tauri', 'src', 'bridge', 'plugin.rs'), 'utf8')
const iframeShim = readFileSync(join(root, 'src', 'hooks', 'use-iframe-shim.ts'), 'utf8')
const expectedTools = [
  'mir3_system_list',
  'mir3_system_describe',
  'mir3_resource_query',
  'mir3_resource_get',
  'mir3_dependency_resolve',
  'mir3_draft_open',
  'mir3_draft_diff',
  'mir3_domain_operate',
  'mir3_capability_list',
  'mir3_capability_describe',
  'mir3_capability_invoke',
  'mir3_validate',
]
for (const tool of expectedTools) {
  if (!mcp.includes(`"${tool}"`))
    failures.push(`MCP is missing ${tool}`)
}
if (!mcp.includes('MCP_MAX_QUERY_ITEMS') || !mcp.includes('MCP_MAX_RESULT_BYTES') || !mcp.includes('MCP_MAX_SCHEMA_BYTES'))
  failures.push('MCP must expose quantified query, result, and schema context budgets')
if (!mcp.includes('.map(system_list_payload)') || !mcp.includes('MCP_RESULT_BUDGET_EXCEEDED:'))
  failures.push('MCP list output must use summaries and fail closed when the result budget is exceeded')
if (!mcp.includes('every_writable_official_operation_compiles_into_a_scoped_draft')
  || !mcp.includes('.filter(|capability| !capability.write_systems.is_empty())')
  || !mcp.includes('assert_capability_lifecycle_coverage(&coverage)')
  || !mcp.includes('assert_eq!(coverage.len(), 33')
  || !mcp.includes('assert_eq!(compiled, 155')
  || !mcp.includes('systems without a representative Apply/restore lifecycle')) {
  failures.push('MCP tests must compile every writable operation from all 33 packs, not only shaped examples')
}
if (!packLifecycle.includes('all_33_domain_packs_support_disable_upgrade_and_rollback')
  || !packLifecycle.includes('corrupt_or_disabled_pack_is_isolated_and_reported_as_a_missing_dependency')
  || !packLifecycle.includes('for system_id in &system_ids')
  || !packLifecycle.includes('assert_eq!(system_ids.len(), 33)')) {
  failures.push('Domain lifecycle tests must cover all-pack upgrade/rollback and every-pack fault isolation')
}
if (!packLifecycle.includes('execute_domain_pack_fixture_canary')
  || !packLifecycle.includes('semantically_tampered_candidate_fixture_never_changes_current')
  || !domainFixtures.includes('all_bundled_domain_pack_fixtures_execute_with_exact_diagnostics')
  || !domainFixtures.includes('execute_fixture(&manifest, &schema, &valid)')
  || !domainFixtures.includes('invalid_diagnostics != expected_diagnostics')) {
  failures.push('Domain candidates must execute valid/invalid fixtures and operation dry-runs before staging or activation')
}
if (!packLifecycle.includes('activate_domain_pack_with_canary')
  || !packLifecycle.includes('domain_pack_canary_failure_rolls_back_and_success_advances_lkg')
  || !pluginBridge.includes('activate_domain_pack_with_governance_canary')) {
  failures.push('Domain candidate activation must rollback failed canaries and advance LKG only after success')
}
if (!iframeShim.includes('invoke<boolean>(\'rollback_core_update\')')
  || !iframeShim.includes('await invoke(\'launch_harness\')')
  || !iframeShim.includes('store.harness.refreshIframe()')) {
  failures.push('Harness candidate canary failure must restore LKG and relaunch the workbench')
}
if (!domainSystems.includes('unknown_extensions_are_readonly')
  || !domainSystems.includes('external_real_project_corpus_runs_the_full_readonly_domain_matrix')
  || !domainSystems.includes('"every detected domain must be validated"')) {
  failures.push('Domain tests must cover unknown-format readonly behavior and the external corpus matrix')
}
if (!domainSystems.includes('load_runtime_resource_schema')
  || !domainSystems.includes('validate_projected_schema_record')
  || !domainSystems.includes('bundled_runtime_schema_loader_covers_all_domain_packs')
  || !domainSystems.includes('schema_validator_loads_pinned_pack_schema_and_fails_closed')
  || !domainSystems.includes('projected_schema_enforces_required_type_enum_pattern_and_bounds')
  || !domainSystems.includes('DOMAIN_SCHEMA_RECORD_INVALID:')) {
  failures.push('Runtime schema validation must load the pinned pack schema, validate projected records, and fail closed')
}
if (!domainSystems.includes('canonicalize_mapped_record')
  || !domainResources.includes('apply_field_mappings')
  || !domainResources.includes('typed_mapped_value')
  || !domainMcp.includes('manifest_field_aliases')
  || !domainMcp.includes('field_line_replacement_with_aliases')) {
  failures.push('Executable field mappings must canonicalize typed reads, schema validation, and safe text/XLS writes')
}
if (!domainSystems.includes('validate_runtime_rule(&validator.rule, projected_records)')
  || !domainFixtures.includes('validate_runtime_rule(&runtime.rule, &records)')
  || !domainFixtures.includes('DOMAIN_PACK_FIXTURE_RUNTIME_ASSERTION_MISMATCH')
  || !runtimeValidators.includes('DOMAIN_RUNTIME_RULE_UNSUPPORTED:')) {
  failures.push('Runtime diagnostics must execute the shared fail-closed evaluator in fixtures and live project validation')
}
for (const rule of runtimeRules) {
  if (!runtimeValidators.includes(`"${rule}" =>`))
    failures.push(`Runtime evaluator does not implement declared rule: ${rule}`)
}
if (!domainSystems.includes('.filter_map(|record| record.value.get(&reference.field))')
  || !domainSystems.includes('validation_dependency_values')
  || !domainSystems.includes('recommended_monster_id=MISSING')) {
  failures.push('Reference validation must consume canonical field mappings for both source and dependency records')
}
if ((domainSystems.match(/#\[ignore = "requires MIR3_DOMAIN_CORPUS_ROOTS/g) || []).length !== 2
  || !domainSystems.includes('MIR3_DOMAIN_CORPUS_ROOTS must be set by the disposable-corpus acceptance runner')
  || !domainSystems.includes('MIR3_CORPUS_READONLY:')
  || !domainSystems.includes('MIR3_CORPUS_WRITE:')
  || !corpusRunner.includes('DOMAIN_CORPUS_DOMAIN_COVERAGE_INCOMPLETE')
  || !/'--ignored'/.test(corpusRunner)) {
  failures.push('External corpus tests must be ignored by default and explicitly executed by the disposable-corpus runner')
}
if (!systemAi.includes('manifest.capabilities')
  || !systemAi.includes('writeSystems')
  || !systemAi.includes('registerGlobalTask')) {
  failures.push('System AI must consume manifest capabilities and support global multi-system tasks')
}
if (!domainDrafts.includes('recover_composite_apply_journals')
  || !domainGovernance.includes('apply_validated_domain_draft_with_governance')
  || !domainGovernance.includes('recover_composite_capability_journals')
  || !domainGovernance.includes('recover_snapshot_governance_journals')
  || !domainGovernance.includes('SNAPSHOT_GOVERNANCE_EXTERNAL_EDIT_CONFLICT:')
  || !domainStore.includes('recover_composite_apply_journals()')
  || !domainStore.includes('recover_snapshot_governance_journals()')) {
  failures.push('Draft Apply, capability invocation, Receipt/Memory and Snapshot rollback must share durable crash recovery without overwriting external edits')
}
if (!globalTaskHandoff.includes('appendScopedUserRequest')
  || !globalTaskHandoff.includes('buildGlobalTaskHandoff')
  || !globalTaskHandoff.includes('REDACTED_CREDENTIAL')
  || !globalTaskRecovery.includes('recoverAndManageGlobalTaskScope')
  || !globalTaskRecovery.includes('retireSourceTaskScope')
  || !domainGovernance.includes('recover_global_task_scope')) {
  failures.push('System-to-global handoff must persist only redacted semantics and recover a backend-verified scoped lease')
}
for (const marker of ['ChartPreview', 'CalendarPreview', 'RankingPreview', 'RelationshipPreview', 'MapCanvas']) {
  if (!rendererSource.includes(marker))
    failures.push(`Central renderer implementation is missing ${marker}`)
}

const systemSummaryBytes = Buffer.byteLength(JSON.stringify({
  systems: registry.packs.map(pack => ({
    systemId: pack.systemId,
    version: pack.version,
    category: pack.category,
    renderer: pack.renderer,
    dependencies: pack.dependencies,
    capabilityCount: pack.capabilities.length,
  })),
}))
if (systemSummaryBytes >= 32 * 1024)
  failures.push(`MCP 33-system summary exceeds 32 KiB: ${systemSummaryBytes}`)
const cyclicComponents = dependencyCycleComponents(registry.packs)

if (failures.length) {
  process.stderr.write(`${failures.join('\n')}\n`)
  process.exit(1)
}
for (const row of evidenceRows) {
  const families = Object.entries(row.operationFamilies).filter(([, present]) => present).map(([family]) => family).join(',') || 'none'
  process.stdout.write(`${row.systemId}@${row.version} engine=${row.engine} metadata=ok projection=${row.files}/${row.fingerprints} resources=${row.resources}/${row.mappings} renderer=${row.renderer} operations=${row.readOperations}R+${row.writeOperations}W families=${families} validators=${row.validators} fixtures=${row.fixtures} i18n=${row.i18n}\n`)
}
const familyCoverage = Object.keys(operationFamilies).map(family => `${family}=${evidenceRows.filter(row => row.operationFamilies[family]).length}/33`).join(', ')
process.stdout.write(`Domain delivery family coverage (reported, not inferred): ${familyCoverage}\n`)
process.stdout.write(`Domain Kernel audit passed (33 per-pack evidence rows, ${capabilityIds.size} official capabilities, 155 writable compiler cases, 12 MCP tools, ${systemSummaryBytes}B system summary, ${cyclicComponents.length} cyclic dependency components reported)\n`)

function dependencyCycleComponents(packs) {
  const dependencies = new Map(packs.map(pack => [pack.systemId, pack.dependencies || []]))
  const reaches = (start, target) => {
    const pending = [...(dependencies.get(start) || [])]
    const seen = new Set()
    while (pending.length) {
      const current = pending.pop()
      if (current === target)
        return true
      if (seen.has(current))
        continue
      seen.add(current)
      pending.push(...(dependencies.get(current) || []))
    }
    return false
  }
  const remaining = new Set(dependencies.keys())
  const components = []
  while (remaining.size) {
    const [first] = remaining
    const component = [...remaining].filter(candidate => reaches(first, candidate) && reaches(candidate, first))
    for (const systemId of component)
      remaining.delete(systemId)
    if (component.length > 1 || (dependencies.get(first) || []).includes(first))
      components.push(component.sort())
    else
      remaining.delete(first)
  }
  return components
}

function auditSemanticContract(pack, directory, failures, semanticFingerprints, runtimeRules) {
  const schemaPath = join(directory, 'schemas', 'resource.schema.json')
  const validPath = join(directory, 'fixtures', 'valid.json')
  const invalidPath = join(directory, 'fixtures', 'invalid.json')
  const diagnosticsPath = join(directory, 'fixtures', 'expected-diagnostics.json')
  if (![schemaPath, validPath, invalidPath, diagnosticsPath].every(existsSync))
    return

  const schema = JSON.parse(readFileSync(schemaPath, 'utf8'))
  const valid = JSON.parse(readFileSync(validPath, 'utf8'))
  const invalid = JSON.parse(readFileSync(invalidPath, 'utf8'))
  const diagnostics = JSON.parse(readFileSync(diagnosticsPath, 'utf8'))
  const properties = schema.properties || {}
  const required = schema.required || []
  const validators = new Map((pack.validators || []).map(validator => [validator.kind, validator]))
  const requiredValidatorKinds = ['syntax', 'schema', 'uniqueness', 'range', 'reference-integrity', 'client-engine-consistency', 'runtime-diagnostics']

  if (schema.type !== 'object' || schema.additionalProperties !== false || Object.keys(properties).length < 4)
    failures.push(`${pack.systemId}: resource schema must be a closed object with at least four fields`)
  if (required.length !== Object.keys(properties).length || required.some(field => !properties[field]))
    failures.push(`${pack.systemId}: resource schema must explicitly require its fields`)
  if (!schema['x-mir3']?.uniqueKey?.length || !schema['x-mir3']?.runtimeRule)
    failures.push(`${pack.systemId}: resource schema lacks MIR3 unique/runtime semantics`)
  for (const kind of requiredValidatorKinds) {
    if (!validators.has(kind))
      failures.push(`${pack.systemId}: missing ${kind} validator`)
  }
  const unique = validators.get('uniqueness')
  const range = validators.get('range')
  const references = validators.get('reference-integrity')
  const consistency = validators.get('client-engine-consistency')
  const runtime = validators.get('runtime-diagnostics')
  if (!unique?.fields?.length || unique.fields.some(field => !properties[field]))
    failures.push(`${pack.systemId}: uniqueness validator has no schema-backed fields`)
  if (!range?.fields?.length || range.fields.some(rule => !properties[rule.field] || (rule.minimum === undefined && rule.maximum === undefined)))
    failures.push(`${pack.systemId}: range validator has no concrete field limits`)
  if (!references?.references?.length || references.references.some(rule => !properties[rule.field] || !rule.systemId))
    failures.push(`${pack.systemId}: reference validator has no concrete cross-system references`)
  if (!consistency?.matchBy || !consistency?.compareFields?.length || consistency.compareFields.some(field => !properties[field]))
    failures.push(`${pack.systemId}: client/engine consistency validator is incomplete`)
  if (!runtime?.rule || runtime.severity !== 'error')
    failures.push(`${pack.systemId}: runtime diagnostics rule is incomplete`)
  else if (runtimeRules.has(runtime.rule))
    failures.push(`${pack.systemId}: runtime rule ${runtime.rule} is reused`)
  else
    runtimeRules.add(runtime.rule)

  const semanticFingerprint = JSON.stringify({
    properties,
    unique: unique?.fields,
    range: range?.fields,
    references: references?.references,
    consistency: consistency?.compareFields,
    runtime: runtime?.rule,
  })
  semanticFingerprints.add(semanticFingerprint)

  if (valid.systemId !== pack.systemId || invalid.systemId !== pack.systemId)
    failures.push(`${pack.systemId}: fixture systemId does not match the pack`)
  if (!Array.isArray(valid.records) || valid.records.length < 2 || !Array.isArray(invalid.records) || invalid.records.length < 2)
    failures.push(`${pack.systemId}: valid and invalid fixtures need at least two records`)
  for (const [index, record] of (valid.records || []).entries())
    auditValidRecord(pack.systemId, index, record, schema, valid.referenceCatalog || {}, failures)

  const invalidUniqueKeys = new Set((invalid.records || []).map(record => (unique?.fields || []).map(field => JSON.stringify(record[field])).join('|')))
  if (invalidUniqueKeys.size === (invalid.records || []).length)
    failures.push(`${pack.systemId}: invalid fixture does not violate its unique key`)
  const missingReference = (invalid.records || []).some(record => (references?.references || []).some(rule => !(invalid.referenceCatalog?.[rule.systemId] || []).includes(record[rule.field])))
  if (!missingReference)
    failures.push(`${pack.systemId}: invalid fixture does not contain a missing reference`)
  const rangeViolated = (invalid.records || []).some(record => (range?.fields || []).some(rule => (rule.minimum !== undefined && record[rule.field] < rule.minimum) || (rule.maximum !== undefined && record[rule.field] > rule.maximum)))
  if (!rangeViolated)
    failures.push(`${pack.systemId}: invalid fixture does not violate a declared range`)
  if (!invalid.runtimeAssertions?.some(assertion => assertion.rule === runtime?.rule && assertion.expected === false))
    failures.push(`${pack.systemId}: invalid fixture does not declare its runtime-rule failure`)
  if (!Array.isArray(diagnostics) || diagnostics.length < 4)
    failures.push(`${pack.systemId}: expected diagnostics must cover at least four rules`)
  const diagnosticCodes = new Set((diagnostics || []).map(diagnostic => diagnostic.code))
  for (const suffix of ['unique', 'reference', 'runtime']) {
    if (!diagnosticCodes.has(`${pack.systemId}.${suffix}`))
      failures.push(`${pack.systemId}: expected diagnostics omit ${suffix}`)
  }
  if (!diagnosticCodes.has(`${pack.systemId}.range`) && !diagnosticCodes.has(`${pack.systemId}.schema`))
    failures.push(`${pack.systemId}: expected diagnostics omit range/schema failure`)

  const readme = readFileSync(join(directory, 'README.md'), 'utf8')
  if (!readme.includes('schemas/resource.schema.json') || !readme.includes('fixtures/expected-diagnostics.json'))
    failures.push(`${pack.systemId}: README does not document schema and fixture contracts`)
}

function auditValidRecord(systemId, index, record, schema, referenceCatalog, failures) {
  for (const field of schema.required || []) {
    const value = record[field]
    const rule = schema.properties[field]
    if (value === undefined) {
      failures.push(`${systemId}: valid fixture ${index} misses ${field}`)
      continue
    }
    if (!valueMatchesType(value, rule.type))
      failures.push(`${systemId}: valid fixture ${index}.${field} has wrong type`)
    if (rule.minimum !== undefined && value < rule.minimum)
      failures.push(`${systemId}: valid fixture ${index}.${field} is below minimum`)
    if (rule.maximum !== undefined && value > rule.maximum)
      failures.push(`${systemId}: valid fixture ${index}.${field} is above maximum`)
    if (rule.enum && !rule.enum.includes(value))
      failures.push(`${systemId}: valid fixture ${index}.${field} is outside enum`)
    if (rule.pattern && typeof value === 'string' && !new RegExp(rule.pattern).test(value))
      failures.push(`${systemId}: valid fixture ${index}.${field} does not match pattern`)
    const referenceSystem = rule['x-mir3-reference-system']
    if (referenceSystem && !(referenceCatalog[referenceSystem] || []).includes(value))
      failures.push(`${systemId}: valid fixture ${index}.${field} has unresolved reference`)
  }
}

function valueMatchesType(value, type) {
  if (type === 'integer')
    return Number.isInteger(value)
  if (type === 'number')
    return typeof value === 'number' && Number.isFinite(value)
  if (type === 'string')
    return typeof value === 'string'
  if (type === 'boolean')
    return typeof value === 'boolean'
  if (type === 'object')
    return value !== null && typeof value === 'object' && !Array.isArray(value)
  return false
}
