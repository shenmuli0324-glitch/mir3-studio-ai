import type { Mir3UiNode } from './types'
import { FileCode, FolderOpen, Layers, Picture, Square, Text } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'

const COMPONENTS = [
  { kind: 'Panel', icon: Square },
  { kind: 'Image', icon: Picture },
  { kind: 'Text', icon: Text },
  { kind: 'Button', icon: Square },
] as const

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
  const virtualFiles = Object.values(scope.files).filter(file => file.isNew)
  return (
    <div>
      <PanelHeading icon={<FolderOpen />} title={t('studio.gui.files.gui_export')} />
      <If cond={scope.entriesLoading}><p className="px-2 py-4 text-center text-[11px] text-muted">{t('studio.gui.loading')}</p></If>
      <div className="space-y-0.5">
        {scope.entries.map(entry => (
          <button
            className={fileClass(scope.currentPath === entry.path, entry.kind === 'readonly')}
            type="button"
            key={entry.path}
            disabled={entry.kind === 'readonly'}
            onClick={() => void scope.openFile(entry.path).catch(() => {})}
          >
            <FileCode className="size-3.5 shrink-0" />
            <span className="min-w-0 flex-1 truncate text-left">{entry.path.replace(/^GUIExport\//, '')}</span>
            <If cond={entry.kind === 'readonly'}><span className="text-[9px] uppercase">{t('studio.gui.readonly')}</span></If>
          </button>
        ))}
        {virtualFiles.map(file => (
          <button className={fileClass(scope.currentPath === file.path, false)} type="button" key={file.path} onClick={() => scope.setCurrentPath(file.path)}>
            <FileCode className="size-3.5 shrink-0 text-accent" />
            <span className="min-w-0 flex-1 truncate text-left">{file.path.replace(/^GUIExport\//, '')}</span>
            <span className="text-[9px] uppercase text-accent">{t('studio.gui.new_badge')}</span>
          </button>
        ))}
      </div>
    </div>
  )
}

function LayerPanel() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const file = scope.currentFile
  return (
    <div>
      <PanelHeading icon={<Layers />} title={t('studio.gui.layers.title')} />
      <If cond={file == null}><p className="px-2 py-4 text-center text-[11px] text-muted">{t('studio.gui.layers.empty')}</p></If>
      <If cond={file != null}>
        <div className="space-y-0.5">
          {file?.document.roots.map(id => <LayerNode nodeId={id} depth={0} key={id} />)}
        </div>
      </If>
    </div>
  )
}

function LayerNode({ nodeId, depth }: { nodeId: string, depth: number }) {
  const scope = useScope(GuiDesignerScope)
  const node = scope.currentFile?.document.nodes[nodeId]
  if (!node)
    return null
  return (
    <>
      <button
        className={layerClass(scope.selectedNodeId === nodeId, node.compatibility === 'unsupported')}
        style={{ paddingLeft: 8 + depth * 12 }}
        type="button"
        onClick={() => scope.setSelectedNodeId(nodeId)}
      >
        <NodeGlyph node={node} />
        <span className="min-w-0 flex-1 truncate text-left">{node.name?.value || node.luaVariable || node.kind}</span>
      </button>
      {node.children.map(childId => <LayerNode nodeId={childId} depth={depth + 1} key={childId} />)}
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
      <div className="grid grid-cols-2 gap-2">
        {COMPONENTS.map((item) => {
          const Icon = item.icon
          return (
            <button
              className="group flex min-h-20 flex-col items-center justify-center gap-2 rounded-xl bg-panel-2 text-muted ring-1 ring-line transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:-translate-y-0.5 hover:text-ink active:scale-[0.98] disabled:opacity-35"
              type="button"
              draggable
              disabled={!scope.currentFile || scope.parsePending}
              key={item.kind}
              onClick={() => scope.addNode(item.kind)}
              onDragStart={(event) => {
                event.dataTransfer.setData('application/x-mir3-ui-kind', item.kind)
                event.dataTransfer.effectAllowed = 'copy'
              }}
            >
              <Icon className="size-5 text-accent" />
              <span className="text-[11px]">{t(`studio.gui.component.${item.kind.toLowerCase()}`)}</span>
            </button>
          )
        })}
      </div>
    </div>
  )
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

function fileClass(active: boolean, readonly: boolean): string {
  const base = 'flex h-8 w-full items-center gap-2 rounded-lg px-2 text-[10px]'
  if (active)
    return `${base} bg-accent/12 text-accent`
  if (readonly)
    return `${base} text-muted/60 hover:bg-panel-hover`
  return `${base} text-muted hover:bg-panel-hover hover:text-ink`
}

function layerClass(active: boolean, unsupported: boolean): string {
  const base = 'flex h-7 w-full items-center gap-2 rounded-md pr-2 text-[10px]'
  if (active)
    return `${base} bg-accent/12 text-accent`
  if (unsupported)
    return `${base} text-danger/80 hover:bg-panel-hover`
  return `${base} text-muted hover:bg-panel-hover hover:text-ink`
}
