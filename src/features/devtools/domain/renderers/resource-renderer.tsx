import type { ReactNode } from 'react'
import type { DomainMapCell, DomainMapProjection, DomainResourceRecord, DomainXlsProjection } from '../types'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { projectionTable } from '../projection-model'
import { SpecializedDomainSummary } from './specialized-domain-summary'

export function ResourceRenderer({ renderer, resource, loading, error }: {
  renderer: string
  resource?: DomainResourceRecord
  loading: boolean
  error: Error | null
}) {
  const { t } = useTranslation()
  if (loading)
    return <RendererNotice title={t('studio.devtools.renderer.loading')} />
  if (error)
    return <RendererNotice title={t('studio.devtools.renderer.failed')} detail={String(error)} />
  if (!resource)
    return <RendererNotice title={t('studio.devtools.renderer.select_resource')} detail={t('studio.devtools.renderer.select_resource_desc')} />
  if (!resource.projection)
    return <RendererNotice title={t('studio.devtools.renderer.unavailable')} detail={resource.diagnostics.join('\n')} />
  if (resource.projection.kind === 'map')
    return <MapCanvas projection={resource.projection} diagnostics={resource.diagnostics} />
  if (resource.projection.kind === 'xls')
    return <CompositePreview resource={resource}><SemanticPreview renderer={renderer} resource={resource} fallback={<XlsPreview projection={resource.projection} />} /></CompositePreview>
  if (isSemanticRenderer(renderer))
    return <CompositePreview resource={resource}><SemanticPreview renderer={renderer} resource={resource} fallback={<StructuredPreview resource={resource} />} /></CompositePreview>
  return <StructuredPreview resource={resource} />
}

function CompositePreview({ resource, children }: { resource: DomainResourceRecord, children: ReactNode }) {
  return (
    <div className="mt-5 space-y-3">
      <SpecializedDomainSummary resource={resource} />
      {children}
    </div>
  )
}

function SemanticPreview({ renderer, resource, fallback }: { renderer: string, resource: DomainResourceRecord, fallback: ReactNode }) {
  const table = projectionTable(resource.projection!)
  if (!table)
    return fallback
  if (renderer.includes('chart'))
    return <ChartPreview table={table} resource={resource} />
  if (renderer.includes('calendar'))
    return <CalendarPreview table={table} resource={resource} />
  if (renderer.includes('ranking'))
    return <RankingPreview table={table} resource={resource} />
  if (renderer.includes('timeline'))
    return <TimelinePreview table={table} resource={resource} />
  if (isRelationshipRenderer(renderer))
    return <RelationshipPreview renderer={renderer} resource={resource} />
  return fallback
}

function XlsPreview({ projection }: { projection: DomainXlsProjection }) {
  const { t } = useTranslation()
  const [sheetIndex, setSheetIndex] = useState(0)
  const table = projectionTable(projection, sheetIndex)
  const sheet = projection.sheets[sheetIndex]
  return (
    <div className="mt-5 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        {projection.sheets.map((item, index) => (
          <button key={item.name} type="button" className={sheetButtonClass(index === sheetIndex)} onClick={() => setSheetIndex(index)}>{item.name}</button>
        ))}
        <If cond={projection.truncated}><span className="text-[10px] text-danger">{t('studio.devtools.renderer.truncated')}</span></If>
      </div>
      <If cond={table != null && sheet != null} else={<RendererNotice title={t('studio.devtools.renderer.empty')} />}>
        <ProjectionTable table={table!} />
        <p className="text-[10px] text-muted">{t('studio.devtools.renderer.sheet_dimensions', { rows: sheet?.rowCount, columns: sheet?.columnCount })}</p>
      </If>
    </div>
  )
}

function StructuredPreview({ resource }: { resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const projection = resource.projection
  if (!projection || projection.kind === 'map')
    return null
  const table = projectionTable(projection)
  return (
    <div className="mt-5 space-y-3">
      <If cond={table != null} else={<RendererNotice title={t('studio.devtools.renderer.empty')} />}>
        <ProjectionTable table={table!} />
      </If>
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function RelationshipPreview({ renderer, resource }: { renderer: string, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const projection = resource.projection
  if (!projection || projection.kind === 'map')
    return null
  const table = projectionTable(projection)
  if (!table)
    return <RendererNotice title={t('studio.devtools.renderer.empty')} />
  const nodes = semanticNodes(table)
  const edges = semanticEdges(table, nodes)
  return (
    <div className="mt-5 space-y-3">
      <SemanticGraph renderer={renderer} nodes={nodes} edges={edges} />
      <p className="text-[10px] text-muted">{t('studio.devtools.renderer.real_rows', { shown: Math.min(table.rows.length, 24), total: table.totalRows })}</p>
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function ChartPreview({ table, resource }: { table: NonNullable<ReturnType<typeof projectionTable>>, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  const series = numericSeries(table)
  const points = chartPoints(series)
  return (
    <div className="mt-5 space-y-3">
      <div className="rounded-xl border border-line bg-panel2 p-4">
        <div className="mb-3 flex items-center justify-between gap-3 text-[10px] text-muted">
          <span>{t('studio.devtools.renderer.chart_series')}</span>
          <span>{series.length}</span>
        </div>
        <svg viewBox="0 0 640 240" className="h-60 w-full" role="img" aria-label={t('studio.devtools.renderer.chart_label')}>
          <path d="M32 16V216H624" fill="none" stroke="currentColor" className="text-line" />
          <polyline points={points} fill="none" stroke="currentColor" strokeWidth="3" className="text-accent" />
          {series.map((item, index) => <circle key={item.key} cx={chartX(index, series.length)} cy={chartY(item.value, series)} r="4" fill="currentColor" className="text-accent" />)}
        </svg>
      </div>
      <ProjectionTable table={table} />
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function CalendarPreview({ table, resource }: { table: NonNullable<ReturnType<typeof projectionTable>>, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  return (
    <div className="mt-5 space-y-3">
      <div className="grid grid-cols-7 overflow-hidden rounded-xl border border-line bg-panel2">
        {[1, 2, 3, 4, 5, 6, 7].map(day => <strong key={`calendar-day-${day}`} className="border-b border-r border-line px-2 py-2 text-center text-[9px] text-muted last:border-r-0">{t('studio.devtools.renderer.calendar_day', { day })}</strong>)}
        {table.rows.slice(0, 35).map((row, index) => (
          <article key={row.join('\u241F')} className="min-h-20 border-r border-t border-line p-2 [border-top-width:0] nth-[7n]:border-r-0">
            <strong className="text-[10px] text-accent">{index + 1}</strong>
            <span className="mt-1 block line-clamp-3 text-[9px] text-ink">{relationshipTitle(table.columns, row, index, String(index + 1))}</span>
          </article>
        ))}
      </div>
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function RankingPreview({ table, resource }: { table: NonNullable<ReturnType<typeof projectionTable>>, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  return (
    <div className="mt-5 space-y-3">
      <div className="overflow-hidden rounded-xl border border-line bg-panel2">
        {table.rows.slice(0, 50).map((row, index) => (
          <div key={row.join('\u241F')} className="grid grid-cols-[52px_minmax(140px,1fr)_2fr] items-center gap-3 border-b border-line px-4 py-3 last:border-b-0">
            <strong className={rankingClass(index)}>
              #
              {index + 1}
            </strong>
            <span className="truncate text-[11px] text-ink">{relationshipTitle(table.columns, row, index, t('studio.devtools.renderer.row', { index: index + 1 }))}</span>
            <span className="truncate text-right text-[10px] tabular-nums text-muted">{row.slice(1).join(' · ')}</span>
          </div>
        ))}
      </div>
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function TimelinePreview({ table, resource }: { table: NonNullable<ReturnType<typeof projectionTable>>, resource: DomainResourceRecord }) {
  const { t } = useTranslation()
  return (
    <div className="mt-5 space-y-3">
      <ol className="relative ml-3 border-l border-line pl-6">
        {table.rows.slice(0, 40).map((row, index) => (
          <li key={row.join('\u241F')} className="relative pb-5 last:pb-0">
            <i className="absolute -left-[29px] top-1 size-2 rounded-full bg-accent ring-4 ring-panel2" />
            <span className="block text-[9px] tabular-nums text-muted">{timelineLabel(table.columns, row, index, t)}</span>
            <strong className="mt-1 block text-[11px] text-ink">{relationshipTitle(table.columns, row, index, t('studio.devtools.renderer.row', { index: index + 1 }))}</strong>
            <span className="mt-1 block text-[9px] text-muted">{row.slice(1, 6).join(' · ')}</span>
          </li>
        ))}
      </ol>
      <ProjectionDiagnostics resource={resource} />
    </div>
  )
}

function ProjectionTable({ table }: { table: NonNullable<ReturnType<typeof projectionTable>> }) {
  const { t } = useTranslation()
  const visibleColumns = Math.max(table.columns.length, ...table.rows.map(row => row.length), 1)
  const gridTemplateColumns = `repeat(${visibleColumns}, minmax(120px, 1fr))`
  return (
    <div className="overflow-auto rounded-xl border border-line">
      <div className="grid min-w-max bg-panel2" style={{ gridTemplateColumns }}>
        {Array.from({ length: visibleColumns }, (_, index) => (
          <strong key={columnLabel(table.columns, index, t)} className="border-r border-line px-3 py-2 text-[9px] font-medium text-muted last:border-r-0">{columnLabel(table.columns, index, t)}</strong>
        ))}
      </div>
      {table.rows.map(row => (
        <div key={row.join('\u241F')} className="grid min-w-max border-t border-line" style={{ gridTemplateColumns }}>
          {Array.from({ length: visibleColumns }, (_, columnIndex) => (
            <span key={columnLabel(table.columns, columnIndex, t)} className="max-w-80 truncate border-r border-line px-3 py-2 text-[10px] text-ink last:border-r-0" title={row[columnIndex] ?? ''}>{row[columnIndex] ?? ''}</span>
          ))}
        </div>
      ))}
      <div className="border-t border-line bg-panel2 px-3 py-2 text-[9px] text-muted">{t('studio.devtools.renderer.table_dimensions', { rows: table.totalRows, columns: table.totalColumns })}</div>
    </div>
  )
}

function MapCanvas({ projection, diagnostics }: { projection: DomainMapProjection, diagnostics: string[] }) {
  const { t } = useTranslation()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const chunk = projection.initialChunk
  const walkable = chunk.cells.filter(cell => cell.walkable).length
  const blocked = chunk.cells.length - walkable

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas)
      return
    drawMapCanvas(canvas, projection)
  }, [projection])

  return (
    <div className="mt-5 space-y-3">
      <div className="grid grid-cols-4 gap-2 max-[900px]:grid-cols-2">
        <MapMetric label={t('studio.devtools.renderer.map_dimensions')} value={`${projection.header.width} × ${projection.header.height}`} />
        <MapMetric label={t('studio.devtools.renderer.map_chunk')} value={`${chunk.width} × ${chunk.height}`} />
        <MapMetric label={t('studio.devtools.renderer.map_origin')} value={`(${chunk.startX}, ${chunk.startY})`} />
        <MapMetric label={t('studio.devtools.renderer.map_walkable')} value={`${walkable} / ${chunk.cells.length}`} />
      </div>
      <div className="overflow-auto rounded-xl border border-line bg-panel2 p-3">
        <canvas ref={canvasRef} width={Math.max(chunk.width * 8, 1)} height={Math.max(chunk.height * 8, 1)} className="mx-auto max-h-[520px] max-w-full bg-panel [image-rendering:pixelated]" aria-label={t('studio.devtools.renderer.map_canvas_label')} />
      </div>
      <div className="flex flex-wrap gap-3 text-[9px] text-muted">
        <Legend swatch="bg-accent" label={t('studio.devtools.renderer.map_walkable_count', { count: walkable })} />
        <Legend swatch="bg-danger" label={t('studio.devtools.renderer.map_blocked_count', { count: blocked })} />
        <Legend swatch="border border-ink bg-transparent" label={t('studio.devtools.renderer.map_layer_background')} />
        <Legend swatch="bg-ink" label={t('studio.devtools.renderer.map_layer_middle')} />
        <Legend swatch="bg-warning" label={t('studio.devtools.renderer.map_layer_front')} />
      </div>
      <MapCellGrid cells={chunk.cells.slice(0, 16)} />
      <DiagnosticList diagnostics={[...projection.header.diagnostics.map(item => `${item.code}: ${item.message}`), ...diagnostics]} />
    </div>
  )
}

function MapCellGrid({ cells }: { cells: DomainMapCell[] }) {
  const { t } = useTranslation()
  return (
    <div className="grid grid-cols-4 gap-2 max-[900px]:grid-cols-2">
      {cells.map(cell => (
        <div key={`${cell.x}-${cell.y}`} className="rounded-lg border border-line bg-panel2 px-3 py-2 text-[9px]">
          <strong className="text-ink">
            (
            {cell.x}
            ,
            {' '}
            {cell.y}
            )
          </strong>
          <span className="mt-1 block text-muted">{t('studio.devtools.renderer.map_cell_layers', { background: spriteLabel(cell.background), middle: spriteLabel(cell.middle), front: spriteLabel(cell.front) })}</span>
          <span className="mt-1 block text-muted">{t(cell.walkable ? 'studio.devtools.renderer.map_cell_walkable' : 'studio.devtools.renderer.map_cell_blocked')}</span>
        </div>
      ))}
    </div>
  )
}

function ProjectionDiagnostics({ resource }: { resource: DomainResourceRecord }) {
  return <DiagnosticList diagnostics={resource.diagnostics} />
}

function DiagnosticList({ diagnostics }: { diagnostics: string[] }) {
  const { t } = useTranslation()
  return (
    <If cond={diagnostics.length > 0}>
      <div className="rounded-lg border border-danger/20 bg-danger/5 p-3">
        <strong className="text-[10px] text-danger">{t('studio.devtools.renderer.diagnostics')}</strong>
        {diagnostics.map(diagnostic => <p key={diagnostic} className="mt-1 break-words text-[9px] text-muted">{diagnostic}</p>)}
      </div>
    </If>
  )
}

function RendererNotice({ title, detail }: { title: string, detail?: string }) {
  return (
    <div className="mt-5 rounded-xl border border-line bg-panel2 p-6 text-center">
      <strong className="block text-xs text-ink">{title}</strong>
      <If cond={detail != null && detail.length > 0}><p className="mt-2 whitespace-pre-wrap text-[10px] leading-5 text-muted">{detail}</p></If>
    </div>
  )
}

function MapMetric({ label, value }: { label: string, value: string }) {
  return (
    <div className="rounded-lg border border-line bg-panel2 px-3 py-2">
      <span className="block text-[9px] text-muted">{label}</span>
      <strong className="mt-1 block text-xs tabular-nums text-ink">{value}</strong>
    </div>
  )
}

function Legend({ swatch, label }: { swatch: string, label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <i className={`size-2.5 rounded-sm ${swatch}`} />
      {label}
    </span>
  )
}

function drawMapCanvas(canvas: HTMLCanvasElement, projection: DomainMapProjection) {
  const context = canvas.getContext('2d')
  if (!context)
    return
  const styles = getComputedStyle(document.documentElement)
  const accent = styles.getPropertyValue('--color-accent').trim()
  const danger = styles.getPropertyValue('--color-danger').trim()
  const ink = styles.getPropertyValue('--color-ink').trim()
  const warning = styles.getPropertyValue('--color-warning').trim() || accent
  const line = styles.getPropertyValue('--color-line-strong').trim()
  const cellSize = 8
  context.clearRect(0, 0, canvas.width, canvas.height)
  projection.initialChunk.cells.forEach((cell) => {
    const x = (cell.x - projection.initialChunk.startX) * cellSize
    const y = (cell.y - projection.initialChunk.startY) * cellSize
    context.fillStyle = cell.walkable ? accent : danger
    context.globalAlpha = spriteVisible(cell.background) ? 0.72 : 0.36
    context.fillRect(x, y, cellSize, cellSize)
    context.globalAlpha = 1
    context.strokeStyle = line
    context.strokeRect(x + 0.5, y + 0.5, cellSize - 1, cellSize - 1)
    if (spriteVisible(cell.middle)) {
      context.fillStyle = ink
      context.fillRect(x + 3, y + 3, 2, 2)
    }
    if (spriteVisible(cell.front)) {
      context.fillStyle = warning
      context.fillRect(x + 1, y + 1, 2, 2)
    }
  })
}

function spriteVisible(sprite: { library: number, image: number }) {
  return sprite.library >= 0 && sprite.image > 0
}

function spriteLabel(sprite: { library: number, image: number }) {
  return `${sprite.library}:${sprite.image}`
}

interface SemanticNode {
  id: string
  title: string
  detail: string
  row: string[]
}

interface SemanticEdge {
  from: number
  to: number
}

function SemanticGraph({ renderer, nodes, edges }: { renderer: string, nodes: SemanticNode[], edges: SemanticEdge[] }) {
  const visible = nodes.slice(0, 18)
  const height = graphHeight(visible.length, renderer)
  return (
    <div className="overflow-auto rounded-xl border border-line bg-panel2 p-3">
      <svg viewBox={`0 0 720 ${height}`} className="min-h-56 min-w-[640px] w-full" role="img">
        {edges.map(edge => <path key={`${edge.from}-${edge.to}`} d={edgePath(edge, visible.length, renderer)} fill="none" stroke="currentColor" strokeWidth="1.5" className="text-line" />)}
        {visible.map((node, index) => <GraphNode key={node.id} node={node} position={nodePosition(index, visible.length, renderer)} />)}
      </svg>
    </div>
  )
}

function GraphNode({ node, position }: { node: SemanticNode, position: { x: number, y: number } }) {
  return (
    <g transform={`translate(${position.x} ${position.y})`}>
      <rect width="152" height="48" x="-76" y="-24" rx="10" fill="currentColor" className="text-panel" stroke="currentColor" />
      <rect width="152" height="48" x="-76" y="-24" rx="10" fill="none" stroke="currentColor" className="text-line" />
      <text textAnchor="middle" y="-2" fill="currentColor" className="text-[10px] font-medium text-ink">{truncateLabel(node.title, 20)}</text>
      <text textAnchor="middle" y="14" fill="currentColor" className="text-[8px] text-muted">{truncateLabel(node.detail, 24)}</text>
    </g>
  )
}

function semanticNodes(table: NonNullable<ReturnType<typeof projectionTable>>): SemanticNode[] {
  return table.rows.slice(0, 24).map((row, index) => {
    const title = relationshipTitle(table.columns, row, index, String(index + 1))
    return {
      id: rowIdentity(table.columns, row, index),
      title,
      detail: row.filter(value => value && value !== title && value !== rowIdentity(table.columns, row, index)).slice(0, 2).join(' · '),
      row,
    }
  })
}

function semanticEdges(table: NonNullable<ReturnType<typeof projectionTable>>, nodes: SemanticNode[]): SemanticEdge[] {
  const identities = new Map(nodes.map((node, index) => [node.id.toLowerCase(), index]))
  const referenceIndexes = table.columns
    .map((column, index) => ({ column, index }))
    .filter(item => containsAny(item.column, ['parent', 'next', 'target', 'dependency', 'from', 'to', '前置', '后续', '目标', '父']))
    .map(item => item.index)
  const edges: SemanticEdge[] = []
  nodes.forEach((node, index) => {
    referenceIndexes.forEach((columnIndex) => {
      const target = identities.get((node.row[columnIndex] ?? '').trim().toLowerCase())
      if (target != null && target !== index)
        edges.push({ from: index, to: target })
    })
    if (referenceIndexes.length === 0 && index > 0)
      edges.push({ from: index - 1, to: index })
  })
  return edges.slice(0, 48)
}

function rowIdentity(columns: string[], row: string[], index: number) {
  const identityIndex = columns.findIndex(column => ['id', 'key', 'code', 'nodeid', '节点id'].includes(column.toLowerCase()))
  return row[identityIndex] || `row-${index + 1}`
}

function nodePosition(index: number, total: number, renderer: string) {
  if (renderer.includes('topology') || renderer.includes('spatial')) {
    const angle = total > 0 ? (Math.PI * 2 * index) / total : 0
    return { x: 360 + Math.cos(angle) * 250, y: 180 + Math.sin(angle) * 120 }
  }
  if (renderer.includes('graph')) {
    const columns = 4
    return { x: 100 + (index % columns) * 174, y: 54 + Math.floor(index / columns) * 88 }
  }
  return { x: 110 + (index % 4) * 166, y: 50 + Math.floor(index / 4) * 78 }
}

function graphHeight(total: number, renderer: string) {
  if (renderer.includes('topology') || renderer.includes('spatial'))
    return 360
  return Math.max(220, Math.ceil(total / 4) * 88 + 28)
}

function edgePath(edge: SemanticEdge, total: number, renderer: string) {
  const from = nodePosition(edge.from, total, renderer)
  const to = nodePosition(edge.to, total, renderer)
  const middleX = (from.x + to.x) / 2
  return `M${from.x} ${from.y} C${middleX} ${from.y},${middleX} ${to.y},${to.x} ${to.y}`
}

function numericSeries(table: NonNullable<ReturnType<typeof projectionTable>>) {
  const numericIndex = table.columns.findIndex((_, columnIndex) => table.rows.some(row => Number.isFinite(Number(row[columnIndex]))))
  const valueIndex = numericIndex >= 0 ? numericIndex : 0
  return table.rows.slice(0, 32).map((row, index) => ({
    key: `${index}-${row.join('\u241F')}`,
    value: Number(row[valueIndex]) || 0,
  }))
}

function chartX(index: number, total: number) {
  return 40 + (total <= 1 ? 0 : (index / (total - 1)) * 574)
}

function chartY(value: number, series: Array<{ value: number }>) {
  const values = series.map(item => item.value)
  const minimum = Math.min(...values, 0)
  const maximum = Math.max(...values, 1)
  return 208 - ((value - minimum) / Math.max(maximum - minimum, 1)) * 176
}

function chartPoints(series: Array<{ value: number }>) {
  return series.map((item, index) => `${chartX(index, series.length)},${chartY(item.value, series)}`).join(' ')
}

function rankingClass(index: number) {
  if (index === 0)
    return 'text-sm tabular-nums text-warning'
  if (index < 3)
    return 'text-xs tabular-nums text-accent'
  return 'text-[10px] tabular-nums text-muted'
}

function timelineLabel(columns: string[], row: string[], index: number, t: ReturnType<typeof useTranslation>['t']) {
  const timeIndex = columns.findIndex(column => containsAny(column, ['time', 'date', 'start', 'end', 'day', '时间', '日期', '开始', '结束']))
  if (timeIndex >= 0 && row[timeIndex])
    return row[timeIndex]
  return t('studio.devtools.renderer.timeline_step', { step: index + 1 })
}

function truncateLabel(value: string, length: number) {
  if (value.length <= length)
    return value
  return `${value.slice(0, length - 1)}…`
}

function containsAny(value: string, needles: string[]) {
  const normalized = value.toLowerCase()
  return needles.some(needle => normalized.includes(needle))
}

function relationshipTitle(columns: string[], row: string[], rowIndex: number, fallback: string) {
  const normalized = columns.map(column => column.toLowerCase())
  const preferredIndex = ['name', 'title', 'event', 'node', 'key', 'id']
    .map(column => normalized.indexOf(column))
    .find(index => index >= 0) ?? -1
  if (preferredIndex >= 0 && row[preferredIndex])
    return row[preferredIndex]
  return row.find(value => value.length > 0) ?? fallback ?? String(rowIndex + 1)
}

function columnLabel(columns: string[], index: number, t: ReturnType<typeof useTranslation>['t']) {
  if (columns[index])
    return columns[index]
  return t('studio.devtools.renderer.column', { index: index + 1 })
}

function isSemanticRenderer(renderer: string) {
  return ['chart', 'calendar', 'ranking', 'flow', 'graph', 'tree', 'timeline', 'topology', 'spatial'].some(kind => renderer.includes(kind))
}

function isRelationshipRenderer(renderer: string) {
  return ['flow', 'graph', 'tree', 'timeline', 'topology', 'spatial'].some(kind => renderer.includes(kind))
}

function sheetButtonClass(active: boolean) {
  return `rounded-lg border px-3 py-1.5 text-[10px] ${active ? 'border-accent bg-accent/10 text-accent' : 'border-line bg-panel2 text-muted'}`
}
