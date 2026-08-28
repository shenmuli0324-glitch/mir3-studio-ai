import type { GuiDiagnostic } from './types'
import { TriangleExclamation } from '@gravity-ui/icons'
import { lazy, Suspense } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { DesignerCanvas } from './designer-canvas'
import { GuiDesignerScope } from './gui-designer-scope'

const DesignerSourceEditor = lazy(() => import('./designer-source-editor'))

export function DesignerWorkspace() {
  const scope = useScope(GuiDesignerScope)
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex min-h-0 min-w-0 flex-1">
        <If cond={scope.mode === 'visual'}><DesignerCanvas /></If>
        <If cond={scope.mode === 'code'}><LazySourceEditor /></If>
        <If cond={scope.mode === 'split'}>
          <div className="flex min-h-0 min-w-0 flex-1">
            <div className="flex min-w-0 flex-1"><DesignerCanvas /></div>
            <div className="flex min-w-0 flex-1 border-l border-line"><LazySourceEditor /></div>
          </div>
        </If>
      </div>
      <DiagnosticsDrawer />
    </section>
  )
}

function LazySourceEditor() {
  const { t } = useTranslation()
  return (
    <Suspense fallback={<div className="grid min-h-0 min-w-0 flex-1 place-items-center bg-panel text-xs text-muted">{t('studio.gui.code.loading')}</div>}>
      <DesignerSourceEditor />
    </Suspense>
  )
}

function DiagnosticsDrawer() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const diagnostics = scope.previewDocument?.diagnostics ?? scope.currentFile?.document.diagnostics ?? []
  const summaries = summarizeDiagnostics(diagnostics)
  return (
    <div className="shrink-0 border-t border-line bg-panel">
      <button className="flex h-8 w-full items-center gap-2 px-3 text-[10px] text-muted hover:bg-panel-hover" type="button" onClick={() => scope.setDiagnosticsOpen(!scope.diagnosticsOpen)}>
        <TriangleExclamation className="size-3.5" />
        <span>{t('studio.gui.diagnostics')}</span>
        <span className="rounded-full bg-panel-2 px-1.5 py-0.5 tabular-nums">{diagnostics.length}</span>
        <span className="ml-auto">{scope.diagnosticsOpen ? '−' : '+'}</span>
      </button>
      <If cond={scope.diagnosticsOpen}>
        <div className="max-h-28 overflow-y-auto border-t border-line px-3 py-2">
          <If cond={diagnostics.length === 0}><p className="text-[10px] text-muted">{t('studio.gui.diagnostics.empty')}</p></If>
          {summaries.map(({ diagnostic, count }) => (
            <div className="flex gap-2 py-1 text-[10px]" key={`${diagnostic.severity}-${diagnostic.code}-${diagnostic.message}`}>
              <span className={diagnostic.severity === 'error' ? 'text-danger' : 'text-accent'}>{diagnostic.code}</span>
              <span className="min-w-0 flex-1 text-muted">{diagnostic.message}</span>
              <If cond={count > 1}><span className="shrink-0 text-warning">{t('studio.gui.diagnostics.repeated', { count })}</span></If>
            </div>
          ))}
          <If cond={scope.currentFile?.parseError != null}><p className="py-1 text-[10px] text-danger">{scope.currentFile?.parseError}</p></If>
          <If cond={scope.error != null}><p className="py-1 text-[10px] text-danger">{String(scope.error)}</p></If>
        </div>
      </If>
    </div>
  )
}

function summarizeDiagnostics(diagnostics: GuiDiagnostic[]): Array<{ diagnostic: GuiDiagnostic, count: number }> {
  const summaries = new Map<string, { diagnostic: GuiDiagnostic, count: number }>()
  for (const diagnostic of diagnostics) {
    const key = `${diagnostic.severity}\u0000${diagnostic.code}\u0000${diagnostic.message}`
    const existing = summaries.get(key)
    if (existing) {
      existing.count += 1
      continue
    }
    summaries.set(key, { diagnostic, count: 1 })
  }
  return [...summaries.values()]
}
