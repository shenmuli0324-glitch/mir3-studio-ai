import type { BoundValue, GuiPropertyValue, Mir3UiNode } from './types'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { guiAssetMetaQueryOptions } from './api'
import { useCanvasAssets } from './canvas-assets'
import { componentDefinition, nodeAssetValue } from './component-catalog'
import { GuiDesignerScope } from './gui-designer-scope'

export function DesignerInspector() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const node = scope.selectedNode
  return (
    <aside className="flex w-64 shrink-0 flex-col border-l border-line bg-panel">
      <header className="flex h-10 shrink-0 items-center border-b border-line px-3">
        <strong className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted">{t('studio.gui.inspector.title')}</strong>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <If cond={node == null}>
          <p className="px-5 py-12 text-center text-[11px] leading-5 text-muted">{t('studio.gui.inspector.empty')}</p>
        </If>
        <If cond={node != null}><InspectorContent node={node!} /></If>
      </div>
    </aside>
  )
}

function InspectorContent({ node }: { node: Mir3UiNode }) {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const genericProperties = inspectorProperties(node)
  const assetSlots = node.kind === 'Unsupported' ? [] : componentDefinition(node.kind).assetSlots
  const inspectorAssets = useCanvasAssets(scope.activeProject?.id, { [node.id]: node }, true, node.id)
  return (
    <div>
      <section className="border-b border-line p-3">
        <div className="flex items-start justify-between gap-2">
          <span className="min-w-0">
            <strong className="block truncate text-xs font-medium text-ink">{node.name?.value || node.luaVariable || node.kind}</strong>
            <small className="mt-1 block text-[10px] text-muted">
              {node.kind}
              {' '}
              ·
              {' '}
              {node.luaVariable}
            </small>
          </span>
          <span className={compatibilityClass(node.compatibility)}>{t(`studio.gui.compatibility.${node.compatibility}`)}</span>
        </div>
        <If cond={node.binding?.statement != null}>
          <p className="mt-3 text-[9px] tabular-nums text-muted">{t('studio.gui.inspector.source_line', { line: (node.binding?.statement?.startLine ?? 0) + 1 })}</p>
        </If>
      </section>
      <PropertySection title={t('studio.gui.inspector.position')}>
        <div className="grid grid-cols-2 gap-2">
          <PropertyInput label={t('studio.gui.inspector.x')} value={node.position.x.value} writable={scope.nodePropertyWritable(node, 'x') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'x', Number(value))} />
          <PropertyInput label={t('studio.gui.inspector.y')} value={node.position.y.value} writable={scope.nodePropertyWritable(node, 'y') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'y', Number(value))} />
        </div>
      </PropertySection>
      <PropertySection title={t('studio.gui.inspector.size')}>
        <div className="grid grid-cols-2 gap-2">
          <PropertyInput label={t('studio.gui.inspector.width')} value={node.size.width.value} writable={scope.nodePropertyWritable(node, 'width') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'width', Number(value))} />
          <PropertyInput label={t('studio.gui.inspector.height')} value={node.size.height.value} writable={scope.nodePropertyWritable(node, 'height') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'height', Number(value))} />
        </div>
      </PropertySection>
      <If cond={isTextNode(node)}>
        <PropertySection title={t('studio.gui.inspector.content')}>
          <PropertyInput label={t('studio.gui.inspector.text')} value={node.paint?.text?.value ?? ''} writable={scope.nodePropertyWritable(node, 'text') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'text', value)} />
        </PropertySection>
      </If>
      <If cond={assetSlots.length > 0}>
        <PropertySection title={t('studio.gui.inspector.asset')}>
          <div className="grid gap-2">
            {assetSlots.map(slot => (
              <AssetPropertyInput node={node} property={slot.property} hrefs={inspectorAssets.hrefs} key={slot.property} />
            ))}
          </div>
          <p className="mt-2 text-[9px] leading-4 text-muted">{t('studio.gui.inspector.asset_hint')}</p>
        </PropertySection>
      </If>
      <If cond={genericProperties.length > 0}>
        <PropertySection title={t('studio.gui.inspector.advanced')}>
          <div className="grid gap-2">
            {genericProperties.map(([property, bound]) => (
              <GenericPropertyInput
                key={property}
                property={property}
                bound={bound}
                writable={scope.nodeGenericPropertyWritable(node, property) && !scope.parsePending}
                onCommit={value => scope.updateNodeGenericProperty(node.id, property, value)}
              />
            ))}
          </div>
        </PropertySection>
      </If>
      <PropertySection title={t('studio.gui.inspector.behaviors')}>
        <p className="mb-3 text-[9px] leading-4 text-muted">{t('studio.gui.inspector.behaviors_hint')}</p>
        <div className="grid grid-cols-2 gap-2">
          <button className="h-8 rounded-lg bg-panel-2 text-[10px] text-ink ring-1 ring-line hover:ring-accent disabled:opacity-40" type="button" disabled={!scope.canAddNodeBehavior(node)} onClick={() => scope.addNodeBehavior(node.id, 'timeline')}>{t('studio.gui.inspector.add_timeline')}</button>
          <button className="h-8 rounded-lg bg-panel-2 text-[10px] text-ink ring-1 ring-line hover:ring-accent disabled:opacity-40" type="button" disabled={!scope.canAddNodeBehavior(node)} onClick={() => scope.addNodeBehavior(node.id, 'action')}>{t('studio.gui.inspector.add_action')}</button>
        </div>
      </PropertySection>
      <If cond={node.compatibility !== 'supported'}>
        <div className="m-3 rounded-lg bg-danger/8 px-3 py-2 text-[10px] leading-4 text-danger ring-1 ring-danger/20">
          {compatibilityReason(node, t)}
        </div>
      </If>
    </div>
  )
}

function compatibilityReason(node: Mir3UiNode, t: (key: string) => string): string {
  if (node.compatibilityReasonCode)
    return t(`studio.gui.compatibility_reason.${node.compatibilityReasonCode}`)
  return node.compatibilityReason || t(`studio.gui.inspector.compatibility_hint.${node.compatibility}`)
}

function GenericPropertyInput({ property, bound, writable, onCommit }: {
  property: string
  bound: BoundValue<GuiPropertyValue>
  writable: boolean
  onCommit: (value: GuiPropertyValue) => void
}) {
  const { t } = useTranslation()
  if (typeof bound.value === 'boolean') {
    return (
      <label className="block min-w-0">
        <span className="mb-1 flex items-center justify-between gap-2 text-[9px] text-muted">
          <span>{property}</span>
          <span>{t(`studio.gui.value_source.${bound.source}`)}</span>
        </span>
        <select className="h-8 w-full rounded-lg bg-panel-2 px-2 text-[11px] text-ink outline-none ring-1 ring-line focus:ring-accent disabled:opacity-45" value={String(bound.value)} disabled={!writable} onChange={event => onCommit(event.target.value === 'true')}>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
    )
  }
  return (
    <label className="block min-w-0">
      <span className="mb-1 flex items-center justify-between gap-2 text-[9px] text-muted">
        <span>{property}</span>
        <span>{t(`studio.gui.value_source.${bound.source}`)}</span>
      </span>
      <input
        className="h-8 w-full rounded-lg bg-panel-2 px-2 text-[11px] text-ink outline-none ring-1 ring-line focus:ring-accent disabled:opacity-45"
        key={`${property}:${String(bound.value)}`}
        defaultValue={propertyDisplayValue(bound.value)}
        disabled={!writable}
        onBlur={event => onCommit(parsePropertyInput(event.target.value, bound.value))}
        onKeyDown={(event) => {
          if (event.key === 'Enter')
            event.currentTarget.blur()
        }}
      />
    </label>
  )
}

function inspectorProperties(node: Mir3UiNode): Array<[string, BoundValue<GuiPropertyValue>]> {
  const assetProperties = node.kind === 'Unsupported' ? [] : componentDefinition(node.kind).assetSlots.map(slot => slot.property)
  const hidden = new Set(['parent', 'name', 'x', 'y', 'width', 'height', 'text', 'normalImage', ...assetProperties])
  return Object.entries(node.properties ?? {}).filter(([property]) => !hidden.has(property))
}

function AssetPropertyInput({ node, property, hrefs }: { node: Mir3UiNode, property: string, hrefs: Record<string, string> }) {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const bound = nodeAssetValue(node, property)
  const path = bound?.value ?? ''
  const projectId = scope.activeProject?.id ?? ''
  const meta = useQuery({
    ...guiAssetMetaQueryOptions(projectId, path),
    enabled: projectId.length > 0 && path.length > 0,
    retry: false,
  })
  const writable = property === 'image'
    ? scope.nodePropertyWritable(node, 'image')
    : scope.nodeGenericPropertyWritable(node, property)

  function commit(value: string) {
    if (property === 'image') {
      scope.updateNodeProperty(node.id, 'image', value)
      return
    }
    scope.updateNodeGenericProperty(node.id, property, value)
  }

  return (
    <label className="block min-w-0">
      <span className="mb-1 flex items-center justify-between gap-2 text-[9px] text-muted">
        <span>{t(`studio.gui.inspector.asset_slot.${property}`)}</span>
        <span>{bound ? t(`studio.gui.value_source.${bound.source}`) : t('studio.gui.value_source.default')}</span>
      </span>
      <If cond={path.length > 0}>
        <span className="mb-2 flex min-h-14 items-center gap-2 overflow-hidden rounded-lg bg-canvas p-2 ring-1 ring-line">
          <If
            cond={hrefs[path] != null}
            then={<img className="size-10 shrink-0 object-contain" src={hrefs[path]} alt="" />}
            else={<span className="grid size-10 shrink-0 place-items-center rounded bg-panel-2 text-[9px] text-muted">◇</span>}
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[9px] text-ink">{path}</span>
            <span className={meta.isError ? 'mt-1 block text-[8px] text-danger' : 'mt-1 block text-[8px] text-muted'}>
              {assetStatus(meta.status, meta.data?.width, meta.data?.height, t)}
            </span>
          </span>
        </span>
      </If>
      <input
        className="h-8 w-full rounded-lg bg-panel-2 px-2 text-[11px] text-ink outline-none ring-1 ring-line focus:ring-accent disabled:cursor-not-allowed disabled:opacity-60"
        aria-label={t(`studio.gui.inspector.asset_slot.${property}`)}
        key={`${property}:${bound?.value ?? ''}`}
        defaultValue={bound?.value ?? ''}
        disabled={!writable || scope.parsePending}
        onBlur={event => commit(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter')
            event.currentTarget.blur()
        }}
      />
    </label>
  )
}

function assetStatus(status: 'error' | 'pending' | 'success', width: number | undefined, height: number | undefined, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (status === 'success')
    return t('studio.gui.inspector.asset_available', { width: width ?? 0, height: height ?? 0 })
  if (status === 'error')
    return t('studio.gui.inspector.asset_missing')
  return t('studio.gui.inspector.asset_loading')
}

function parsePropertyInput(value: string, current: GuiPropertyValue): GuiPropertyValue {
  if (typeof current === 'number') {
    const parsed = Number(value)
    return Number.isFinite(parsed) ? parsed : current
  }
  if (current == null && value === 'nil')
    return null
  if (isRawLuaLiteral(current))
    return { luaLiteral: value }
  return value
}

function propertyDisplayValue(value: GuiPropertyValue): string | number {
  if (value == null)
    return 'nil'
  if (isRawLuaLiteral(value))
    return value.luaLiteral
  return String(value)
}

function isRawLuaLiteral(value: GuiPropertyValue): value is { luaLiteral: string } {
  return value != null && typeof value === 'object' && typeof value.luaLiteral === 'string'
}

function isTextNode(node: Mir3UiNode): boolean {
  return node.kind === 'Text' || node.kind === 'TextAtlas' || node.kind === 'RichText' || node.kind === 'ScrollText' || node.kind === 'TextInput' || node.kind === 'MenuItem'
}

function PropertySection({ title, children }: { title: string, children: React.ReactNode }) {
  return (
    <section className="border-b border-line p-3">
      <strong className="mb-3 block text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">{title}</strong>
      {children}
    </section>
  )
}

function PropertyInput({ label, value, writable, onCommit }: { label: string, value: string | number, writable: boolean, onCommit: (value: string) => void }) {
  return (
    <label className="block min-w-0">
      <span className="mb-1 block text-[9px] text-muted">{label}</span>
      <input
        className="h-8 w-full rounded-lg bg-panel-2 px-2 text-[11px] text-ink outline-none ring-1 ring-line focus:ring-accent disabled:cursor-not-allowed disabled:opacity-45"
        key={String(value)}
        defaultValue={value}
        disabled={!writable}
        onBlur={event => onCommit(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter')
            event.currentTarget.blur()
        }}
      />
    </label>
  )
}

function compatibilityClass(compatibility: Mir3UiNode['compatibility']): string {
  const base = 'shrink-0 rounded-full px-2 py-0.5 text-[8px] font-semibold uppercase tracking-[0.08em]'
  if (compatibility === 'supported')
    return `${base} bg-ok/10 text-ok`
  if (compatibility === 'approximate' || compatibility === 'dynamic')
    return `${base} bg-accent/10 text-accent`
  return `${base} bg-danger/10 text-danger`
}
