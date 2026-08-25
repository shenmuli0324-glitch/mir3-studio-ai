import type { GuiComponentCategory, GuiComponentKind } from './component-catalog'
import type { Mir3UiNode } from './types'
import { ChevronRight, Layers, Picture, Square, Text } from '@gravity-ui/icons'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GUI_COMPONENTS, isContainerKind } from './component-catalog'
import { DevFileTree } from './dev-file-tree'
import { GuiDesignerScope } from './gui-designer-scope'

const COMPONENT_CATEGORIES: readonly GuiComponentCategory[] = ['basic', 'text-input', 'container', 'progress', 'runtime']

export function DesignerSidebar() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-line bg-panel">
      <div className="grid h-10 shrink-0 grid-cols-3 border-b border-line p-1">
        <SideTab active={scope.leftPanel === 'files'} label={t('studio.gui.panel.files')} onPress={() => scope.setLeftPanel('files')} />
        <SideTab active={scope.leftPanel === 'layers'} label={t('studio.gui.panel.layers')} onPress={() => scope.setLeftPanel('layers')} />
        <SideTab active={scope.leftPanel === 'components'} label={t('studio.gui.panel.components')} onPress={() => scope.setLeftPanel('components')} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        <If cond={scope.leftPanel === 'files'}><FilePanel /></If>
        <If cond={scope.leftPanel === 'layers'}><LayerPanel /></If>
        <If cond={scope.leftPanel === 'components'}><ComponentPanel /></If>
      </div>
      <footer className="shrink-0 border-t border-line px-3 py-2">
        <span className="block truncate text-[10px] text-muted">
          {scope.activeProject?.clientRoot}
          /dev
        </span>
      </footer>
    </aside>
  )
}

function FilePanel() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  if (!scope.activeProject)
    return <p className="px-2 py-4 text-center text-[11px] text-muted">{t('studio.gui.no_project')}</p>
  const newPaths = Object.values(scope.files).filter(file => file.isNew).map(file => file.path)
  return <DevFileTree projectId={scope.activeProject.id} currentPath={scope.currentPath} newPaths={newPaths} onOpenFile={scope.openFile} key={scope.activeProject.id} />
}

function LayerPanel() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const document = scope.previewDocument
  const roots = document?.roots ?? []
  const unresolvedRoots = roots.filter(id => document?.nodes[id]?.compatibilityReasonCode === 'unresolved_parent')
  const normalRoots = roots.filter(id => document?.nodes[id]?.compatibilityReasonCode !== 'unresolved_parent')

  function toggleNode(nodeId: string) {
    setCollapsed(value => ({ ...value, [nodeId]: !value[nodeId] }))
  }

  return (
    <div>
      <PanelHeading icon={<Layers />} title={t('studio.gui.layers.title')} />
      <If cond={document == null}><p className="px-2 py-4 text-center text-[11px] text-muted">{t('studio.gui.layers.empty')}</p></If>
      <If cond={document != null}>
        <div className="space-y-0.5">
          {normalRoots.map(id => <LayerNode nodeId={id} depth={0} collapsed={collapsed} onToggle={toggleNode} key={id} />)}
          <If cond={unresolvedRoots.length > 0}>
            <button className="flex h-7 w-full items-center gap-2 rounded-md px-2 text-[10px] text-warning hover:bg-panel-hover" type="button" onClick={() => toggleNode('__unresolved__')}>
              <ChevronRight className={chevronClass(collapsed.__unresolved__)} />
              <Layers className="size-3.5" />
              <span>{t('studio.gui.layers.unresolved_group')}</span>
              <span className="ml-auto text-[9px] tabular-nums">{unresolvedRoots.length}</span>
            </button>
            <If cond={!collapsed.__unresolved__}>
              {unresolvedRoots.map(id => <LayerNode nodeId={id} depth={1} collapsed={collapsed} onToggle={toggleNode} key={id} />)}
            </If>
          </If>
        </div>
      </If>
    </div>
  )
}

function LayerNode({ nodeId, depth, collapsed, onToggle }: { nodeId: string, depth: number, collapsed: Record<string, boolean>, onToggle: (nodeId: string) => void }) {
  const scope = useScope(GuiDesignerScope)
  const node = scope.previewDocument?.nodes[nodeId]
  if (!node)
    return null
  return (
    <>
      <button
        className={layerClass(scope.selectedNodeId === nodeId, node.compatibility === 'unknown')}
        style={{ paddingLeft: 8 + depth * 12 }}
        type="button"
        onClick={() => scope.setSelectedNodeId(nodeId)}
      >
        <If cond={node.children.length > 0}>
          <span
            className="grid size-4 shrink-0 place-items-center"
            role="button"
            tabIndex={0}
            onClick={(event) => {
              event.stopPropagation()
              onToggle(nodeId)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault()
                event.stopPropagation()
                onToggle(nodeId)
              }
            }}
          >
            <ChevronRight className={chevronClass(collapsed[nodeId])} />
          </span>
        </If>
        <If cond={node.children.length === 0}><span className="size-4 shrink-0" /></If>
        <NodeGlyph node={node} />
        <span className="min-w-0 flex-1 truncate text-left">{node.name?.value || node.luaVariable || node.kind}</span>
      </button>
      <If cond={!collapsed[nodeId]}>
        {node.children.map(childId => <LayerNode nodeId={childId} depth={depth + 1} collapsed={collapsed} onToggle={onToggle} key={childId} />)}
      </If>
    </>
  )
}

function ComponentPanel() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  return (
    <div>
      <PanelHeading icon={<Square />} title={t('studio.gui.components.title')} />
      <p className="mb-3 px-2 text-[10px] leading-4 text-muted">{t('studio.gui.components.hint')}</p>
      {COMPONENT_CATEGORIES.map(category => (
        <section className="mb-4" key={category}>
          <strong className="mb-2 block px-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-muted">{t(`studio.gui.component.category.${category}`)}</strong>
          <div className="grid grid-cols-2 gap-2">
            {GUI_COMPONENTS.filter(item => item.category === category).map(item => (
              <ComponentButton kind={item.kind} disabled={!scope.currentFile || scope.parsePending} onAdd={scope.addNode} key={item.kind} />
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}

function ComponentButton({ kind, disabled, onAdd }: { kind: GuiComponentKind, disabled: boolean, onAdd: (kind: GuiComponentKind) => void }) {
  const { t } = useTranslation()
  return (
    <button
      className="group flex min-h-16 flex-col items-center justify-center gap-1.5 rounded-xl bg-panel-2 px-1 text-muted ring-1 ring-line transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:-translate-y-0.5 hover:text-ink active:scale-[0.98] disabled:opacity-35"
      type="button"
      draggable
      disabled={disabled}
      onClick={() => onAdd(kind)}
      onDragStart={(event) => {
        event.dataTransfer.setData('application/x-mir3-ui-kind', kind)
        event.dataTransfer.effectAllowed = 'copy'
      }}
    >
      {componentIcon(kind)}
      <span className="max-w-full truncate text-[9px]">{t(`studio.gui.component.${kind.toLowerCase()}`)}</span>
    </button>
  )
}

function componentIcon(kind: GuiComponentKind) {
  if (kind === 'Image' || kind === 'Button' || kind === 'CheckBox' || kind === 'Slider' || kind === 'ProgressTimer' || kind === 'LoadingBar')
    return <Picture className="size-4 text-accent" />
  if (kind === 'Text' || kind === 'TextAtlas' || kind === 'RichText' || kind === 'ScrollText' || kind === 'TextInput')
    return <Text className="size-4 text-accent" />
  if (isContainerKind(kind))
    return <Layers className="size-4 text-accent" />
  return <Square className="size-4 text-accent" />
}

function PanelHeading({ icon, title }: { icon: React.ReactNode, title: string }) {
  return (
    <div className="mb-2 flex h-7 items-center gap-2 px-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted">
      {icon}
      {title}
    </div>
  )
}

function SideTab({ active, label, onPress }: { active: boolean, label: string, onPress: () => void }) {
  return <button className={tabClass(active)} type="button" onClick={onPress}>{label}</button>
}

function NodeGlyph({ node }: { node: Mir3UiNode }) {
  return <span className="grid size-4 shrink-0 place-items-center rounded text-[8px] font-semibold text-accent ring-1 ring-accent/25">{node.kind.slice(0, 1)}</span>
}

function tabClass(active: boolean): string {
  if (active)
    return 'rounded-md bg-panel-2 text-[10px] font-medium text-ink'
  return 'rounded-md text-[10px] text-muted hover:text-ink'
}

function layerClass(active: boolean, unsupported: boolean): string {
  const base = 'flex h-7 w-full items-center gap-2 rounded-md pr-2 text-[10px]'
  if (active)
    return `${base} bg-accent/12 text-accent`
  if (unsupported)
    return `${base} text-danger/80 hover:bg-panel-hover`
  return `${base} text-muted hover:bg-panel-hover hover:text-ink`
}

function chevronClass(collapsed: boolean | undefined): string {
  if (collapsed)
    return 'size-3 transition-transform'
  return 'size-3 rotate-90 transition-transform'
}
