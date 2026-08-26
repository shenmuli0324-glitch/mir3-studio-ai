import { Buffer } from 'node:buffer'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'

const root = resolve(import.meta.dirname, '..')
const packRoot = join(root, 'src-tauri', 'resources', 'mir3-domain-packs')
const registryPath = join(packRoot, 'registry.json')
const failures = []
const safePrimitives = new Set(['text', 'xls', 'map', 'graph', 'timeline'])

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
  if (!pack.renderer || !Array.isArray(pack.dependencies) || !Array.isArray(pack.capabilities))
    failures.push(`${pack.systemId}: renderer, dependencies, and capabilities are required`)
  if (!pack.fileProjection?.keywords?.length)
    failures.push(`${pack.systemId}: file projection keywords are required`)
  if (pack.fileProjection?.unknownFormatPolicy !== 'readonly')
    failures.push(`${pack.systemId}: unknown formats must be read-only`)
  if (!safePrimitives.has(pack.presentation?.safePrimitive))
    failures.push(`${pack.systemId}: unsupported safe primitive ${pack.presentation?.safePrimitive}`)
  if (!pack.resources?.schema || !pack.resources?.uniqueKey?.length)
    failures.push(`${pack.systemId}: resource schema and unique key are required`)
  for (const resourceType of pack.resources?.resourceTypes || []) {
    if (resourceTypes.has(resourceType))
      failures.push(`${pack.systemId}: duplicate resource type ${resourceType}`)
    resourceTypes.add(resourceType)
  }
  for (const dependency of pack.dependencies || []) {
    if (!idSet.has(dependency))
      failures.push(`${pack.systemId}: unknown dependency ${dependency}`)
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
    const operation = (pack.operations || []).find(entry => entry.id === capability.id)
    if (!operation || JSON.stringify(operation.parameterSchema) !== JSON.stringify(capability.parameterSchema) || JSON.stringify(operation.steps) !== JSON.stringify(capability.steps))
      failures.push(`${pack.systemId}: capability ${capability.id} is not backed by the declared operation contract`)
  }
  if (existsSync(join(directory, 'domain.json'))) {
    const local = JSON.parse(readFileSync(join(directory, 'domain.json'), 'utf8'))
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
  }
  auditSemanticContract(pack, directory, failures, semanticFingerprints, runtimeRules)
}

if (semanticFingerprints.size !== 33)
  failures.push(`All 33 packs need distinct resource/validator semantics, got ${semanticFingerprints.size}`)
if (runtimeRules.size !== 33)
  failures.push(`All 33 packs need distinct runtime rules, got ${runtimeRules.size}`)
if (resourceTypes.size !== 33)
  failures.push(`All 33 packs need distinct resource types, got ${resourceTypes.size}`)
if (operationSchemaFingerprints.size !== 113)
  failures.push(`All 113 capabilities need explicit parameter schemas, got ${operationSchemaFingerprints.size}`)
if (capabilityIds.size !== 113)
  failures.push(`Domain registry must expose exactly 113 official capabilities, got ${capabilityIds.size}`)
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
const expectedTools = [
  'mir3_system_list',
  'mir3_system_describe',
  'mir3_resource_query',
  'mir3_resource_get',
  'mir3_dependency_resolve',
  'mir3_draft_open',
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
process.stdout.write(`Domain Kernel audit passed (33 packs, ${capabilityIds.size} official capabilities, 11 MCP tools, ${systemSummaryBytes}B system summary, ${cyclicComponents.length} cyclic dependency components reported)\n`)

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
