import type { GuiTemplateRequest } from './types'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'

export function DesignerDialogs() {
  const scope = useScope(GuiDesignerScope)
  return (
    <If cond={scope.newDialogOpen}><NewPageDialog /></If>
  )
}

function NewPageDialog() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const [relativePath, setRelativePath] = useState(() => t('studio.gui.new_dialog.default_path'))
  const [targets, setTargets] = useState<GuiTemplateRequest['targets']>('mobile')
  const valid = validRelativePath(relativePath)
  return (
    <DialogFrame title={t('studio.gui.new_dialog.title')} onClose={() => scope.setNewDialogOpen(false)}>
      <p className="mb-5 text-xs leading-5 text-muted">{t('studio.gui.new_dialog.description')}</p>
      <label className="block">
        <span className="mb-1.5 block text-[10px] font-medium text-muted">{t('studio.gui.new_dialog.path')}</span>
        <div className="flex h-9 items-center rounded-lg bg-panel-2 px-3 ring-1 ring-line focus-within:ring-accent">
          <span className="text-[11px] text-muted">GUIExport/</span>
          <input className="min-w-0 flex-1 bg-transparent text-[11px] text-ink outline-none" value={relativePath} onChange={event => setRelativePath(event.target.value)} />
        </div>
      </label>
      <If cond={!valid}><p className="mt-2 text-[10px] text-danger">{t('studio.gui.new_dialog.path_invalid')}</p></If>
      <div className="mt-5">
        <span className="mb-2 block text-[10px] font-medium text-muted">{t('studio.gui.new_dialog.targets')}</span>
        <div className="grid grid-cols-3 gap-2">
          {(['mobile', 'pc', 'both'] as const).map(target => (
            <button className={targetClass(targets === target)} type="button" onClick={() => setTargets(target)} key={target}>{t(`studio.gui.new_dialog.target.${target}`)}</button>
          ))}
        </div>
      </div>
      <div className="mt-6 flex justify-end gap-2">
        <button className="h-8 rounded-lg px-3 text-[11px] text-muted hover:bg-panel-hover" type="button" onClick={() => scope.setNewDialogOpen(false)}>{t('studio.gui.cancel')}</button>
        <button className="h-8 rounded-lg bg-accent px-4 text-[11px] font-medium text-white disabled:opacity-40" type="button" disabled={!valid || scope.busy} onClick={() => void scope.createPage({ relativePath, targets }).catch(() => {})}>{t('studio.gui.create')}</button>
      </div>
    </DialogFrame>
  )
}

function DialogFrame({ title, children, onClose }: { title: string, children: React.ReactNode, onClose: () => void }) {
  return (
    <div className="absolute inset-0 z-20 grid place-items-center bg-black/55 p-6" role="dialog" aria-modal="true" aria-label={title} onMouseDown={onClose}>
      <div className={dialogClass()} onMouseDown={event => event.stopPropagation()}>
        <header className="mb-4 flex items-center justify-between">
          <strong className="text-sm font-semibold text-ink">{title}</strong>
          <button className="grid size-7 place-items-center rounded-lg text-muted hover:bg-panel-hover hover:text-ink" type="button" onClick={onClose}>×</button>
        </header>
        {children}
      </div>
    </div>
  )
}

function validRelativePath(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed || trimmed.startsWith('/') || trimmed.startsWith('\\') || trimmed.includes('..'))
    return false
  const hasControlCharacter = Array.from(trimmed).some(character => character.charCodeAt(0) < 32)
  return !hasControlCharacter && !/[<>:"|?*]/.test(trimmed)
}

function targetClass(active: boolean): string {
  const base = 'h-16 rounded-xl text-[11px] ring-1'
  if (active)
    return `${base} bg-accent/12 text-accent ring-accent/40`
  return `${base} bg-panel-2 text-muted ring-line hover:text-ink`
}

function dialogClass(): string {
  return 'max-h-[calc(100vh-64px)] w-[min(440px,calc(100vw-64px))] overflow-auto rounded-2xl bg-panel p-5 shadow-[0_32px_100px_rgba(0,0,0,0.35)] ring-1 ring-line-strong'
}
