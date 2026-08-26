import type { DomainMapCell, DomainMapProjection, DomainResourceRecord, DomainXlsProjection } from '../types'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { projectionTable } from '../projection-model'

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
    return <XlsPreview projection={resource.projection} />
  if (isRelationshipRenderer(renderer))
    return <RelationshipPreview renderer={renderer} resource={resource} />
  return <StructuredPreview resource={resource} />
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
  return (
    <div className="mt-5 space-y-3">
      <div className={relationshipGridClass(renderer)}>
        {table.rows.slice(0, 24).map((row, rowIndex) => (
          <article key={row.join('\u241F')} className="min-w-0 rounded-xl border border-line bg-panel2 p-3">
            <strong className="block truncate text-[11px] text-ink">{relationshipTitle(table.columns, row, rowIndex, t('studio.devtools.renderer.row', { index: rowIndex + 1 }))}</strong>
            <dl className="mt-2 space-y-1">
              {row.slice(0, 6).map((value, columnIndex) => (
                <div key={`${columnLabel(table.columns, columnIndex, t)}-${value}`} className="grid grid-cols-[minmax(64px,0.4fr)_1fr] gap-2 text-[9px]">
                  <dt className="truncate text-muted">{columnLabel(table.columns, columnIndex, t)}</dt>
                  <dd className="break-words text-ink">{value}</dd>
                </div>
              ))}
            </dl>
          </article>
        ))}
      </div>
      <p className="text-[10px] text-muted">{t('studio.devtools.renderer.real_rows', { shown: Math.min(table.rows.length, 24), total: table.totalRows })}</p>
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

function relationshipTitle(columns: string[], row: string[], rowIndex: number, fallback: string) {
  const preferredIndex = columns.findIndex(column => /^(?:name|title|id|key|event|node)$/i.test(column))
  if (preferredIndex >= 0 && row[preferredIndex])
    return row[preferredIndex]
  return row.find(value => value.length > 0) ?? fallback ?? String(rowIndex + 1)
}

function columnLabel(columns: string[], index: number, t: ReturnType<typeof useTranslation>['t']) {
  if (columns[index])
    return columns[index]
  return t('studio.devtools.renderer.column', { index: index + 1 })
}

function isRelationshipRenderer(renderer: string) {
  return ['flow', 'graph', 'tree', 'timeline', 'topology', 'spatial'].some(kind => renderer.includes(kind))
}

function relationshipGridClass(renderer: string) {
  if (renderer.includes('timeline'))
    return 'grid grid-cols-3 gap-2 max-[900px]:grid-cols-1'
  if (renderer.includes('tree') || renderer.includes('topology'))
    return 'grid grid-cols-2 gap-3 max-[900px]:grid-cols-1'
  return 'grid grid-cols-3 gap-3 max-[1000px]:grid-cols-2 max-[760px]:grid-cols-1'
}

function sheetButtonClass(active: boolean) {
  return `rounded-lg border px-3 py-1.5 text-[10px] ${active ? 'border-accent bg-accent/10 text-accent' : 'border-line bg-panel2 text-muted'}`
}
