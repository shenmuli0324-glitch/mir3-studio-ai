import type { TaskReceipt } from '../src/features/devtools/domain/types'
import { describe, expect, it } from 'vitest'
import { appendScopedUserRequest, buildGlobalTaskHandoff, buildTaskSemanticSummary, formatTaskReceiptSummary, parseGlobalTaskHandoff, projectTaskMessages } from '../src/features/system-ai/global-task-handoff'

const ALL_SYSTEMS = [
  'level',
  'title',
  'checkin',
  'online_reward',
  'first_charge',
  'cumulative_charge',
  'ranking',
  'item',
  'equipment',
  'monster',
  'npc',
  'buff',
  'vip',
  'shop',
  'recycle',
  'enhance',
  'crafting',
  'gem',
  'refine',
  'rebirth',
  'skill',
  'talent',
  'resource_production',
  'hero_soul',
  'quest',
  'limited_event',
  'launch_event',
  'guild',
  'manor',
  'season',
  'map',
  'sabac',
  'cross_server',
]

describe('global task semantic handoff', () => {
  it('projects only user requests and assistant text while hiding internal runtime nodes', () => {
    const userRequest = '只读列出当前地图系统相关文件，不要修改任何文件'
    const messages = projectTaskMessages([
      { kind: 'context', content: '[MIR3 System Scope] private runtime context' },
      { kind: 'user', content: appendScopedUserRequest(['[MIR3 System Scope] scopeToken=secret'], userRequest) },
      { kind: 'tool-result', content: JSON.stringify({ rows: [{ path: '引擎/Mir200/Envir/Data/cfg_mapinfo.xls' }] }) },
      {
        kind: 'assistant',
        blocks: [
          { kind: 'thinking', text: 'internal reasoning' },
          { kind: 'text', text: '已找到地图配置与地图脚本。' },
          { kind: 'tool-call', text: 'mir3_resource_query payload' },
        ],
      },
    ], {
      turn: 2,
      step: 1,
      blocks: [{ kind: 'text', text: '正在整理文件列表…' }],
    })

    expect(messages).toEqual([
      { id: 'node-1', role: 'user', content: userRequest },
      { id: 'node-3', role: 'assistant', content: '已找到地图配置与地图脚本。' },
      { id: 'partial-2-1', role: 'assistant', content: '正在整理文件列表…' },
    ])
    expect(JSON.stringify(messages)).not.toContain('scopeToken')
    expect(JSON.stringify(messages)).not.toContain('cfg_mapinfo.xls')
    expect(JSON.stringify(messages)).not.toContain('internal reasoning')
    expect(JSON.stringify(messages)).not.toContain('mir3_resource_query payload')
  })

  it('keeps credentials and raw assistant chat out of the real scoped-prompt semantic pipeline', () => {
    const systemScopeToken = 'scope-secret-system-123'
    const toolScopeToken = 'scope-secret-tool-456'
    const bearerToken = 'bearer-secret-789'
    const rawAssistantMessage = 'RAW_ASSISTANT_TRANSCRIPT_MUST_NOT_PERSIST'
    const userRequest = 'Batch-adjust shop prices and preserve item references'
    const scopedPrompt = appendScopedUserRequest([
      `[MIR3 System Scope] project=project-1; system=shop; scopeToken=${systemScopeToken}.`,
      '[Activated domain memories]\n- Keep aliases stable',
    ], userRequest)
    expect(scopedPrompt).toContain(systemScopeToken)

    const messages = projectTaskMessages([
      { id: 'user-1', role: 'user', content: scopedPrompt },
      { id: 'assistant-1', role: 'assistant', content: rawAssistantMessage },
    ])
    const semanticSummary = buildTaskSemanticSummary({
      messages,
      decisions: [`authorization: Bearer ${bearerToken}`],
      completedOperations: [`mir3_validate scopeToken=${toolScopeToken}`],
      constraints: ['Do not modify generated files'],
      unfinishedSteps: ['Review the composite Diff'],
    })
    const receiptSummary = formatTaskReceiptSummary(semanticSummary)
    const handoff = buildGlobalTaskHandoff({
      source: source(),
      explicitSummary: semanticSummary,
      references: { draftIds: ['draft-shop-1'] },
      pluginVersions: { shop: '1.3.1' },
      allowedReadSystems: ['shop'],
      allowedWriteSystems: ['shop'],
    })
    const persisted = JSON.stringify({ receiptSummary, semanticSummary, handoff })

    expect(semanticSummary.goal).toBe(userRequest)
    expect(receiptSummary).toBe(userRequest)
    expect(persisted).toContain('[REDACTED_CREDENTIAL]')
    expect(persisted).not.toContain(systemScopeToken)
    expect(persisted).not.toContain(toolScopeToken)
    expect(persisted).not.toContain(bearerToken)
    expect(persisted).not.toContain('scopeToken')
    expect(persisted).not.toContain(rawAssistantMessage)
    expect(persisted).not.toContain('[MIR3 System Scope]')
  })

  it('projects task state, Receipt and explicit semantics without copying raw messages', () => {
    const rawAssistantMessage = 'RAW_ASSISTANT_SECRET_SHOULD_NOT_CROSS_THE_BOUNDARY'
    const input = {
      source: source(),
      explicitSummary: {
        goal: 'Batch-adjust shop prices while preserving item references',
        decisions: ['Use one atomic composite Draft'],
        constraints: ['Keep prices non-negative'],
      },
      taskState: {
        completedOperations: ['mir3_draft_operate:update-price'],
        openQuestions: ['Should VIP discounts also change?'],
        unfinishedSteps: ['Validate the item dependency'],
      },
      receipts: [receipt()],
      references: {
        resourceIds: ['shop:item:1001'],
        relativePaths: ['Data/Shop/Items.txt'],
        draftIds: ['draft-shop-1'],
      },
      pluginVersions: { shop: '1.3.1', item: '1.3.1', monster: '9.9.9' },
      allowedReadSystems: ['shop', 'item'],
      allowedWriteSystems: ['shop'],
      rawMessages: [
        { role: 'user', content: 'raw user content' },
        { role: 'assistant', content: rawAssistantMessage },
      ],
    }
    const handoff = buildGlobalTaskHandoff(input)

    expect(handoff).toMatchObject({
      goal: 'Batch-adjust shop prices while preserving item references',
      decisions: ['Use one atomic composite Draft', 'Keep the current item aliases'],
      completedOperations: ['mir3_draft_operate:update-price', 'mir3_validate'],
      constraints: ['Keep prices non-negative', 'Do not modify generated files'],
      openQuestions: ['Should VIP discounts also change?'],
      unfinishedSteps: ['Validate the item dependency', 'Apply after user confirmation'],
      references: {
        receiptIds: ['receipt-shop-1'],
        resourceIds: ['shop:item:1001'],
        relativePaths: ['Data/Shop/Items.txt'],
        draftIds: ['draft-shop-1'],
      },
      pluginVersions: { shop: '1.3.1', item: '1.3.1' },
      scope: {
        allowedReadSystems: ['shop', 'item'],
        allowedWriteSystems: ['shop'],
      },
    })
    const serialized = JSON.stringify(handoff)
    expect(serialized).not.toContain('rawMessages')
    expect(serialized).not.toContain(rawAssistantMessage)
    expect(serialized).not.toContain('raw user content')
    expect(serialized).not.toContain('monster')
  })

  it.each([3, 8, 33])('retains an exact safe scope and plugin map for %i systems', (systemCount) => {
    const systems = ALL_SYSTEMS.slice(0, systemCount)
    const pluginVersions = Object.fromEntries(systems.map(systemId => [systemId, '1.3.1']))
    const handoff = buildGlobalTaskHandoff({
      source: { ...source(), systemId: systems[0] },
      explicitSummary: { goal: `Coordinate ${systemCount} systems` },
      references: { draftIds: systems.map(systemId => `draft-${systemId}`) },
      pluginVersions,
      allowedReadSystems: systems,
      allowedWriteSystems: systems,
    })

    expect(handoff.scope.allowedReadSystems).toEqual(systems)
    expect(handoff.scope.allowedWriteSystems).toEqual(systems)
    expect(Object.keys(handoff.pluginVersions)).toEqual(systems)
    expect(handoff.references.draftIds).toHaveLength(systemCount)
    expect(parseGlobalTaskHandoff(JSON.parse(JSON.stringify(handoff)))).toEqual(handoff)
  })

  it('fails closed when a restored handoff broadens write scope or lacks a pinned plugin version', () => {
    const valid = buildGlobalTaskHandoff({
      source: source(),
      explicitSummary: { goal: 'Update shop' },
      references: { draftIds: ['draft-shop-1'] },
      pluginVersions: { shop: '1.3.1', item: '1.3.1' },
      allowedReadSystems: ['shop', 'item'],
      allowedWriteSystems: ['shop'],
    })
    expect(parseGlobalTaskHandoff({
      ...valid,
      scope: { ...valid.scope, allowedWriteSystems: ['shop', 'map'] },
    })).toBeNull()
    expect(parseGlobalTaskHandoff({
      ...valid,
      pluginVersions: { shop: '1.3.1' },
    })).toBeNull()
  })

  it('redacts credentials from restored semantic text before it can be persisted again', () => {
    const valid = buildGlobalTaskHandoff({
      source: source(),
      explicitSummary: { goal: 'Update shop' },
      references: { draftIds: ['draft-shop-1'] },
      pluginVersions: { shop: '1.3.1' },
      allowedReadSystems: ['shop'],
      allowedWriteSystems: ['shop'],
    })
    const restored = parseGlobalTaskHandoff({
      ...valid,
      decisions: ['use scopeToken=persisted-secret until completion'],
    })

    expect(restored?.decisions).toEqual(['use [REDACTED_CREDENTIAL] until completion'])
    expect(JSON.stringify(restored)).not.toContain('persisted-secret')
    expect(JSON.stringify(restored)).not.toContain('scopeToken')
  })

  it('requires the real source system session identity instead of accepting a placeholder', () => {
    expect(() => buildGlobalTaskHandoff({
      source: { ...source(), sessionId: '' },
      explicitSummary: { goal: 'Update shop' },
      references: { draftIds: ['draft-shop-1'] },
      pluginVersions: { shop: '1.3.1' },
      allowedReadSystems: ['shop'],
      allowedWriteSystems: ['shop'],
    })).toThrow('GLOBAL_TASK_HANDOFF_SOURCE_INVALID')
  })
})

function source() {
  return {
    projectId: 'project-1',
    systemId: 'shop',
    taskId: 'system-task-1',
    sessionId: 'system-session-1',
  }
}

function receipt(): TaskReceipt {
  return {
    id: 'receipt-shop-1',
    taskId: 'system-task-1',
    systemId: 'shop',
    summary: 'user: legacy transcript line\nassistant: this must not be copied',
    status: 'applied',
    draftId: 'draft-shop-1',
    pluginVersions: { shop: '1.3.1' },
    evidence: {
      toolCalls: ['mir3_validate'],
      semanticSummary: {
        goal: 'Adjust prices',
        decisions: ['Keep the current item aliases'],
        constraints: ['Do not modify generated files'],
        unfinishedSteps: ['Apply after user confirmation'],
      },
    },
    createdAt: 1_700_000_000_000,
  }
}
