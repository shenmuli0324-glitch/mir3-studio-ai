import type { TaskReceipt } from '@/features/devtools/domain/types'

export interface GlobalTaskSemanticSummary {
  goal?: string | null
  decisions?: string[]
  completedOperations?: string[]
  constraints?: string[]
  openQuestions?: string[]
  unfinishedSteps?: string[]
}

export interface TaskSemanticMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
}

export interface TaskSemanticSummaryInput {
  messages: TaskSemanticMessage[]
  decisions?: unknown[]
  completedOperations?: unknown[]
  constraints?: unknown[]
  openQuestions?: unknown[]
  unfinishedSteps?: unknown[]
}

export interface GlobalTaskHandoff {
  schemaVersion: 1
  source: {
    projectId: string
    systemId: string
    taskId: string
    sessionId: string
  }
  goal: string
  decisions: string[]
  completedOperations: string[]
  constraints: string[]
  openQuestions: string[]
  unfinishedSteps: string[]
  references: {
    receiptIds: string[]
    resourceIds: string[]
    relativePaths: string[]
    draftIds: string[]
  }
  pluginVersions: Record<string, string>
  scope: {
    allowedReadSystems: string[]
    allowedWriteSystems: string[]
  }
}

export interface GlobalTaskHandoffInput {
  source: GlobalTaskHandoff['source']
  explicitSummary?: GlobalTaskSemanticSummary | null
  taskState?: GlobalTaskSemanticSummary | null
  receipts?: TaskReceipt[]
  references?: Partial<GlobalTaskHandoff['references']>
  pluginVersions: Record<string, string>
  allowedReadSystems: string[]
  allowedWriteSystems: string[]
}

const MAX_SYSTEMS = 33
const MAX_SUMMARY_ITEMS = 256
const USER_REQUEST_START = '[MIR3 User Request JSON]'
const USER_REQUEST_END = '[/MIR3 User Request JSON]'
const CREDENTIAL_REDACTION = '[REDACTED_CREDENTIAL]'
const SENSITIVE_ASSIGNMENT_PATTERN = /["'`]?(?:scope[_-]?token|lease[_-]?token|access[_-]?token|refresh[_-]?token|api[_-]?key|authorization)["'`]?\s*[:=]\s*(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s;,}\]]+)/giu
const BEARER_CREDENTIAL_PATTERN = /\bBearer\s+[\w.~+/=-]+/giu

/**
 * 把用户正文放入可无歧义解析的 JSON 标记区；作用域上下文与正文由此保持不同数据边界。
 */
export function appendScopedUserRequest(context: string[], content: string): string {
  return [
    ...context,
    USER_REQUEST_START,
    JSON.stringify(content),
    USER_REQUEST_END,
  ].join('\n')
}

/** 从 Harness Snapshot 节点投影会话展示消息，不在这里推断任务语义。 */
export function projectTaskMessages(nodes: unknown[]): TaskSemanticMessage[] {
  const messages: TaskSemanticMessage[] = []
  nodes.forEach((node, index) => {
    if (!node || typeof node !== 'object')
      return
    const record = node as Record<string, unknown>
    const content = nodeTextContent(record.content ?? record.text ?? record.message)
    if (!content)
      return
    messages.push({
      id: String(record.id ?? `node-${index}`),
      role: record.role === 'user' ? 'user' : 'assistant',
      content,
    })
  })
  return messages
}

/**
 * 只从明确标记的用户正文生成任务目标；兼容尚未收到 Snapshot 的本地乐观消息，拒绝旧版作用域 Prompt。
 */
export function taskGoalFromMessages(messages: TaskSemanticMessage[]): string | null {
  for (const message of messages) {
    if (message.role !== 'user')
      continue
    const marked = markedUserRequest(message.content)
    if (marked != null)
      return summaryText(marked)
    if (message.content.includes('[MIR3 System Scope]') || message.content.includes('[MIR3 Scope Renewal]'))
      continue
    const safe = summaryText(message.content)
    if (safe)
      return safe
  }
  return null
}

/** 统一构造 Receipt 与 handoff 使用的结构化语义，所有自由文本均经过凭证脱敏。 */
export function buildTaskSemanticSummary(input: TaskSemanticSummaryInput): GlobalTaskSemanticSummary {
  return {
    goal: taskGoalFromMessages(input.messages),
    decisions: summaryList(input.decisions ?? []),
    completedOperations: summaryList(input.completedOperations ?? []),
    constraints: summaryList(input.constraints ?? []),
    openQuestions: summaryList(input.openQuestions ?? []),
    unfinishedSteps: summaryList(input.unfinishedSteps ?? []),
  }
}

/** 对需要单独持久化的恢复错误等文本复用同一凭证脱敏规则。 */
export function sanitizeTaskSemanticText(value: unknown): string | null {
  return summaryText(value)
}

/** Receipt 的 summary 只保存结构化语义摘要，不再保存最近聊天原文。 */
export function formatTaskReceiptSummary(summary: GlobalTaskSemanticSummary): string {
  const normalized = normalizeSemanticSummary(summary)
  return normalized.goal ?? ''
}

/**
 * 将系统任务投影成可恢复的语义交接对象；输入不会被原样转发，避免把聊天记录复制到全局会话。
 */
export function buildGlobalTaskHandoff(input: GlobalTaskHandoffInput): GlobalTaskHandoff {
  const source = parseSource(input.source)
  if (!source)
    throw new Error('GLOBAL_TASK_HANDOFF_SOURCE_INVALID')
  const allowedReadSystems = identifierList(input.allowedReadSystems, 64, MAX_SYSTEMS)
  const allowedWriteSystems = identifierList(input.allowedWriteSystems, 64, MAX_SYSTEMS)
  if (allowedReadSystems.length === 0 || allowedWriteSystems.length === 0
    || !allowedReadSystems.includes(source.systemId)
    || allowedWriteSystems.some(systemId => !allowedReadSystems.includes(systemId))) {
    throw new Error('GLOBAL_TASK_HANDOFF_SCOPE_INVALID')
  }
  const pluginVersions = scopedPluginVersions(input.pluginVersions, allowedReadSystems)
  if (Object.keys(pluginVersions).length !== allowedReadSystems.length)
    throw new Error('GLOBAL_TASK_HANDOFF_PLUGIN_VERSION_MISSING')

  const receipts = (input.receipts ?? [])
    .filter(receipt => receipt.taskId === source.taskId && receipt.systemId === source.systemId)
    .sort((left, right) => right.createdAt - left.createdAt)
    .slice(0, 32)
  const receiptSummaries = receipts.map(receiptSemanticSummary)
  const summaries = [input.explicitSummary, input.taskState, ...receiptSummaries]
    .map(normalizeSemanticSummary)
  const goal = firstSummaryText(summaries.map(summary => summary?.goal))
    ?? `Continue ${source.systemId} task ${source.taskId}`

  return {
    schemaVersion: 1,
    source,
    goal,
    decisions: mergeSummaryLists(summaries, 'decisions'),
    completedOperations: mergeSummaryLists(summaries, 'completedOperations'),
    constraints: mergeSummaryLists(summaries, 'constraints'),
    openQuestions: mergeSummaryLists(summaries, 'openQuestions'),
    unfinishedSteps: mergeSummaryLists(summaries, 'unfinishedSteps'),
    references: {
      receiptIds: identifierList([
        ...(input.references?.receiptIds ?? []),
        ...receipts.map(receipt => receipt.id),
      ], 200, 64),
      resourceIds: identifierList(input.references?.resourceIds ?? [], 256, 10_000),
      relativePaths: relativePathList(input.references?.relativePaths ?? []),
      draftIds: identifierList([
        ...(input.references?.draftIds ?? []),
        ...receipts.flatMap(receipt => receipt.draftId ? [receipt.draftId] : []),
      ], 160, 128),
    },
    pluginVersions,
    scope: {
      allowedReadSystems,
      allowedWriteSystems,
    },
  }
}

/** 读取本地恢复数据时执行与创建阶段相同的封闭校验。 */
export function parseGlobalTaskHandoff(value: unknown): GlobalTaskHandoff | null {
  const record = asRecord(value)
  const source = parseSource(record?.source)
  const scope = asRecord(record?.scope)
  const references = asRecord(record?.references)
  if (record?.schemaVersion !== 1 || !source || !scope || !references)
    return null
  const allowedReadSystems = identifierList(scope.allowedReadSystems, 64, MAX_SYSTEMS)
  const allowedWriteSystems = identifierList(scope.allowedWriteSystems, 64, MAX_SYSTEMS)
  if (allowedReadSystems.length === 0 || allowedWriteSystems.length === 0
    || !allowedReadSystems.includes(source.systemId)
    || allowedWriteSystems.some(systemId => !allowedReadSystems.includes(systemId))) {
    return null
  }
  const pluginVersions = scopedPluginVersions(record.pluginVersions, allowedReadSystems)
  if (Object.keys(pluginVersions).length !== allowedReadSystems.length)
    return null
  const goal = summaryText(record.goal)
  const receiptIds = identifierList(references.receiptIds, 200, 64)
  const resourceIds = identifierList(references.resourceIds, 256, 10_000)
  const draftIds = identifierList(references.draftIds, 160, 128)
  const relativePaths = relativePathList(references.relativePaths)
  if (!goal || !exactArrayInput(references.receiptIds, receiptIds)
    || !exactArrayInput(references.resourceIds, resourceIds)
    || !exactArrayInput(references.draftIds, draftIds)
    || !exactArrayInput(references.relativePaths, relativePaths)) {
    return null
  }
  const decisions = summaryList(record.decisions)
  const completedOperations = summaryList(record.completedOperations)
  const constraints = summaryList(record.constraints)
  const openQuestions = summaryList(record.openQuestions)
  const unfinishedSteps = summaryList(record.unfinishedSteps)
  if (!exactArrayInput(record.decisions, decisions)
    || !exactArrayInput(record.completedOperations, completedOperations)
    || !exactArrayInput(record.constraints, constraints)
    || !exactArrayInput(record.openQuestions, openQuestions)
    || !exactArrayInput(record.unfinishedSteps, unfinishedSteps)) {
    return null
  }
  return {
    schemaVersion: 1,
    source,
    goal,
    decisions,
    completedOperations,
    constraints,
    openQuestions,
    unfinishedSteps,
    references: { receiptIds, resourceIds, relativePaths, draftIds },
    pluginVersions,
    scope: { allowedReadSystems, allowedWriteSystems },
  }
}

function receiptSemanticSummary(receipt: TaskReceipt): GlobalTaskSemanticSummary {
  const semantic = asRecord(receipt.evidence.semanticSummary)
  const evidenceOperations = summaryList(receipt.evidence.toolCalls)
  return {
    goal: summaryText(semantic?.goal),
    decisions: summaryList(semantic?.decisions),
    completedOperations: uniqueSummaryItems([
      ...summaryList(semantic?.completedOperations),
      ...evidenceOperations,
    ]),
    constraints: summaryList(semantic?.constraints),
    openQuestions: summaryList(semantic?.openQuestions),
    unfinishedSteps: summaryList(semantic?.unfinishedSteps),
  }
}

function mergeSummaryLists(summaries: Array<GlobalTaskSemanticSummary | null | undefined>, key: Exclude<keyof GlobalTaskSemanticSummary, 'goal'>): string[] {
  return uniqueSummaryItems(summaries.flatMap(summary => summary?.[key] ?? []))
}

function uniqueSummaryItems(values: unknown[]): string[] {
  const items = values.flatMap((value) => {
    const text = summaryText(value)
    return text ? [text] : []
  })
  return [...new Set(items)].slice(0, MAX_SUMMARY_ITEMS)
}

function firstSummaryText(values: unknown[]): string | null {
  for (const value of values) {
    const text = summaryText(value)
    if (text)
      return text
  }
  return null
}

function summaryList(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > MAX_SUMMARY_ITEMS)
    return []
  return uniqueSummaryItems(value)
}

function summaryText(value: unknown): string | null {
  if (typeof value !== 'string')
    return null
  const normalized = [...value].filter((character) => {
    const code = character.charCodeAt(0)
    return code === 9 || code === 10 || code === 13 || (code >= 32 && code !== 127)
  }).join('').trim()
  if (!normalized)
    return null
  const redacted = normalized
    .replace(BEARER_CREDENTIAL_PATTERN, CREDENTIAL_REDACTION)
    .replace(SENSITIVE_ASSIGNMENT_PATTERN, CREDENTIAL_REDACTION)
    .trim()
  return redacted ? redacted.slice(0, 1_000) : null
}

function normalizeSemanticSummary(summary: GlobalTaskSemanticSummary | null | undefined): GlobalTaskSemanticSummary {
  return {
    goal: summaryText(summary?.goal),
    decisions: summaryList(summary?.decisions),
    completedOperations: summaryList(summary?.completedOperations),
    constraints: summaryList(summary?.constraints),
    openQuestions: summaryList(summary?.openQuestions),
    unfinishedSteps: summaryList(summary?.unfinishedSteps),
  }
}

function markedUserRequest(content: string): string | null {
  const start = content.lastIndexOf(`${USER_REQUEST_START}\n`)
  if (start < 0)
    return null
  const jsonStart = start + USER_REQUEST_START.length + 1
  const end = content.indexOf(`\n${USER_REQUEST_END}`, jsonStart)
  if (end < 0)
    return null
  try {
    const value = JSON.parse(content.slice(jsonStart, end)) as unknown
    return typeof value === 'string' ? value : null
  }
  catch {
    return null
  }
}

function nodeTextContent(value: unknown): string {
  if (typeof value === 'string')
    return value
  if (Array.isArray(value))
    return value.map(nodeTextContent).filter(Boolean).join('\n')
  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>
    return nodeTextContent(record.text ?? record.content ?? record.value)
  }
  return ''
}

function parseSource(value: unknown): GlobalTaskHandoff['source'] | null {
  const record = asRecord(value)
  if (!record)
    return null
  const projectId = identifier(record.projectId, 160)
  const systemId = identifier(record.systemId, 64)
  const taskId = identifier(record.taskId, 200)
  const sessionId = identifier(record.sessionId, 200)
  if (!projectId || !systemId || !taskId || !sessionId)
    return null
  return { projectId, systemId, taskId, sessionId }
}

function scopedPluginVersions(value: unknown, allowedSystems: string[]): Record<string, string> {
  const record = asRecord(value)
  if (!record)
    return {}
  const versions: Record<string, string> = {}
  for (const systemId of allowedSystems) {
    const version = semanticVersion(record[systemId])
    if (version)
      versions[systemId] = version
  }
  return versions
}

function semanticVersion(value: unknown): string | null {
  if (typeof value !== 'string' || value.length > 64 || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(value))
    return null
  return value
}

function identifierList(value: unknown, maxLength: number, maximumItems: number): string[] {
  if (!Array.isArray(value) || value.length > maximumItems)
    return []
  const values = value.flatMap((item) => {
    const parsed = identifier(item, maxLength)
    return parsed ? [parsed] : []
  })
  if (values.length !== value.length)
    return []
  return [...new Set(values)]
}

function relativePathList(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > 10_000)
    return []
  const paths = value.flatMap((item) => {
    const path = relativePath(item)
    return path ? [path] : []
  })
  if (paths.length !== value.length)
    return []
  return [...new Set(paths)]
}

function relativePath(value: unknown): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > 1_024 || value.includes('\0'))
    return null
  const normalized = value.replaceAll('\\', '/')
  if (normalized.startsWith('/') || /^[A-Za-z]:\//u.test(normalized) || normalized.split('/').includes('..'))
    return null
  return normalized
}

function identifier(value: unknown, maxLength: number): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > maxLength)
    return null
  if (!/^[\w:./@-]+$/u.test(value) || value.includes('..') || value.includes('\\'))
    return null
  return value
}

function exactArrayInput(value: unknown, parsed: string[]): boolean {
  return Array.isArray(value) && value.length === parsed.length
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    return null
  return value as Record<string, unknown>
}
