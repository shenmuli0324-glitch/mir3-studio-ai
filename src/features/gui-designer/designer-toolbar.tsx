import { ArrowUturnCcwLeft, ArrowUturnCwRight, CodeCompare, Display, MagnifierMinus, MagnifierPlus, Plus, Smartphone } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'
import { PC_VIEWPORTS } from './types'

export function DesignerToolbar() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const file = scope.currentFile
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-panel px-3">
      <div className="flex items-center rounded-lg bg-panel-2 p-0.5">
        <ToolbarSegment active={scope.device === 'mobile'} label={t('studio.gui.device.mobile')} onPress={() => scope.setDevice('mobile')} icon={<Smartphone />} />
        <ToolbarSegment active={scope.device === 'pc'} label={t('studio.gui.device.pc')} onPress={() => scope.setDevice('pc')} icon={<Display />} />
      </div>
      <If cond={scope.device === 'pc'}>
        <select
          className="h-8 rounded-lg bg-panel-2 px-2 text-[11px] text-ink outline-none ring-1 ring-line focus:ring-accent"
          aria-label={t('studio.gui.pc_resolution')}
          value={scope.pcViewportIndex}
          onChange={event => scope.setPcViewportIndex(Number(event.target.value))}
        >
          {PC_VIEWPORTS.map((viewport, index) => (
            <option key={`${viewport.width}x${viewport.height}`} value={index}>
              {viewport.width}
              {' '}
              ×
              {' '}
              {viewport.height}
            </option>
          ))}
        </select>
      </If>
      <span className="mx-1 h-5 w-px bg-line" />
      <div className="flex items-center rounded-lg bg-panel-2 p-0.5">
        <ToolbarSegment active={scope.interactionMode === 'design'} label={t('studio.gui.interaction.design')} onPress={() => scope.setInteractionMode('design')} />
        <ToolbarSegment active={scope.interactionMode === 'interact'} label={t('studio.gui.interaction.interact')} onPress={() => scope.setInteractionMode('interact')} />
      </div>
      <div className="flex items-center rounded-lg bg-panel-2 p-0.5">
        <ToolbarSegment active={scope.mode === 'visual'} label={t('studio.gui.mode.visual')} onPress={() => scope.setMode('visual')} />
        <ToolbarSegment active={scope.mode === 'code'} label={t('studio.gui.mode.code')} onPress={() => scope.setMode('code')} />
        <ToolbarSegment active={scope.mode === 'split'} label={t('studio.gui.mode.split')} onPress={() => scope.setMode('split')} />
      </div>
      <ToolbarIcon label={t('studio.gui.undo')} disabled={!file || file.history.length === 0} onPress={scope.undo}><ArrowUturnCcwLeft /></ToolbarIcon>
      <ToolbarIcon label={t('studio.gui.redo')} disabled={!file || file.future.length === 0} onPress={scope.redo}><ArrowUturnCwRight /></ToolbarIcon>
      <span className="min-w-0 flex-1 truncate text-center text-[11px] text-muted">
        <span>{t(scope.activeSceneProfile.titleKey)}</span>
        <If cond={file != null}><span className="ml-2 opacity-65">{file?.path}</span></If>
        <If cond={file != null && (file?.workingSource !== file?.originalSource || file?.isNew)}><span className="ml-2 text-accent">●</span></If>
      </span>
      <ToolbarIcon label={t('studio.gui.zoom_out')} onPress={() => scope.setZoom(scope.zoom - 0.1)}><MagnifierMinus /></ToolbarIcon>
      <button className="h-8 min-w-14 rounded-lg px-2 text-[11px] tabular-nums text-muted hover:bg-panel-hover hover:text-ink" type="button" title={t('studio.gui.fit_canvas')} onClick={scope.fitCanvas}>
        {Math.round(scope.zoom * 100)}
        %
      </button>
      <ToolbarIcon label={t('studio.gui.zoom_in')} onPress={() => scope.setZoom(scope.zoom + 0.1)}><MagnifierPlus /></ToolbarIcon>
      <button className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-muted hover:bg-panel-hover hover:text-ink" type="button" onClick={() => scope.setNewDialogOpen(true)}>
        <Plus className="size-3.5" />
        {t('studio.gui.new_page')}
      </button>
      <button
        className="flex h-8 items-center gap-1.5 rounded-lg bg-accent px-3 text-[11px] font-medium text-white transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97] disabled:opacity-40"
        type="button"
        disabled={!scope.dirty || scope.busy || file?.valid === false}
        onClick={() => void scope.prepareDiff().catch(() => {})}
      >
        <CodeCompare className="size-3.5" />
        {t('studio.gui.diff')}
      </button>
    </header>
  )
}

function ToolbarSegment({ active, label, onPress, icon }: { active: boolean, label: string, onPress: () => void, icon?: React.ReactNode }) {
  return (
    <button className={segmentClass(active)} type="button" onClick={onPress} aria-pressed={active}>
      {icon}
      <span>{label}</span>
    </button>
  )
}

function ToolbarIcon({ label, onPress, disabled = false, children }: { label: string, onPress: () => void, disabled?: boolean, children: React.ReactNode }) {
  return <button className="grid size-8 place-items-center rounded-lg text-muted hover:bg-panel-hover hover:text-ink disabled:opacity-30" type="button" aria-label={label} title={label} disabled={disabled} onClick={onPress}>{children}</button>
}

function segmentClass(active: boolean): string {
  const base = 'flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[11px] transition-colors duration-200'
  if (active)
    return `${base} bg-canvas text-ink shadow-[inset_0_0_0_1px_var(--color-line)]`
  return `${base} text-muted hover:text-ink`
}
