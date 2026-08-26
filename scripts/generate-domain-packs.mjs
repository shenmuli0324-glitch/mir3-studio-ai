import { mkdirSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import process from 'node:process'
import { defineDomainFixtures, defineDomainManifest } from '../src-tauri/resources/mir3-domain-sdk/index.mjs'

const root = resolve(import.meta.dirname, '..')
const outputRoot = join(root, 'src-tauri', 'resources', 'mir3-domain-packs')

const packDefinitions = [
  pack('map', 'resources', 5, 'map-canvas-v1', ['mapinfo', '/map/', '.map', '地图'], ['npc', 'monster', 'quest', 'manor', 'sabac'], ['inspect-map', 'clone-map', 'edit-map-config', 'edit-map-region']),
  pack('npc', 'resources', 3, 'flow-v1', ['npc', 'market_def', 'merchant', '商人'], ['map', 'quest', 'shop', 'item'], ['inspect-npc', 'move-npc', 'edit-dialogue', 'replace-npc-reference']),
  pack('monster', 'resources', 2, 'graph-v1', ['monster', 'mongen', 'monitems', '怪物'], ['map', 'item', 'quest'], ['inspect-monster', 'clone-monster', 'tune-monster', 'edit-drop-table']),
  pack('equipment', 'resources', 2, 'table-v1', ['equipment', 'equip', '装备'], ['item', 'enhance', 'gem', 'refine', 'skill', 'buff'], ['inspect-equipment', 'clone-equipment', 'batch-tune-equipment', 'replace-equipment-reference']),
  pack('item', 'resources', 2, 'table-v1', ['item', 'cfg_item', '物品', '道具'], ['equipment', 'buff', 'skill'], ['inspect-item', 'clone-item', 'batch-edit-item', 'replace-item-reference']),
  pack('level', 'growth', 1, 'chart-v1', ['level', 'exp', '等级', '经验'], ['monster', 'quest', 'ranking'], ['inspect-level-curve', 'scale-experience', 'interpolate-levels']),
  pack('rebirth', 'growth', 3, 'graph-v1', ['rebirth', 'reincarnation', '转生'], ['level', 'title', 'skill', 'talent'], ['inspect-rebirth', 'add-rebirth-tier', 'batch-edit-rebirth']),
  pack('title', 'growth', 1, 'table-v1', ['title', '称号'], ['buff', 'level', 'limited_event'], ['inspect-title', 'clone-title', 'batch-edit-title']),
  pack('buff', 'growth', 2, 'timeline-v1', ['buff', 'status_effect', '状态'], ['skill', 'equipment', 'item'], ['inspect-buff', 'clone-buff', 'edit-buff-stacking']),
  pack('skill', 'growth', 4, 'graph-v1', ['skill', 'magic', '技能'], ['buff', 'equipment', 'monster'], ['inspect-skill', 'clone-skill', 'generate-skill-curve', 'bind-skill-effect']),
  pack('enhance', 'equipment', 3, 'chart-v1', ['enhance', 'strengthen', '强化'], ['equipment', 'item', 'buff'], ['inspect-enhancement', 'generate-enhancement-tiers', 'tune-enhancement-probability']),
  pack('crafting', 'equipment', 3, 'graph-v1', ['craft', 'compose', 'recipe', '合成', '配方'], ['item', 'equipment', 'shop'], ['inspect-recipe', 'clone-recipe', 'replace-recipe-material', 'scale-recipe']),
  pack('gem', 'equipment', 3, 'graph-v1', ['gem', 'jewel', '宝石', '镶嵌'], ['item', 'equipment', 'crafting', 'buff'], ['inspect-gem', 'generate-gem-tiers', 'edit-gem-slot']),
  pack('refine', 'equipment', 3, 'chart-v1', ['refine', '洗炼', '精炼'], ['equipment', 'item'], ['inspect-refine-pool', 'edit-refine-weight', 'clone-refine-template']),
  pack('quest', 'activities', 4, 'flow-v1', ['quest', 'questdiary', '任务'], ['npc', 'map', 'monster', 'item', 'level'], ['inspect-quest', 'clone-quest-chain', 'insert-quest-step', 'replace-quest-reward']),
  pack('checkin', 'activities', 1, 'calendar-v1', ['checkin', 'sign', '签到'], ['item', 'vip', 'limited_event'], ['inspect-checkin', 'fill-checkin-rewards', 'clone-checkin-cycle']),
  pack('online_reward', 'activities', 1, 'timeline-v1', ['online_reward', 'online reward', '在线奖励'], ['item', 'vip'], ['inspect-online-reward', 'edit-online-duration', 'replace-online-reward']),
  pack('limited_event', 'activities', 4, 'timeline-v1', ['limited_event', 'limited event', '限时活动'], ['quest', 'shop', 'item', 'npc', 'map'], ['inspect-limited-event', 'clone-limited-event', 'shift-event-window']),
  pack('launch_event', 'activities', 4, 'timeline-v1', ['launch_event', 'open_server', '开服活动'], ['checkin', 'online_reward', 'first_charge', 'cumulative_charge', 'shop', 'quest', 'ranking'], ['inspect-launch-event', 'clone-launch-event', 'shift-launch-schedule']),
  pack('first_charge', 'commercial', 1, 'flow-v1', ['first_charge', 'first charge', '首充'], ['item', 'vip', 'shop', 'limited_event'], ['inspect-first-charge', 'replace-first-charge-reward', 'clone-first-charge-tier']),
  pack('cumulative_charge', 'commercial', 2, 'timeline-v1', ['cumulative_charge', 'recharge_total', '累计充值'], ['item', 'vip', 'limited_event'], ['inspect-cumulative-charge', 'generate-charge-tiers', 'clone-charge-cycle']),
  pack('vip', 'commercial', 2, 'table-v1', ['vip', '贵族'], ['first_charge', 'cumulative_charge', 'shop', 'title', 'level'], ['inspect-vip', 'generate-vip-tiers', 'batch-edit-vip-benefits']),
  pack('shop', 'commercial', 3, 'table-v1', ['shop', 'mall', 'store', '商城'], ['item', 'equipment', 'vip', 'first_charge', 'limited_event'], ['inspect-shop', 'batch-price-shop', 'schedule-shop-item', 'replace-shop-item']),
  pack('recycle', 'commercial', 2, 'table-v1', ['recycle', 'recover', '回收'], ['item', 'equipment', 'shop'], ['inspect-recycle', 'batch-edit-recycle', 'preview-recycle-value']),
  pack('guild', 'social', 4, 'graph-v1', ['guild', 'clan', '行会'], ['npc', 'map', 'item', 'ranking'], ['inspect-guild', 'edit-guild-permission', 'generate-guild-levels']),
  pack('sabac', 'social', 5, 'spatial-flow-v1', ['sabac', '沙巴克', '攻城'], ['map', 'guild', 'npc', 'monster', 'ranking', 'item'], ['inspect-sabac', 'edit-sabac-phase', 'edit-sabac-region', 'validate-sabac-settlement']),
  pack('ranking', 'social', 2, 'ranking-v1', ['ranking', 'rank', '排行榜'], ['level', 'equipment', 'guild', 'season'], ['inspect-ranking', 'clone-ranking', 'edit-ranking-cycle', 'replace-ranking-reward']),
  pack('resource_production', 'featured', 4, 'flow-v1', ['resource_production', 'production', '资源生产'], ['map', 'npc', 'item', 'monster'], ['inspect-production', 'edit-production-rate', 'clone-production-point']),
  pack('manor', 'featured', 4, 'spatial-flow-v1', ['manor', '庄园'], ['map', 'npc', 'monster', 'quest', 'resource_production', 'item'], ['inspect-manor', 'clone-manor', 'edit-manor-entrance', 'validate-manor-loop']),
  pack('hero_soul', 'featured', 4, 'graph-v1', ['hero_soul', 'hero soul', '英雄之魂'], ['item', 'equipment', 'skill', 'buff', 'quest'], ['inspect-hero-soul', 'add-hero-soul-route', 'batch-edit-hero-soul']),
  pack('talent', 'featured', 4, 'graph-v1', ['talent', '天赋'], ['skill', 'buff', 'level', 'rebirth'], ['inspect-talent', 'edit-talent-node', 'edit-talent-edge', 'validate-talent-budget']),
  pack('season', 'featured', 5, 'timeline-v1', ['season', '赛季'], ['ranking', 'quest', 'limited_event', 'shop', 'cross_server'], ['inspect-season', 'clone-season', 'shift-season', 'validate-season-settlement']),
  pack('cross_server', 'extension', 5, 'topology-v1', ['cross_server', 'cross server', '跨服'], ['season', 'ranking', 'guild', 'sabac', 'shop'], ['inspect-cross-server', 'edit-cross-server-route', 'validate-cross-server-compatibility']),
]

const s = (name, options = {}) => ({ name, type: 'string', minLength: 1, ...options })
const i = (name, minimum, maximum, options = {}) => ({ name, type: 'integer', minimum, maximum, ...options })
const n = (name, minimum, maximum, options = {}) => ({ name, type: 'number', minimum, maximum, ...options })
const e = (name, values, options = {}) => ({ name, type: 'string', enum: values, ...options })
const r = (name, systemId, options = {}) => s(name, { referenceSystem: systemId, ...options })

// 这些定义是领域包的可执行语义源，不是 UI 标签。字段、约束、引用和运行规则会进入
// 独立 Schema、Fixture、Validator 与能力参数，生成后由 domain:audit 防止退化为通用模板。
const domainSpecs = {
  map: domain('map-record', [s('mapId', { pattern: '^[A-Za-z0-9_\\-]+$' }), s('displayName'), i('width', 1, 4096), i('height', 1, 4096), e('safeZoneMode', ['none', 'partial', 'full']), r('spawnNpcId', 'npc')], 'map.bounds-contain-spawns', ['width', 'height']),
  npc: domain('npc-record', [s('npcId'), s('scriptPath', { pattern: '\\.(txt|lua)$' }), r('mapId', 'map'), i('coordinateX', 0, 4095), i('coordinateY', 0, 4095), r('shopId', 'shop')], 'npc.script-entry-resolves', ['npcId', 'scriptPath']),
  monster: domain('monster-record', [s('monsterId'), i('combatLevel', 1, 255), i('healthPoints', 1, 2000000000), r('spawnMapId', 'map'), r('primaryDropItemId', 'item')], 'monster.drop-weight-positive', ['monsterId', 'combatLevel']),
  equipment: domain('equipment-record', [s('equipmentId'), e('slot', ['weapon', 'armor', 'helmet', 'ring', 'bracelet', 'necklace', 'boots']), r('baseItemId', 'item'), i('requiredLevel', 0, 255), i('durability', 1, 65535)], 'equipment.slot-matches-item-mode', ['equipmentId', 'slot']),
  item: domain('item-record', [s('itemId'), e('itemType', ['consumable', 'material', 'equipment', 'currency', 'quest']), i('stackLimit', 1, 65535), s('clientIcon'), i('engineStdMode', 0, 255), r('linkedBuffId', 'buff')], 'item.icon-resource-exists', ['itemId', 'engineStdMode']),
  level: domain('level-record', [i('level', 1, 255), i('requiredExperience', 0, 2000000000), i('statPoints', 0, 100000), r('recommendedMonsterId', 'monster')], 'level.experience-monotonic', ['level', 'requiredExperience']),
  rebirth: domain('rebirth-record', [i('rebirthTier', 0, 99), i('minimumLevel', 1, 255), r('costItemId', 'item'), i('costAmount', 0, 1000000000), r('grantedTitleId', 'title')], 'rebirth.minimum-level-reachable', ['rebirthTier', 'minimumLevel']),
  title: domain('title-record', [s('titleId'), s('displayLabel'), r('grantedBuffId', 'buff'), i('durationSeconds', 0, 315360000), i('minimumLevel', 0, 255)], 'title.permanent-duration-zero', ['titleId', 'displayLabel']),
  buff: domain('buff-record', [s('buffId'), e('stackMode', ['replace', 'refresh', 'stack', 'independent']), i('maximumStacks', 1, 999), i('durationMilliseconds', 1, 2147483647), r('effectSkillId', 'skill')], 'buff.stack-mode-capacity-compatible', ['buffId', 'stackMode']),
  skill: domain('skill-record', [s('skillId'), i('skillLevel', 1, 99), i('manaCost', 0, 1000000), i('cooldownMilliseconds', 0, 86400000), r('appliedBuffId', 'buff')], 'skill.level-curve-contiguous', ['skillId', 'skillLevel']),
  enhance: domain('enhance-record', [i('enhanceTier', 1, 99), e('equipmentClass', ['weapon', 'armor', 'accessory']), i('successRateBasisPoints', 0, 10000), r('materialItemId', 'item'), e('failureMode', ['none', 'downgrade', 'break'])], 'enhance.probability-budget-valid', ['enhanceTier', 'equipmentClass']),
  crafting: domain('recipe-record', [s('recipeId'), r('outputItemId', 'item'), i('outputCount', 1, 999999), r('materialItemId', 'item'), i('materialCount', 1, 999999)], 'crafting.no-self-consuming-cycle', ['recipeId', 'outputItemId']),
  gem: domain('gem-record', [s('gemId'), i('gemTier', 1, 99), e('socketType', ['attack', 'defense', 'utility', 'universal']), r('itemId', 'item'), r('grantedBuffId', 'buff')], 'gem.tier-chain-contiguous', ['gemId', 'gemTier']),
  refine: domain('refine-record', [s('poolId'), r('equipmentId', 'equipment'), e('attributeKey', ['attack', 'defense', 'health', 'speed', 'critical']), i('weight', 1, 1000000), n('minimumValue', -1000000, 1000000), n('maximumValue', -1000000, 1000000)], 'refine.minimum-not-greater-than-maximum', ['minimumValue', 'maximumValue']),
  quest: domain('quest-record', [s('questId'), r('startNpcId', 'npc'), r('targetMonsterId', 'monster'), r('rewardItemId', 'item'), i('minimumLevel', 1, 255), s('nextQuestId')], 'quest.chain-acyclic-and-reachable', ['questId', 'nextQuestId']),
  checkin: domain('checkin-record', [s('cycleId'), i('dayIndex', 1, 31), r('rewardItemId', 'item'), i('rewardCount', 1, 999999), n('vipMultiplier', 1, 100)], 'checkin.days-contiguous', ['cycleId', 'dayIndex']),
  online_reward: domain('online-reward-record', [s('rewardId'), i('durationSeconds', 1, 604800), r('rewardItemId', 'item'), i('rewardCount', 1, 999999), i('minimumVipLevel', 0, 99)], 'online-reward.duration-monotonic', ['rewardId', 'durationSeconds']),
  limited_event: domain('limited-event-record', [s('eventId'), i('startEpochSeconds', 0, 4102444800), i('endEpochSeconds', 1, 4102444800), r('eventMapId', 'map'), r('questId', 'quest')], 'limited-event.start-before-end', ['startEpochSeconds', 'endEpochSeconds']),
  launch_event: domain('launch-event-record', [s('scheduleId'), i('openServerDay', 1, 3650), r('eventId', 'limited_event'), r('rewardItemId', 'item'), i('rewardCount', 1, 999999)], 'launch-event.day-windows-nonoverlapping', ['scheduleId', 'openServerDay']),
  first_charge: domain('first-charge-record', [s('tierId'), n('chargeThreshold', 0.01, 100000000), r('rewardItemId', 'item'), i('rewardCount', 1, 999999), i('minimumVipLevel', 0, 99)], 'first-charge.first-tier-is-minimum', ['tierId', 'chargeThreshold']),
  cumulative_charge: domain('cumulative-charge-record', [s('tierId'), s('cycleId'), n('chargeThreshold', 0.01, 100000000), r('rewardItemId', 'item'), i('rewardCount', 1, 999999)], 'cumulative-charge.thresholds-strictly-increase', ['cycleId', 'chargeThreshold']),
  vip: domain('vip-record', [i('vipLevel', 0, 99), i('requiredPoints', 0, 2000000000), i('shopDiscountBasisPoints', 0, 10000), r('grantedTitleId', 'title')], 'vip.points-monotonic', ['vipLevel', 'requiredPoints']),
  shop: domain('shop-offer-record', [s('offerId'), s('shopId'), r('itemId', 'item'), r('currencyItemId', 'item'), n('price', 0, 1000000000), i('startEpochSeconds', 0, 4102444800), i('endEpochSeconds', 1, 4102444800)], 'shop.sale-window-and-price-valid', ['offerId', 'shopId']),
  recycle: domain('recycle-rule-record', [s('ruleId'), e('itemType', ['equipment', 'material', 'consumable']), i('minimumQuality', 0, 20), i('maximumQuality', 0, 20), r('currencyItemId', 'item'), i('returnValue', 0, 2000000000)], 'recycle.quality-range-ordered', ['minimumQuality', 'maximumQuality']),
  guild: domain('guild-level-record', [i('guildLevel', 1, 99), i('requiredContribution', 0, 2000000000), i('maximumMembers', 1, 100000), r('rankingBoardId', 'ranking')], 'guild.members-and-contribution-monotonic', ['guildLevel', 'requiredContribution']),
  sabac: domain('sabac-phase-record', [s('phaseId'), r('battleMapId', 'map'), i('startMinute', 0, 10079), i('endMinute', 1, 10080), r('guildRewardItemId', 'item')], 'sabac.phases-ordered-and-regions-contained', ['phaseId', 'battleMapId']),
  ranking: domain('ranking-board-record', [s('boardId'), e('metric', ['level', 'power', 'guild', 'wealth', 'season-points']), i('cycleSeconds', 60, 31536000), r('rewardItemId', 'item'), r('seasonId', 'season')], 'ranking.settlement-within-cycle', ['boardId', 'metric']),
  resource_production: domain('production-point-record', [s('pointId'), r('mapId', 'map'), r('outputItemId', 'item'), i('intervalSeconds', 1, 31536000), i('yieldCount', 1, 999999), r('guardMonsterId', 'monster')], 'production.point-inside-map-and-rate-positive', ['pointId', 'mapId']),
  manor: domain('manor-record', [s('manorId'), r('mapId', 'map'), r('entryNpcId', 'npc'), i('minimumLevel', 1, 255), r('productionPointId', 'resource_production')], 'manor.entry-and-exit-loop-reachable', ['manorId', 'mapId']),
  hero_soul: domain('hero-soul-route-record', [s('routeId'), s('nodeId'), r('costItemId', 'item'), r('grantedSkillId', 'skill'), i('powerValue', 0, 2000000000)], 'hero-soul.route-acyclic-and-affordable', ['routeId', 'nodeId']),
  talent: domain('talent-node-record', [s('nodeId'), s('treeId'), i('costPoints', 0, 9999), i('requiredLevel', 1, 255), r('grantedSkillId', 'skill'), s('parentNodeId')], 'talent.graph-acyclic-and-budget-valid', ['treeId', 'nodeId']),
  season: domain('season-record', [s('seasonId'), i('startEpochSeconds', 0, 4102444800), i('endEpochSeconds', 1, 4102444800), r('rankingBoardId', 'ranking'), r('seasonShopId', 'shop')], 'season.window-and-settlement-ordered', ['seasonId', 'startEpochSeconds']),
  cross_server: domain('cross-server-route-record', [s('routeId'), s('sourceShard'), s('targetShard'), s('minimumEngineVersion', { pattern: '^\\d+\\.\\d+$' }), s('maximumEngineVersion', { pattern: '^\\d+\\.\\d+$' }), i('maximumConcurrentPlayers', 1, 1000000), r('seasonId', 'season')], 'cross-server.route-and-engine-range-compatible', ['routeId', 'sourceShard', 'targetShard']),
}

function domain(resourceType, fields, runtimeRule, consistencyFields) {
  return { resourceType, fields, runtimeRule, consistencyFields }
}

const compoundUniqueKeys = {
  skill: ['skillId', 'skillLevel'],
  enhance: ['enhanceTier', 'equipmentClass'],
  gem: ['gemId', 'gemTier'],
  refine: ['poolId', 'equipmentId', 'attributeKey'],
  checkin: ['cycleId', 'dayIndex'],
  cumulative_charge: ['cycleId', 'chargeThreshold'],
  hero_soul: ['routeId', 'nodeId'],
  talent: ['treeId', 'nodeId'],
}

function pack(id, category, complexity, renderer, keywords, dependencies, capabilities, version = '1.3.0') {
  return { id, category, complexity, renderer, keywords, dependencies, capabilities, version }
}

function createPack(definition) {
  const { id, category, complexity, renderer, keywords, dependencies, capabilities, version } = definition
  const spec = domainSpecs[id]
  const primitive = primitiveForRenderer(renderer)
  const uniqueKey = compoundUniqueKeys[id] || [spec.fields[0].name]
  const completedCapabilities = completeOperationFamilies(id, capabilities)
  const operations = completedCapabilities.map(capabilityId => operation(id, dependencies, capabilityId, spec, operationPrimitive(capabilityId, primitive)))
  const rangeFields = spec.fields
    .filter(field => field.minimum !== undefined || field.maximum !== undefined)
    .map(field => ({ field: field.name, minimum: field.minimum, maximum: field.maximum }))
  const references = spec.fields
    .filter(field => field.referenceSystem)
    .map(field => ({ field: field.name, systemId: field.referenceSystem, required: true }))
  return {
    kind: 'domain',
    systemId: id,
    version,
    kernelApiRange: '^1.0.0',
    supportedEngineRange: '>=1.0.0',
    engineCompatibility: {
      strategy: 'evidence-gated-auto-generalization-v1',
      versionAliases: ['semver', 'v-prefixed-semver', 'major-minor'],
      requiredEvidence: ['project-directory-layout', 'owned-selector-or-content-fingerprint', 'resource-schema-validation'],
      unknownVersionPolicy: 'readonly',
      incompatibleVersionPolicy: 'readonly',
    },
    manifestSchemaVersion: 1,
    resourceSchemaVersion: 1,
    capabilitySchemaVersion: 1,
    memorySchemaVersion: 1,
    category,
    complexity,
    renderer,
    documentation: { readme: 'README.md', changelog: 'CHANGELOG.md' },
    requiredKernelPrimitives: ['resource-index-v1', 'draft-v1', 'diff-v1', 'validation-v1', 'capability-v1'],
    fileProjection: {
      keywords,
      ownedSelectors: [...new Set([id, ...keywords])],
      dependencySelectors: dependencies.map(systemId => ({ systemId })),
      excludes: ['**/.git/**', '**/node_modules/**', '**/.mir3-studio/**'],
      contentFingerprints: keywords.map(value => ({ contains: value, caseSensitive: false })),
      pathAliases: [{ from: 'client', to: '客户端' }, { from: 'engine', to: '引擎' }],
      roles: ['client', 'engine', 'shared', 'generated', 'readonly'],
      editableExtensions: id === 'map' ? ['txt', 'lua', 'map'] : ['txt', 'lua'],
      structuredExtensions: ['xls'],
      readonlyExtensions: ['png', 'plist', 'json', 'ini', 'cfg'],
      unknownFormatPolicy: 'readonly',
    },
    resources: {
      resourceTypes: [`${id}.${spec.resourceType}`],
      schema: 'schemas/resource.schema.json',
      stableResourceId: `sha256(${id}:${spec.fields[0].name}:normalizedRelativePath)`,
      mappings: ['file-projection', `${id}.field-mapping-v1`],
      fieldMappings: spec.fields.map(field => ({
        field: field.name,
        aliases: fieldAliases(field.name, field.aliases),
        valueType: field.type,
      })),
      dependencyEdges: references,
      uniqueKey,
    },
    presentation: {
      views: [renderer, 'source-v1', 'diff-v1', 'validation-v1'],
      primaryView: renderer,
      safePrimitive: primitive,
    },
    dependencies,
    operations,
    capabilities: operations.map(entry => ({
      ...entry,
      version,
      previewRequired: true,
      validationRequired: true,
      confirmationRequired: true,
    })),
    validators: [
      { id: `${id}.syntax`, kind: 'syntax', extensions: id === 'map' ? ['map', 'txt', 'lua', 'xls'] : ['txt', 'lua', 'xls'], encoding: 'project-detected' },
      { id: `${id}.schema`, kind: 'schema', schema: 'schemas/resource.schema.json', resourceType: spec.resourceType },
      { id: `${id}.unique`, kind: 'uniqueness', fields: uniqueKey, scope: `${id}.project` },
      { id: `${id}.range`, kind: 'range', fields: rangeFields },
      { id: `${id}.reference`, kind: 'reference-integrity', references },
      { id: `${id}.client-engine`, kind: 'client-engine-consistency', matchBy: spec.fields[0].name, compareFields: spec.consistencyFields, missingProjection: 'error' },
      { id: `${id}.runtime`, kind: 'runtime-diagnostics', rule: spec.runtimeRule, severity: 'error', target: `${id}.${spec.resourceType}` },
    ],
    fixtures: {
      valid: 'fixtures/valid.json',
      invalid: 'fixtures/invalid.json',
      expectedDiagnostics: 'fixtures/expected-diagnostics.json',
    },
  }
}

function completeOperationFamilies(systemId, capabilities) {
  const completed = [...capabilities]
  const families = [
    [/^(?:add|generate|insert|fill)-/, `add-${systemId}`],
    [/^clone-/, `clone-${systemId}`],
    [/^(?:batch|scale|tune|interpolate)-/, `batch-update-${systemId}`],
    [/^(?:replace|bind)-/, `replace-${systemId}-reference`],
  ]
  for (const [pattern, fallback] of families) {
    if (!completed.some(capabilityId => pattern.test(capabilityId)))
      completed.push(fallback)
  }
  return completed
}

function primitiveForRenderer(renderer) {
  if (renderer.includes('map') || renderer.includes('spatial'))
    return 'map'
  if (renderer.includes('graph') || renderer.includes('flow') || renderer.includes('topology'))
    return 'graph'
  if (renderer.includes('timeline') || renderer.includes('calendar'))
    return 'timeline'
  return 'xls'
}

function operationPrimitive(capabilityId, defaultPrimitive) {
  if (/(?:config|dialogue|drop-table|reference)$/.test(capabilityId))
    return 'text'
  return defaultPrimitive
}

function fieldSchema(field) {
  const schema = { type: field.type }
  for (const key of ['minimum', 'maximum', 'minLength', 'pattern', 'enum']) {
    if (field[key] !== undefined)
      schema[key] = field[key]
  }
  if (field.referenceSystem)
    schema['x-mir3-reference-system'] = field.referenceSystem
  return schema
}

function fieldAliases(fieldName, declaredAliases = []) {
  const words = fieldName
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .split(/[^a-z0-9]+/i)
    .filter(Boolean)
  const lowerWords = words.map(word => word.toLowerCase())
  const pascal = words.map(word => `${word.slice(0, 1).toUpperCase()}${word.slice(1)}`).join('')
  return [...new Set([
    fieldName,
    pascal,
    lowerWords.join('_'),
    lowerWords.join('-'),
    lowerWords.join(' '),
    ...declaredAliases,
  ])]
}

function resourceSchema(systemId, spec) {
  const uniqueKey = compoundUniqueKeys[systemId] || [spec.fields[0].name]
  return {
    '$schema': 'https://json-schema.org/draft/2020-12/schema',
    '$id': `mir3://domain/${systemId}/resource.schema.json`,
    'title': `${systemId}.${spec.resourceType}`,
    'type': 'object',
    'additionalProperties': false,
    'properties': Object.fromEntries(spec.fields.map(field => [field.name, fieldSchema(field)])),
    'required': spec.fields.map(field => field.name),
    'x-mir3': {
      uniqueKey,
      clientEngineConsistency: { matchBy: spec.fields[0].name, compareFields: spec.consistencyFields },
      runtimeRule: spec.runtimeRule,
    },
  }
}

function operation(systemId, dependencies, capabilityId, spec, primitive) {
  const readonly = /^(?:inspect|validate|preview)-/.test(capabilityId)
  return {
    id: capabilityId,
    parameterSchema: operationSchema(capabilityId, spec, readonly),
    readSystems: [...new Set([systemId, ...dependencies])],
    writeSystems: readonly ? [] : [systemId],
    preconditions: readonly
      ? [`${systemId}.resource-index-ready`, `${systemId}.schema-loaded`]
      : [`${systemId}.resource-index-ready`, `${systemId}.schema-loaded`, `${systemId}.draft-open`, `${systemId}.revision-match`],
    steps: [
      { primitive, action: readonly ? 'resolve-and-read' : 'resolve-and-preview', schema: 'schemas/resource.schema.json' },
      { primitive, action: readonly ? capabilityId : 'record-reversible-draft-step', operation: capabilityId },
    ],
    reversible: true,
    unknownFormatPolicy: 'readonly',
    previewPolicy: { previewRequired: true, validationRequired: true, confirmationRequired: !readonly },
  }
}

function operationSchema(capabilityId, spec, readonly) {
  const id = { type: 'string', minLength: 1 }
  const ids = { type: 'array', items: id, minItems: 1, maxItems: 10000, uniqueItems: true }
  const patch = {
    type: 'object',
    additionalProperties: false,
    minProperties: 1,
    properties: Object.fromEntries(spec.fields.slice(1).map(field => [field.name, fieldSchema(field)])),
  }
  const record = {
    type: 'object',
    additionalProperties: false,
    properties: Object.fromEntries(spec.fields.map(field => [field.name, fieldSchema(field)])),
    required: spec.fields.map(field => field.name),
  }
  const properties = { operation: { const: capabilityId } }
  const required = ['operation']
  if (capabilityId === 'edit-map-region') {
    const coordinateProperties = {
      x: { type: 'integer', minimum: 0, maximum: 4095 },
      y: { type: 'integer', minimum: 0, maximum: 4095 },
    }
    const mapEdit = (type, extraProperties, extraRequired) => ({
      type: 'object',
      additionalProperties: false,
      properties: { type: { const: type }, ...coordinateProperties, ...extraProperties },
      required: ['type', 'x', 'y', ...extraRequired],
    })
    Object.assign(properties, {
      resourceId: id,
      operations: {
        type: 'array',
        minItems: 1,
        maxItems: 20000,
        items: {
          oneOf: [
            mapEdit('setSprite', {
              layer: { enum: ['background', 'middle', 'front'] },
              library: { type: 'integer', minimum: -1, maximum: 32767 },
              image: { type: 'integer', minimum: 0, maximum: 65535 },
            }, ['layer', 'library', 'image']),
            mapEdit('clearSprite', {
              layer: { enum: ['background', 'middle', 'front'] },
            }, ['layer']),
            mapEdit('setCollision', {
              walkable: { type: 'boolean' },
              frontBlocked: { type: 'boolean' },
            }, ['walkable', 'frontBlocked']),
            mapEdit('setDoor', {
              doorIndex: { type: 'integer', minimum: 0, maximum: 255 },
              doorOffset: { type: 'integer', minimum: 0, maximum: 255 },
            }, ['doorIndex', 'doorOffset']),
            mapEdit('setAnimation', {
              middleFrames: { type: 'integer', minimum: 0, maximum: 255 },
              frontFrames: { type: 'integer', minimum: 0, maximum: 255 },
            }, ['middleFrames', 'frontFrames']),
          ],
        },
      },
    })
    required.push('resourceId', 'operations')
  }
  else if (capabilityId.startsWith('inspect-')) {
    Object.assign(properties, { resourceId: id, includeDependencies: { type: 'boolean', default: true }, projection: { enum: ['merged', 'client', 'engine'] } })
  }
  else if (capabilityId.startsWith('validate-')) {
    Object.assign(properties, { resourceIds: ids, targetEngineVersion: { type: 'string', pattern: '^\\d+\\.\\d+$' }, includeRuntimeDiagnostics: { type: 'boolean', default: true } })
    required.push('resourceIds', 'targetEngineVersion')
  }
  else if (capabilityId.startsWith('preview-')) {
    Object.assign(properties, { resourceIds: ids, sampleLimit: { type: 'integer', minimum: 1, maximum: 1000 }, includeDependencyValues: { type: 'boolean', default: true } })
    required.push('resourceIds')
  }
  else if (capabilityId.startsWith('clone-')) {
    Object.assign(properties, { sourceResourceId: id, newResourceId: id, overrides: patch })
    required.push('sourceResourceId', 'newResourceId')
  }
  else if (capabilityId.startsWith('replace-')) {
    Object.assign(properties, { resourceIds: ids, fromReference: id, toReference: id, referenceField: { enum: spec.fields.filter(field => field.referenceSystem).map(field => field.name) } })
    required.push('resourceIds', 'fromReference', 'toReference', 'referenceField')
  }
  else if (capabilityId.startsWith('batch-')) {
    Object.assign(properties, { resourceIds: ids, changes: patch, stopOnFirstError: { type: 'boolean', default: true } })
    required.push('resourceIds', 'changes')
  }
  else if (capabilityId.startsWith('generate-')) {
    Object.assign(properties, { templateResourceId: id, firstOrdinal: { type: 'integer', minimum: 0 }, lastOrdinal: { type: 'integer', minimum: 1, maximum: 10000 }, interpolation: { enum: ['linear', 'geometric', 'step'] }, generatedPatch: patch })
    required.push('templateResourceId', 'firstOrdinal', 'lastOrdinal')
  }
  else if (capabilityId.startsWith('scale-')) {
    Object.assign(properties, { resourceIds: ids, factor: { type: 'number', exclusiveMinimum: 0, maximum: 1000 }, rounding: { enum: ['nearest', 'floor', 'ceil'] }, fields: { type: 'array', items: { enum: spec.fields.filter(field => ['integer', 'number'].includes(field.type)).map(field => field.name) }, minItems: 1, uniqueItems: true } })
    required.push('resourceIds', 'factor', 'fields')
  }
  else if (capabilityId.startsWith('interpolate-')) {
    Object.assign(properties, { anchorResourceIds: ids, firstOrdinal: { type: 'integer', minimum: 0 }, lastOrdinal: { type: 'integer', minimum: 1, maximum: 10000 }, numericFields: { type: 'array', items: { enum: spec.fields.filter(field => ['integer', 'number'].includes(field.type)).map(field => field.name) }, minItems: 1 } })
    required.push('anchorResourceIds', 'firstOrdinal', 'lastOrdinal', 'numericFields')
  }
  else if (capabilityId.startsWith('add-')) {
    Object.assign(properties, { record, insertAfterResourceId: id })
    required.push('record')
  }
  else if (capabilityId.startsWith('insert-')) {
    Object.assign(properties, { parentResourceId: id, insertionIndex: { type: 'integer', minimum: 0, maximum: 1000000 }, record })
    required.push('parentResourceId', 'insertionIndex', 'record')
  }
  else if (capabilityId.startsWith('fill-')) {
    Object.assign(properties, { cycleResourceId: id, firstSlot: { type: 'integer', minimum: 1, maximum: 366 }, lastSlot: { type: 'integer', minimum: 1, maximum: 366 }, rewardTemplate: patch })
    required.push('cycleResourceId', 'firstSlot', 'lastSlot', 'rewardTemplate')
  }
  else if (capabilityId.startsWith('tune-')) {
    Object.assign(properties, { resourceIds: ids, adjustmentMode: { enum: ['absolute', 'delta', 'percentage'] }, amount: { type: 'number', minimum: -1000000000, maximum: 1000000000 }, fields: { type: 'array', items: { enum: spec.fields.filter(field => ['integer', 'number'].includes(field.type)).map(field => field.name) }, minItems: 1 } })
    required.push('resourceIds', 'adjustmentMode', 'amount', 'fields')
  }
  else if (capabilityId.startsWith('bind-')) {
    Object.assign(properties, { resourceId: id, targetReference: id, referenceField: { enum: spec.fields.filter(field => field.referenceSystem).map(field => field.name) }, replaceExisting: { type: 'boolean', default: false } })
    required.push('resourceId', 'targetReference', 'referenceField')
  }
  else if (capabilityId.startsWith('move-')) {
    Object.assign(properties, { resourceId: id, destinationMapId: id, coordinateX: { type: 'integer', minimum: 0, maximum: 4095 }, coordinateY: { type: 'integer', minimum: 0, maximum: 4095 } })
    required.push('resourceId', 'destinationMapId', 'coordinateX', 'coordinateY')
  }
  else if (capabilityId.startsWith('schedule-')) {
    Object.assign(properties, { resourceIds: ids, startEpochSeconds: { type: 'integer', minimum: 0, maximum: 4102444800 }, endEpochSeconds: { type: 'integer', minimum: 1, maximum: 4102444800 }, timezone: { type: 'string', minLength: 1 } })
    required.push('resourceIds', 'startEpochSeconds', 'endEpochSeconds', 'timezone')
  }
  else if (capabilityId.startsWith('shift-')) {
    Object.assign(properties, { resourceIds: ids, offsetSeconds: { type: 'integer', minimum: -315360000, maximum: 315360000 }, preserveDuration: { type: 'boolean', const: true } })
    required.push('resourceIds', 'offsetSeconds')
  }
  else {
    Object.assign(properties, { resourceIds: ids, changes: patch })
    required.push('resourceIds', 'changes')
  }
  if (!readonly) {
    properties.expectedRevision = { type: 'integer', minimum: 0 }
    required.push('expectedRevision')
  }
  return { type: 'object', additionalProperties: false, properties, required }
}

function validValue(systemId, field, ordinal) {
  if (field.referenceSystem)
    return `${field.referenceSystem}:fixture-${ordinal}`
  if (field.enum)
    return field.enum[Math.min(ordinal - 1, field.enum.length - 1)]
  if (field.type === 'integer' || field.type === 'number') {
    const minimum = field.minimum ?? 0
    const maximum = field.maximum ?? minimum + 100
    return Math.min(maximum, minimum + ordinal)
  }
  if (field.name.toLowerCase().includes('version'))
    return ordinal === 1 ? '1.8' : '2.0'
  if (field.name.toLowerCase().includes('path'))
    return `scripts/${systemId}-${ordinal}.lua`
  return `${systemId}-${field.name}-${ordinal}`
}

function fixtures(systemId, spec) {
  const uniqueKey = compoundUniqueKeys[systemId] || [spec.fields[0].name]
  const records = [1, 2].map(ordinal => Object.fromEntries(spec.fields.map(field => [field.name, validValue(systemId, field, ordinal)])))
  if (systemId === 'crafting') {
    for (const [index, record] of records.entries())
      record.materialItemId = `item:material-${index + 1}`
  }
  const invalidRecords = records.map(record => ({ ...record }))
  const ranged = spec.fields.find(field => field.maximum !== undefined)
  const patterned = spec.fields.find(field => field.pattern)
  const referenced = spec.fields.find(field => field.referenceSystem)
  if (ranged)
    invalidRecords[0][ranged.name] = ranged.maximum + 1
  else if (patterned)
    invalidRecords[0][patterned.name] = '!invalid!'
  if (referenced)
    invalidRecords[0][referenced.name] = `${referenced.referenceSystem}:missing`
  for (const field of uniqueKey)
    invalidRecords[1][field] = invalidRecords[0][field]
  invalidateRuntimeFixture(systemId, invalidRecords)
  const expectedDiagnostics = [
    { code: `${systemId}.unique`, severity: 'error', field: uniqueKey.join(',') },
    { code: ranged ? `${systemId}.range` : `${systemId}.schema`, severity: 'error', field: (ranged || patterned).name },
    { code: `${systemId}.reference`, severity: 'error', field: referenced.name },
    { code: `${systemId}.runtime`, severity: 'error', rule: spec.runtimeRule },
  ]
  const referenceSystems = [...new Set(spec.fields.flatMap(field => field.referenceSystem ? [field.referenceSystem] : []))]
  const referenceCatalog = Object.fromEntries(referenceSystems.map(referenceSystem => [
    referenceSystem,
    [...new Set(records.flatMap(record => spec.fields
      .filter(field => field.referenceSystem === referenceSystem)
      .map(field => record[field.name])))],
  ]))
  return {
    valid: { systemId, fixture: 'valid', records, referenceCatalog },
    invalid: {
      systemId,
      fixture: 'invalid',
      records: invalidRecords,
      referenceCatalog: {},
      runtimeAssertions: [{ rule: spec.runtimeRule, expected: false }],
    },
    expectedDiagnostics,
  }
}

function invalidateRuntimeFixture(systemId, records) {
  const first = records[0]
  const second = records[1]
  const setters = {
    map: () => { second.width = 0 },
    npc: () => { first.scriptPath = '../escape.lua' },
    monster: () => { first.healthPoints = 0 },
    equipment: () => { first.durability = 0 },
    item: () => { first.clientIcon = '../icon.png' },
    level: () => { second.requiredExperience = first.requiredExperience - 1 },
    rebirth: () => { second.minimumLevel = first.minimumLevel - 1 },
    title: () => {
      second.displayLabel = 'permanent'
      second.durationSeconds = 1
    },
    buff: () => {
      second.stackMode = 'stack'
      second.maximumStacks = 1
    },
    skill: () => { second.skillId = first.skillId },
    enhance: () => { first.successRateBasisPoints = 10001 },
    crafting: () => { first.materialItemId = first.outputItemId },
    gem: () => { second.socketType = first.socketType },
    refine: () => { first.minimumValue = first.maximumValue + 1 },
    quest: () => {
      first.nextQuestId = second.questId
      second.nextQuestId = first.questId
    },
    checkin: () => { second.cycleId = first.cycleId },
    online_reward: () => { second.durationSeconds = first.durationSeconds },
    limited_event: () => { second.startEpochSeconds = second.endEpochSeconds },
    launch_event: () => {
      second.scheduleId = first.scheduleId
      second.openServerDay = first.openServerDay
    },
    first_charge: () => { second.chargeThreshold = first.chargeThreshold },
    cumulative_charge: () => {
      second.cycleId = first.cycleId
      second.chargeThreshold = first.chargeThreshold
    },
    vip: () => { second.requiredPoints = first.requiredPoints - 1 },
    shop: () => { second.startEpochSeconds = second.endEpochSeconds },
    recycle: () => { second.minimumQuality = second.maximumQuality + 1 },
    guild: () => { second.maximumMembers = first.maximumMembers - 1 },
    sabac: () => { second.startMinute = second.endMinute },
    ranking: () => { second.cycleSeconds = 0 },
    resource_production: () => { second.intervalSeconds = 0 },
    manor: () => { first.productionPointId = first.entryNpcId },
    hero_soul: () => {
      first.routeId = first.nodeId
      second.routeId = first.nodeId
    },
    talent: () => {
      first.parentNodeId = second.nodeId
      second.parentNodeId = first.nodeId
    },
    season: () => { second.startEpochSeconds = second.endEpochSeconds },
    cross_server: () => { first.targetShard = first.sourceShard },
  }
  const invalidate = setters[systemId]
  if (!invalidate)
    throw new Error(`Missing runtime fixture mutator for ${systemId}`)
  invalidate()
}

const packs = packDefinitions.map(definition => defineDomainManifest(createPack(definition)))

mkdirSync(outputRoot, { recursive: true })
writeFileSync(join(outputRoot, 'registry.json'), `${JSON.stringify({ schemaVersion: 1, packs }, null, 2)}\n`)

for (const entry of packs) {
  const directory = join(outputRoot, entry.systemId)
  const schemaDirectory = join(directory, 'schemas')
  const fixtureDirectory = join(directory, 'fixtures')
  mkdirSync(schemaDirectory, { recursive: true })
  mkdirSync(fixtureDirectory, { recursive: true })
  const spec = domainSpecs[entry.systemId]
  const examples = defineDomainFixtures(fixtures(entry.systemId, spec))
  const manifest = {
    name: `@mir3-studio/domain-${entry.systemId}`,
    kind: 'domain',
    version: entry.version,
    private: true,
    files: ['CHANGELOG.md', 'README.md', 'domain.json', 'fixtures/', 'schemas/'],
    mir3Domain: {
      kind: entry.kind,
      systemId: entry.systemId,
      kernelApiRange: entry.kernelApiRange,
      supportedEngineRange: entry.supportedEngineRange,
      engineCompatibility: entry.engineCompatibility,
      resourceSchema: 'schemas/resource.schema.json',
      fixtures: entry.fixtures,
      changelog: 'CHANGELOG.md',
    },
  }
  const fieldList = spec.fields.map(field => `- \`${field.name}\`: ${field.type}${field.referenceSystem ? ` → ${field.referenceSystem}` : ''}`).join('\n')
  const capabilityList = entry.capabilities.map(capability => `- \`${capability.id}\` via \`${capability.steps[0].primitive}\``).join('\n')
  writeFileSync(join(directory, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
  writeFileSync(join(directory, 'domain.json'), `${JSON.stringify(entry, null, 2)}\n`)
  writeFileSync(join(schemaDirectory, 'resource.schema.json'), `${JSON.stringify(resourceSchema(entry.systemId, spec), null, 2)}\n`)
  writeFileSync(join(fixtureDirectory, 'valid.json'), `${JSON.stringify(examples.valid, null, 2)}\n`)
  writeFileSync(join(fixtureDirectory, 'invalid.json'), `${JSON.stringify(examples.invalid, null, 2)}\n`)
  writeFileSync(join(fixtureDirectory, 'expected-diagnostics.json'), `${JSON.stringify(examples.expectedDiagnostics, null, 2)}\n`)
  const compatibility = `Pack version: \`${entry.version}\`; compiler compatibility: MIR3 System Kernel \`${entry.kernelApiRange}\`; engine range: \`${entry.supportedEngineRange}\`.`
  writeFileSync(join(directory, 'README.md'), `# ${entry.systemId}\n\nMIR3 Studio ${entry.systemId} domain pack for MIR3 System Kernel v1. ${compatibility} Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.\n\n## Resource schema\n\n${fieldList}\n\nUnique key: \`${entry.resources.uniqueKey.join(' + ')}\`. Runtime rule: \`${spec.runtimeRule}\`.\n\n## Capabilities\n\n${capabilityList}\n\n## Contract fixtures\n\nThe \`fixtures/valid.json\` and \`fixtures/invalid.json\` corpora are checked against \`schemas/resource.schema.json\`; expected validator output is in \`fixtures/expected-diagnostics.json\`.\n`)
  const mapChangelog = entry.systemId === 'map' ? `## 1.0.1\n\n- Added the closed, structured \`edit-map-region\` parameter contract for scoped binary map Draft edits.\n\n` : ''
  writeFileSync(join(directory, 'CHANGELOG.md'), `# Changelog\n\n## 1.3.0\n\n- Added executable schema-backed field mappings with declared aliases and scalar types; unknown or ambiguous columns remain read-only.\n- Resource projection now preserves canonical fields for validation, cross-system references, and structured operations.\n\n## 1.2.0\n\n- Replaced the wildcard engine declaration with evidence-gated automatic generalization for recognized SemVer aliases.\n- Made unknown and incompatible engine versions explicitly read-only before Draft writes and final Apply.\n\n## 1.1.0\n\n- Completed the registered create, clone, batch-update, and reference-replacement operation families with closed parameter schemas and Draft safety gates.\n- Kept all writes scoped to this domain and compiled only through registered safe primitives.\n\n${mapChangelog}## 1.0.0\n\n- Added the ${spec.resourceType} resource schema with typed fields, unique keys, references, client/engine consistency, and runtime diagnostics.\n- Added parameterized safe operations backed by the ${entry.presentation.safePrimitive} primitive.\n- Added valid and invalid contract fixtures with expected diagnostics.\n`)
}

const sdkExample = packs.find(entry => entry.systemId === 'level')
const sdkExampleRoot = join(root, 'src-tauri', 'resources', 'mir3-domain-sdk', 'fixtures', 'example-pack')
const sdkExampleSpec = domainSpecs.level
const sdkExampleFixtures = defineDomainFixtures(fixtures('level', sdkExampleSpec))
mkdirSync(join(sdkExampleRoot, 'schemas'), { recursive: true })
mkdirSync(join(sdkExampleRoot, 'fixtures'), { recursive: true })
writeFileSync(join(sdkExampleRoot, 'domain.json'), `${JSON.stringify(sdkExample, null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'schemas', 'resource.schema.json'), `${JSON.stringify(resourceSchema('level', sdkExampleSpec), null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'fixtures', 'valid.json'), `${JSON.stringify(sdkExampleFixtures.valid, null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'fixtures', 'invalid.json'), `${JSON.stringify(sdkExampleFixtures.invalid, null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'fixtures', 'expected-diagnostics.json'), `${JSON.stringify(sdkExampleFixtures.expectedDiagnostics, null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'package.json'), `${JSON.stringify({
  name: '@mir3-studio/domain-sdk-example-level',
  kind: 'domain',
  version: sdkExample.version,
  private: true,
  files: ['CHANGELOG.md', 'README.md', 'domain.json', 'fixtures/', 'schemas/'],
  mir3Domain: {
    kind: 'domain',
    systemId: 'level',
    kernelApiRange: sdkExample.kernelApiRange,
    supportedEngineRange: sdkExample.supportedEngineRange,
    engineCompatibility: sdkExample.engineCompatibility,
    resourceSchema: 'schemas/resource.schema.json',
    fixtures: sdkExample.fixtures,
    changelog: 'CHANGELOG.md',
  },
}, null, 2)}\n`)
writeFileSync(join(sdkExampleRoot, 'CHANGELOG.md'), '# Changelog\n\n## 1.3.0 - 2026-08-27\n\n- Added executable schema-backed field mappings to the runtime-installable example.\n')

process.stdout.write(`Generated ${packs.length} MIR3 domain packs with schemas and contract fixtures.\n`)
