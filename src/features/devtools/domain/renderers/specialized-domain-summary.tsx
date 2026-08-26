import type { DomainResourceRecord } from '../types'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { projectionTable } from '../projection-model'

interface ProjectionTable {
  columns: string[]
  rows: string[][]
  totalRows: number
}

interface MetricValue {
  label: string
  value: string | number
}

interface RouteValue {
  id: string
  source: string
  target: string
  compatibility: string
}

export function SpecializedDomainSummary({ resource }: { resource: DomainResourceRecord }) {
  const table = resource.projection ? projectionTable(resource.projection) : null
  if (!table)
    return null
  if (resource.systemId === 'quest')
    return <QuestSummary table={table} resource={resource} />
  if (resource.systemId === 'talent')
    return <TalentSummary table={table} resource={resource} />
  if (isEventSystem(resource.systemId))
    return <EventSummary table={table} resource={resource} />
  if (resource.systemId === 'sabac')
    return <SabacSummary table={table} resource={resource} />
  if (resource.systemId === 'cross_server')
    return <CrossServerSummary table={table} resource={resource} />
  return null
}

function QuestSummary({ table, resource }: { table: ProjectionTable, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const nextIndex = findColumn(table.columns, ['nextquestid', 'next', 'nextid', '后续任务', '下一任务'])
  const terminalCount = table.rows.filter(row => nextIndex < 0 || !cell(row, nextIndex)).length
  const metrics: MetricValue[] = [
    { label: t('studio.devtools.special.quest.nodes'), value: table.totalRows },
    { label: t('studio.devtools.special.quest.terminals'), value: terminalCount },
    { label: t('studio.devtools.special.references'), value: resource.dependencies.length },
    { label: t('studio.devtools.special.unresolved'), value: unresolvedCount(resource) },
  ]
  return <SpecializedPanel title={t('studio.devtools.special.quest.title')} description={t('studio.devtools.special.quest.description')} metrics={metrics} />
}

function TalentSummary({ table, resource }: { table: ProjectionTable, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const parentIndex = findColumn(table.columns, ['parentnodeid', 'parentid', 'parent', '父节点', '前置节点'])
  const costIndex = findColumn(table.columns, ['costpoints', 'cost', 'points', '消耗点数', '天赋点'])
  const levelIndex = findColumn(table.columns, ['requiredlevel', 'level', '需求等级', '等级'])
  const roots = table.rows.filter(row => parentIndex < 0 || !cell(row, parentIndex)).length
  const totalCost = sumColumn(table.rows, costIndex)
  const maximumLevel = maximumColumn(table.rows, levelIndex)
  const metrics: MetricValue[] = [
    { label: t('studio.devtools.special.talent.nodes'), value: table.totalRows },
    { label: t('studio.devtools.special.talent.roots'), value: roots },
    { label: t('studio.devtools.special.talent.cost'), value: totalCost },
    { label: t('studio.devtools.special.talent.level'), value: maximumLevel },
  ]
  return <SpecializedPanel title={t('studio.devtools.special.talent.title')} description={t('studio.devtools.special.talent.description')} metrics={metrics} warningCount={unresolvedCount(resource)} />
}

function EventSummary({ table, resource }: { table: ProjectionTable, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const startIndex = findColumn(table.columns, ['startepochseconds', 'starttime', 'start', 'openserverday', '开始时间', '开服天数'])
  const endIndex = findColumn(table.columns, ['endepochseconds', 'endtime', 'end', '结束时间'])
  const invalidWindows = table.rows.filter((row) => {
    const start = numericCell(row, startIndex)
    const end = numericCell(row, endIndex)
    return start != null && end != null && start >= end
  }).length
  const metrics: MetricValue[] = [
    { label: t('studio.devtools.special.event.windows'), value: table.totalRows },
    { label: t('studio.devtools.special.event.invalid'), value: invalidWindows },
    { label: t('studio.devtools.special.references'), value: resource.dependencies.length },
    { label: t('studio.devtools.special.unresolved'), value: unresolvedCount(resource) },
  ]
  return <SpecializedPanel title={t('studio.devtools.special.event.title')} description={t('studio.devtools.special.event.description')} metrics={metrics} warningCount={invalidWindows + unresolvedCount(resource)} />
}

function SabacSummary({ table, resource }: { table: ProjectionTable, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const mapIndex = findColumn(table.columns, ['battlemapid', 'mapid', 'map', '地图id', '战场地图'])
  const startIndex = findColumn(table.columns, ['startminute', 'start', '开始分钟'])
  const endIndex = findColumn(table.columns, ['endminute', 'end', '结束分钟'])
  const maps = new Set(table.rows.map(row => cell(row, mapIndex)).filter(Boolean)).size
  const invalidPhases = table.rows.filter((row) => {
    const start = numericCell(row, startIndex)
    const end = numericCell(row, endIndex)
    return start != null && end != null && start >= end
  }).length
  const metrics: MetricValue[] = [
    { label: t('studio.devtools.special.sabac.phases'), value: table.totalRows },
    { label: t('studio.devtools.special.sabac.maps'), value: maps },
    { label: t('studio.devtools.special.sabac.invalid'), value: invalidPhases },
    { label: t('studio.devtools.special.unresolved'), value: unresolvedCount(resource) },
  ]
  return <SpecializedPanel title={t('studio.devtools.special.sabac.title')} description={t('studio.devtools.special.sabac.description')} metrics={metrics} warningCount={invalidPhases + unresolvedCount(resource)} />
}

function CrossServerSummary({ table, resource }: { table: ProjectionTable, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const sourceIndex = findColumn(table.columns, ['sourceshard', 'source', 'from', '源服务器', '起点'])
  const targetIndex = findColumn(table.columns, ['targetshard', 'target', 'to', '目标服务器', '终点'])
  const compatibilityIndex = findColumn(table.columns, ['enginerange', 'versionrange', 'compatibility', 'maximumconcurrentplayers', '兼容版本', '最大人数'])
  const idIndex = findColumn(table.columns, ['routeid', 'id', 'key', '路由id'])
  const routes = table.rows.slice(0, 12).map((row, index) => ({
    id: cell(row, idIndex) || String(index + 1),
    source: cell(row, sourceIndex) || t('studio.devtools.special.unknown'),
    target: cell(row, targetIndex) || t('studio.devtools.special.unknown'),
    compatibility: cell(row, compatibilityIndex) || t('studio.devtools.special.unknown'),
  }))
  const metrics: MetricValue[] = [
    { label: t('studio.devtools.special.cross_server.routes'), value: table.totalRows },
    { label: t('studio.devtools.special.cross_server.shards'), value: shardCount(routes) },
    { label: t('studio.devtools.special.references'), value: resource.dependencies.length },
    { label: t('studio.devtools.special.unresolved'), value: unresolvedCount(resource) },
  ]
  return (
    <section className="space-y-3 rounded-xl border border-line bg-panel2 p-4" data-domain-composite="cross-server">
      <SpecializedHeader title={t('studio.devtools.special.cross_server.title')} description={t('studio.devtools.special.cross_server.description')} warningCount={unresolvedCount(resource)} />
      <MetricGrid metrics={metrics} />
      <div className="overflow-auto rounded-lg border border-line bg-panel">
        <div className="grid min-w-[560px] grid-cols-[100px_1fr_40px_1fr_1fr] bg-panel2 px-3 py-2 text-[9px] font-medium text-muted">
          <span>{t('studio.devtools.special.cross_server.route')}</span>
          <span>{t('studio.devtools.special.cross_server.source')}</span>
          <span />
          <span>{t('studio.devtools.special.cross_server.target')}</span>
          <span>{t('studio.devtools.special.cross_server.compatibility')}</span>
        </div>
        {routes.map(route => <RouteRow key={`${route.id}-${route.source}-${route.target}`} route={route} />)}
      </div>
    </section>
  )
}

function SpecializedPanel({ title, description, metrics, warningCount = 0 }: {
  title: string
  description: string
  metrics: MetricValue[]
  warningCount?: number
}) {
  return (
    <section className="space-y-3 rounded-xl border border-line bg-panel2 p-4" data-domain-composite="specialized">
      <SpecializedHeader title={title} description={description} warningCount={warningCount} />
      <MetricGrid metrics={metrics} />
    </section>
  )
}

function SpecializedHeader({ title, description, warningCount }: { title: string, description: string, warningCount: number }) {
  const { t } = useTranslation()
  return (
    <header className="flex items-start justify-between gap-4">
      <span>
        <strong className="block text-xs text-ink">{title}</strong>
        <small className="mt-1 block text-[9px] leading-4 text-muted">{description}</small>
      </span>
      <If cond={warningCount > 0}>
        <span className="shrink-0 rounded-full border border-danger/30 bg-danger/10 px-2 py-1 text-[9px] text-danger">{t('studio.devtools.special.warnings', { count: warningCount })}</span>
      </If>
    </header>
  )
}

function MetricGrid({ metrics }: { metrics: MetricValue[] }) {
  return (
    <dl className="grid grid-cols-4 gap-2 max-[900px]:grid-cols-2">
      {metrics.map(metric => (
        <div key={metric.label} className="rounded-lg border border-line bg-panel px-3 py-2">
          <dt className="text-[8px] uppercase tracking-wider text-muted">{metric.label}</dt>
          <dd className="mt-1 text-sm font-semibold tabular-nums text-ink">{metric.value}</dd>
        </div>
      ))}
    </dl>
  )
}

function RouteRow({ route }: { route: RouteValue }) {
  return (
    <div className="grid min-w-[560px] grid-cols-[100px_1fr_40px_1fr_1fr] items-center border-t border-line px-3 py-2 text-[10px]">
      <strong className="truncate text-ink">{route.id}</strong>
      <span className="truncate text-muted">{route.source}</span>
      <span className="text-center text-accent">→</span>
      <span className="truncate text-muted">{route.target}</span>
      <span className="truncate text-muted">{route.compatibility}</span>
    </div>
  )
}

function isEventSystem(systemId: string) {
  return systemId === 'limited_event' || systemId === 'launch_event' || systemId === 'season'
}

function unresolvedCount(resource: DomainResourceRecord) {
  return resource.dependencies.filter(dependency => dependency.required && dependency.resolvedResourceId == null).length
}

function findColumn(columns: string[], candidates: string[]) {
  const normalizedCandidates = candidates.map(normalizeColumn)
  return columns.findIndex((column) => {
    const normalized = normalizeColumn(column)
    return normalizedCandidates.some(candidate => normalized === candidate || normalized.includes(candidate))
  })
}

function normalizeColumn(value: string) {
  return value.toLocaleLowerCase().replaceAll(/[^\p{L}\p{N}]/gu, '')
}

function cell(row: string[], index: number) {
  if (index < 0)
    return ''
  return row[index]?.trim() ?? ''
}

function numericCell(row: string[], index: number) {
  const value = Number(cell(row, index))
  if (!Number.isFinite(value) || cell(row, index).length === 0)
    return null
  return value
}

function sumColumn(rows: string[][], index: number) {
  if (index < 0)
    return 0
  return rows.reduce((total, row) => total + (numericCell(row, index) ?? 0), 0)
}

function maximumColumn(rows: string[][], index: number) {
  if (index < 0)
    return 0
  return rows.reduce((maximum, row) => Math.max(maximum, numericCell(row, index) ?? 0), 0)
}

function shardCount(routes: RouteValue[]) {
  return new Set(routes.flatMap(route => [route.source, route.target]).filter(value => value.length > 0)).size
}
