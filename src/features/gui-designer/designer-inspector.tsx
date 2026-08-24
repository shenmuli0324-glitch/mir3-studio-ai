import type { Mir3UiNode } from './types'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
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
      <If cond={node.kind === 'Text'}>
        <PropertySection title={t('studio.gui.inspector.content')}>
          <PropertyInput label={t('studio.gui.inspector.text')} value={node.paint?.text?.value ?? ''} writable={scope.nodePropertyWritable(node, 'text') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'text', value)} />
        </PropertySection>
      </If>
      <If cond={node.kind === 'Image' || node.kind === 'Button'}>
        <PropertySection title={t('studio.gui.inspector.asset')}>
          <PropertyInput label={t('studio.gui.inspector.image')} value={node.paint?.image?.value ?? ''} writable={scope.nodePropertyWritable(node, 'image') && !scope.parsePending} onCommit={value => scope.updateNodeProperty(node.id, 'image', value)} />
          <p className="mt-2 text-[9px] leading-4 text-muted">{t('studio.gui.inspector.asset_hint')}</p>
        </PropertySection>
      </If>
      <If cond={node.compatibility !== 'supported'}>
        <div className="m-3 rounded-lg bg-danger/8 px-3 py-2 text-[10px] leading-4 text-danger ring-1 ring-danger/20">{t('studio.gui.inspector.unsupported_hint')}</div>
      </If>
    </div>
  )
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
  if (compatibility === 'partial')
    return `${base} bg-accent/10 text-accent`
  return `${base} bg-danger/10 text-danger`
}
