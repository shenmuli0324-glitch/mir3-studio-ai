const stepPrimitives = new Set(['text', 'xls', 'map', 'graph', 'timeline'])
const presentationPrimitives = new Set(['xls', 'map', 'graph', 'timeline'])
const kernelPrimitives = new Set(['resource-index-v1', 'draft-v1', 'diff-v1', 'validation-v1', 'capability-v1', 'map-binary-v1'])
const fileRoles = new Set(['client', 'engine', 'shared', 'generated', 'readonly'])
const validatorKinds = new Set(['syntax', 'schema', 'unique-range', 'uniqueness', 'range', 'reference-integrity', 'client-engine-consistency', 'runtime-diagnostics'])
const supportedRenderers = new Set(['table-v1', 'chart-v1', 'calendar-v1', 'ranking-v1', 'flow-v1', 'graph-v1', 'timeline-v1', 'spatial-flow-v1', 'topology-v1', 'map-canvas-v1'])
const forbiddenKeys = new Set(['shell', 'command', 'executable', 'module', 'component', 'script', 'sourcecode', 'executablecode'])
const forbiddenActionFragments = ['shell', 'exec', 'command', 'script', 'absolute']

export function defineDomainManifest(input) {
  assertSerializableDeclarativeValue(input, 'manifest')
  const manifest = structuredClone(input)
  validateDomainManifestContract(manifest)
  return deepFreeze(manifest)
}

export function defineDomainFixtures(input) {
  assertSerializableDeclarativeValue(input, 'fixtures')
  const fixtures = structuredClone(input)
  validateDomainFixtureContract(fixtures)
  return deepFreeze(fixtures)
}

export function validateDomainManifestContract(manifest) {
  if (manifest?.kind !== 'domain')
    throw new Error('DOMAIN_SDK_KIND_INVALID: kind must be domain')
  if (!/^[a-z][a-z0-9_]*$/.test(manifest.systemId || ''))
    throw new Error('DOMAIN_SDK_SYSTEM_ID_INVALID: systemId must be stable snake_case')
  if (!/^\d+\.\d+\.\d+$/.test(manifest.version || ''))
    throw new Error('DOMAIN_SDK_VERSION_INVALID: version must be stable SemVer')
  if (manifest.kernelApiRange !== '^1.0.0')
    throw new Error('DOMAIN_SDK_KERNEL_API_INVALID: Kernel API v1 is required')
  if (!validSemVerRange(manifest.supportedEngineRange) || /^[ *x]+$/i.test(manifest.supportedEngineRange))
    throw new Error('DOMAIN_SDK_ENGINE_RANGE_INVALID: an explicit engine range is required')
  validateEngineCompatibility(manifest.engineCompatibility)
  validateSchemaVersions(manifest)
  if (typeof manifest.category !== 'string' || manifest.category.length === 0
    || !Number.isInteger(manifest.complexity) || manifest.complexity < 1) {
    throw new Error('DOMAIN_SDK_CLASSIFICATION_INVALID: category and positive integer complexity are required')
  }
  validateKernelPrimitives(manifest.requiredKernelPrimitives)
  validateProjection(manifest.fileProjection, manifest.dependencies)
  validateResources(manifest.resources, manifest.systemId, manifest.dependencies)
  validatePresentation(manifest)
  validateDocumentationAndFixtures(manifest)
  validateValidators(manifest.validators)
  validateOperationsAndCapabilities(manifest)
  return manifest
}

export function validateDomainFixtureContract(fixtures) {
  if (!fixtures?.valid || !fixtures?.invalid || !Array.isArray(fixtures.expectedDiagnostics))
    throw new Error('DOMAIN_SDK_FIXTURE_SET_INVALID: valid, invalid and expectedDiagnostics are required')
  if (fixtures.valid.systemId !== fixtures.invalid.systemId || fixtures.valid.fixture !== 'valid' || fixtures.invalid.fixture !== 'invalid')
    throw new Error('DOMAIN_SDK_FIXTURE_SYSTEM_MISMATCH: fixture identity differs')
  if (!Array.isArray(fixtures.valid.records) || fixtures.valid.records.length < 2)
    throw new Error('DOMAIN_SDK_VALID_FIXTURE_EMPTY: at least two valid records are required')
  if (!Array.isArray(fixtures.invalid.records) || fixtures.invalid.records.length < 2)
    throw new Error('DOMAIN_SDK_INVALID_FIXTURE_EMPTY: at least two invalid records are required')
  if (fixtures.expectedDiagnostics.length === 0
    || fixtures.expectedDiagnostics.some(diagnostic => typeof diagnostic.code !== 'string' || diagnostic.code.length === 0 || diagnostic.severity !== 'error')) {
    throw new Error('DOMAIN_SDK_DIAGNOSTIC_CODE_REQUIRED: every diagnostic needs a code and error severity')
  }
  return fixtures
}

function validateEngineCompatibility(compatibility) {
  if (compatibility?.strategy !== 'evidence-gated-auto-generalization-v1'
    || JSON.stringify(compatibility?.versionAliases) !== JSON.stringify(['semver', 'v-prefixed-semver', 'major-minor'])
    || JSON.stringify(compatibility?.requiredEvidence) !== JSON.stringify(['project-directory-layout', 'owned-selector-or-content-fingerprint', 'resource-schema-validation'])
    || compatibility?.unknownVersionPolicy !== 'readonly'
    || compatibility?.incompatibleVersionPolicy !== 'readonly') {
    throw new Error('DOMAIN_SDK_ENGINE_COMPATIBILITY_INVALID: aliases and evidence must fail read-only')
  }
}

function validateSchemaVersions(manifest) {
  for (const key of ['manifestSchemaVersion', 'resourceSchemaVersion', 'capabilitySchemaVersion', 'memorySchemaVersion']) {
    if (manifest[key] !== 1)
      throw new Error(`DOMAIN_SDK_SCHEMA_VERSION_INVALID: ${key}`)
  }
}

function validateKernelPrimitives(primitives) {
  if (!Array.isArray(primitives) || primitives.length === 0 || new Set(primitives).size !== primitives.length
    || primitives.some(primitive => !kernelPrimitives.has(primitive))) {
    throw new Error('DOMAIN_SDK_KERNEL_PRIMITIVE_INVALID: required primitives must use Kernel API v1')
  }
}

function validateProjection(projection, dependencies) {
  if (!projection || !nonEmptyArray(projection.keywords) || !nonEmptyArray(projection.ownedSelectors) || !nonEmptyArray(projection.contentFingerprints)
    || !nonEmptyArray(projection.pathAliases) || !nonEmptyArray(projection.roles)
    || projection.roles.some(role => !fileRoles.has(role)) || [...fileRoles].some(role => !projection.roles.includes(role))
    || !Array.isArray(projection.excludes) || !Array.isArray(projection.editableExtensions)
    || !Array.isArray(projection.structuredExtensions) || !Array.isArray(projection.readonlyExtensions)
    || projection.contentFingerprints.some(fingerprint => typeof fingerprint?.contains !== 'string' || fingerprint.contains.length === 0 || typeof fingerprint.caseSensitive !== 'boolean')
    || projection.pathAliases.some(alias => typeof alias?.from !== 'string' || alias.from.length === 0 || typeof alias?.to !== 'string' || alias.to.length === 0)
    || projection.unknownFormatPolicy !== 'readonly') {
    throw new Error('DOMAIN_SDK_FILE_PROJECTION_INVALID: selectors, fingerprints, aliases, roles and readonly fallback are required')
  }
  if (!Array.isArray(dependencies) || new Set(dependencies).size !== dependencies.length
    || dependencies.some(systemId => !/^[a-z][a-z0-9_]*$/.test(systemId))) {
    throw new Error('DOMAIN_SDK_DEPENDENCIES_INVALID: dependencies must be an array')
  }
  const selectors = (projection.dependencySelectors || []).map(selector => selector.systemId).sort()
  if (JSON.stringify(selectors) !== JSON.stringify([...dependencies].sort()))
    throw new Error('DOMAIN_SDK_DEPENDENCY_SELECTOR_INVALID: dependency selectors must match dependencies')
}

function validateResources(resources, systemId) {
  if (!resources || !nonEmptyArray(resources.resourceTypes) || typeof resources.schema !== 'string' || resources.schema.length === 0
    || !nonEmptyArray(resources.uniqueKey) || !nonEmptyArray(resources.mappings) || typeof resources.stableResourceId !== 'string'
    || !resources.stableResourceId.startsWith('sha256(') || !resources.stableResourceId.endsWith(':normalizedRelativePath)')) {
    throw new Error('DOMAIN_SDK_RESOURCE_CONTRACT_INVALID: resource identity, schema and unique keys are required')
  }
  if (!resources.stableResourceId.includes(`${systemId}:`))
    throw new Error('DOMAIN_SDK_RESOURCE_ID_SCOPE_INVALID: stable resource ID must include systemId')
  if (!Array.isArray(resources.dependencyEdges)
    || resources.dependencyEdges.some(edge => typeof edge?.field !== 'string' || edge.field.length === 0
      || typeof edge?.systemId !== 'string' || edge.systemId.length === 0 || typeof edge.required !== 'boolean')) {
    throw new Error('DOMAIN_SDK_RESOURCE_DEPENDENCY_INVALID: dependency edges need a systemId')
  }
}

function validatePresentation(manifest) {
  if (!supportedRenderers.has(manifest.renderer) || !manifest.presentation || !nonEmptyArray(manifest.presentation.views)
    || !manifest.presentation.views.includes(manifest.presentation.primaryView)
    || manifest.renderer !== manifest.presentation.primaryView
    || manifest.presentation.views.some(view => typeof view !== 'string' || view.length === 0)
    || !presentationPrimitives.has(manifest.presentation.safePrimitive)) {
    throw new Error('DOMAIN_SDK_PRESENTATION_INVALID: renderer, primary view and safe primitive must match runtime')
  }
}

function validateDocumentationAndFixtures(manifest) {
  if (manifest.documentation?.readme !== 'README.md' || manifest.documentation?.changelog !== 'CHANGELOG.md')
    throw new Error('DOMAIN_SDK_DOCUMENTATION_INVALID: README.md and CHANGELOG.md are required')
  if (!safeRelativePath(manifest.fixtures?.valid) || !safeRelativePath(manifest.fixtures?.invalid)
    || !safeRelativePath(manifest.fixtures?.expectedDiagnostics)) {
    throw new Error('DOMAIN_SDK_FIXTURE_PATH_INVALID: all runtime fixture paths are required')
  }
}

function validateValidators(validators) {
  if (!nonEmptyArray(validators)
    || new Set(validators.map(validator => validator.id)).size !== validators.length
    || validators.some(validator => typeof validator?.id !== 'string' || validator.id.length === 0
      || !validatorKinds.has(validator.kind)
      || (validator.severity !== undefined && typeof validator.severity !== 'string'))) {
    throw new Error('DOMAIN_SDK_VALIDATOR_INVALID: registered runtime validators are required')
  }
}

function validateOperationsAndCapabilities(manifest) {
  if (!nonEmptyArray(manifest.operations) || !nonEmptyArray(manifest.capabilities))
    throw new Error('DOMAIN_SDK_OPERATIONS_REQUIRED: nonempty operations and capabilities are required')
  const operations = new Map(manifest.operations.map(operation => [operation.id, operation]))
  if (operations.size !== manifest.operations.length || operations.size !== manifest.capabilities.length)
    throw new Error('DOMAIN_SDK_OPERATION_SET_INVALID: operations must map one-to-one to capabilities')
  for (const operation of manifest.operations)
    validateOperation(operation, manifest)
  const capabilityIds = new Set()
  for (const capability of manifest.capabilities) {
    if (!capability.id || capabilityIds.has(capability.id))
      throw new Error(`DOMAIN_SDK_CAPABILITY_ID_INVALID: ${capability.id}`)
    capabilityIds.add(capability.id)
    const operation = operations.get(capability.id)
    if (!operation || JSON.stringify(operation.parameterSchema) !== JSON.stringify(capability.parameterSchema)
      || JSON.stringify(operation.steps) !== JSON.stringify(capability.steps)) {
      throw new Error(`DOMAIN_SDK_CAPABILITY_UNBACKED: ${capability.id}`)
    }
    validateClosedSchema(capability.parameterSchema, capability.id)
    validateScopedSystems(capability, manifest, capability.id)
    validateSteps(capability.steps, capability.id)
    if (!/^\d+\.\d+\.\d+$/.test(capability.version || '') || !nonEmptyArray(capability.preconditions)
      || capability.unknownFormatPolicy !== 'readonly' || !capability.reversible
      || !capability.previewRequired || !capability.validationRequired || !capability.confirmationRequired) {
      throw new Error(`DOMAIN_SDK_SAFETY_GATE_REQUIRED: ${capability.id}`)
    }
  }
}

function validateOperation(operation, manifest) {
  if (!operation?.id || !nonEmptyArray(operation.preconditions) || !operation.reversible || operation.unknownFormatPolicy !== 'readonly')
    throw new Error(`DOMAIN_SDK_OPERATION_INVALID: ${operation?.id}`)
  validateClosedSchema(operation.parameterSchema, operation.id)
  validateScopedSystems(operation, manifest, operation.id)
  validateSteps(operation.steps, operation.id)
  if (!operation.steps.some(step => step.operation === operation.id))
    throw new Error(`DOMAIN_SDK_OPERATION_STEP_MISSING: ${operation.id}`)
  if (!operation.previewPolicy?.previewRequired || !operation.previewPolicy?.validationRequired
    || (operation.writeSystems.length > 0 && !operation.previewPolicy?.confirmationRequired)) {
    throw new Error(`DOMAIN_SDK_OPERATION_GATE_REQUIRED: ${operation.id}`)
  }
}

function validateClosedSchema(schema, id) {
  if (schema?.type !== 'object' || schema.additionalProperties !== false || !schema.properties || !Array.isArray(schema.required))
    throw new Error(`DOMAIN_SDK_PARAMETER_SCHEMA_OPEN: ${id}`)
}

function validateScopedSystems(item, manifest, id) {
  if (!Array.isArray(item.readSystems) || !item.readSystems.includes(manifest.systemId) || !Array.isArray(item.writeSystems)
    || item.writeSystems.some(systemId => systemId !== manifest.systemId)
    || item.readSystems.some(systemId => systemId !== manifest.systemId && !manifest.dependencies.includes(systemId))) {
    throw new Error(`DOMAIN_SDK_SYSTEM_SCOPE_INVALID: ${id}`)
  }
}

function validateSteps(steps, id) {
  if (!nonEmptyArray(steps) || !steps.some(step => step.operation === id))
    throw new Error(`DOMAIN_SDK_STEP_REQUIRED: ${id}`)
  for (const step of steps) {
    const action = String(step?.action || '').toLowerCase()
    const operation = String(step?.operation || '').toLowerCase()
    if (!stepPrimitives.has(step?.primitive) || action.length === 0
      || forbiddenActionFragments.some(fragment => action.includes(fragment) || operation.includes(fragment))) {
      throw new Error(`DOMAIN_SDK_STEP_PRIMITIVE_INVALID: ${id}`)
    }
  }
}

function nonEmptyArray(value) {
  return Array.isArray(value) && value.length > 0
}

function safeRelativePath(value) {
  return typeof value === 'string' && value.length > 0 && !value.startsWith('/')
    && !value.startsWith('\\') && !value.split(/[\\/]/).includes('..')
}

function validSemVerRange(value) {
  if (typeof value !== 'string' || value.length === 0 || /[\t\n\r\f\v]/.test(value))
    return false
  const comparators = value.split(',')
  if (comparators.length > 32 || comparators.some(comparator => comparator.replace(/^ +| +$/g, '').length === 0))
    return false
  return comparators.every((comparator) => {
    const normalized = comparator.replace(/^ +| +$/g, '')
    const operatorMatch = normalized.match(/^(<=|>=|[=<>~^])/)
    const operator = operatorMatch?.[0] ?? ''
    const version = normalized.slice(operator.length).replace(/^ +/, '')
    const match = version.match(/^(\d+|[x*])(?:\.(\d+|[x*]))?(?:\.(\d+|[x*]))?(?:-([0-9a-z-]+(?:\.[0-9a-z-]+)*))?(?:\+([0-9a-z-]+(?:\.[0-9a-z-]+)*))?$/i)
    if (!match)
      return false
    const [, major, minor, patch, prerelease, build] = match
    const components = [major, minor, patch]
    if (components.some(component => component !== undefined && /^\d+$/.test(component) && component.length > 1 && component.startsWith('0')))
      return false
    if (components.some(component => component !== undefined && /^\d+$/.test(component) && BigInt(component) > 18_446_744_073_709_551_615n))
      return false
    const wildcardIndex = components.findIndex(component => ['x', 'X', '*'].includes(component))
    if (wildcardIndex >= 0) {
      if ((wildcardIndex === 0 && operator.length > 0) || prerelease)
        return false
      if (components.slice(wildcardIndex + 1).some(component => component !== undefined && !['x', 'X', '*'].includes(component)))
        return false
    }
    if (prerelease) {
      if (minor === undefined || patch === undefined || wildcardIndex >= 0)
        return false
      if (prerelease.split('.').some(identifier => /^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith('0')))
        return false
    }
    if (build && (minor === undefined || patch === undefined || wildcardIndex >= 0))
      return false
    return true
  })
}

function assertSerializableDeclarativeValue(value, path) {
  if (typeof value === 'function' || typeof value === 'symbol' || typeof value === 'bigint')
    throw new Error(`DOMAIN_SDK_DECLARATIVE_ONLY: ${path}`)
  if (!value || typeof value !== 'object')
    return
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertSerializableDeclarativeValue(item, `${path}[${index}]`))
    return
  }
  const prototype = Object.getPrototypeOf(value)
  if (prototype !== Object.prototype && prototype !== null)
    throw new Error(`DOMAIN_SDK_PLAIN_OBJECT_REQUIRED: ${path}`)
  for (const [key, child] of Object.entries(value)) {
    if (forbiddenKeys.has(key.toLowerCase()))
      throw new Error(`DOMAIN_SDK_ARBITRARY_EXECUTION_FORBIDDEN: ${path}.${key}`)
    assertSerializableDeclarativeValue(child, `${path}.${key}`)
  }
}

function deepFreeze(value) {
  if (!value || typeof value !== 'object' || Object.isFrozen(value))
    return value
  Object.freeze(value)
  for (const child of Object.values(value))
    deepFreeze(child)
  return value
}
