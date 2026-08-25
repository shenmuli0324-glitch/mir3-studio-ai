import type { ReactCodeMirrorRef } from '@uiw/react-codemirror'
import { StreamLanguage } from '@codemirror/language'
import { lua } from '@codemirror/legacy-modes/mode/lua'
import { TriangleExclamation } from '@gravity-ui/icons'
import CodeMirror from '@uiw/react-codemirror'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { DesignerCanvas } from './designer-canvas'
import { GuiDesignerScope } from './gui-designer-scope'

export function DesignerWorkspace() {
  const scope = useScope(GuiDesignerScope)
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex min-h-0 min-w-0 flex-1">
        <If cond={scope.mode === 'visual'}><DesignerCanvas /></If>
        <If cond={scope.mode === 'code'}><CodeEditor /></If>
        <If cond={scope.mode === 'split'}>
          <div className="flex min-h-0 min-w-0 flex-1">
            <div className="flex min-w-0 flex-1"><DesignerCanvas /></div>
            <div className="flex min-w-0 flex-1 border-l border-line"><CodeEditor /></div>
          </div>
        </If>
      </div>
      <DiagnosticsDrawer />
    </section>
  )
}

function CodeEditor() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const file = scope.currentFile
  const jump = scope.codeJump
  const editorRef = useRef<ReactCodeMirrorRef>(null)

  useEffect(() => {
    const view = editorRef.current?.view
    if (!jump || !view || jump.path !== file?.path)
      return
    const offset = sourceOffsetForLine(file.workingSource, jump.line)
    view.dispatch({
      selection: { anchor: offset },
      scrollIntoView: true,
    })
    view.focus()
  }, [file?.path, file?.workingSource, jump])
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col bg-[#111216]">
      <If cond={file?.valid === false}>
        <div className="flex shrink-0 items-center gap-2 bg-danger/10 px-3 py-2 text-[10px] text-danger">
          <TriangleExclamation className="size-3.5" />
          {t('studio.gui.code.invalid')}
        </div>
      </If>
      <If cond={file == null}>
        <div className="grid flex-1 place-items-center text-xs text-muted">{t('studio.gui.no_file')}</div>
      </If>
      <If cond={file != null}>
        <CodeMirror
          ref={editorRef}
          className="min-h-0 flex-1 overflow-auto bg-[#111216] text-[12px] [&_.cm-content]:font-mono [&_.cm-editor]:min-h-full [&_.cm-editor]:bg-[#111216] [&_.cm-gutters]:bg-[#111216] [&_.cm-gutters]:text-muted"
          height="100%"
          theme="dark"
          value={file?.workingSource ?? ''}
          basicSetup={{ foldGutter: true, highlightActiveLine: true, lineNumbers: true }}
          extensions={[StreamLanguage.define(lua)]}
          aria-label={t('studio.gui.code.editor')}
          onChange={scope.updateWorkingSource}
        />
      </If>
    </div>
  )
}

function sourceOffsetForLine(source: string, line: number): number {
  if (line <= 1)
    return 0
  let offset = 0
  for (let index = 1; index < line; index += 1) {
    const newline = source.indexOf('\n', offset)
    if (newline < 0)
      return source.length
    offset = newline + 1
  }
  return offset
}

function DiagnosticsDrawer() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const diagnostics = scope.previewDocument?.diagnostics ?? scope.currentFile?.document.diagnostics ?? []
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
          {diagnostics.map(diagnostic => (
            <div className="flex gap-2 py-1 text-[10px]" key={`${diagnostic.code}-${diagnostic.span?.startByte ?? diagnostic.message}`}>
              <span className={diagnostic.severity === 'error' ? 'text-danger' : 'text-accent'}>{diagnostic.code}</span>
              <span className="text-muted">{diagnostic.message}</span>
            </div>
          ))}
          <If cond={scope.currentFile?.parseError != null}><p className="py-1 text-[10px] text-danger">{scope.currentFile?.parseError}</p></If>
          <If cond={scope.error != null}><p className="py-1 text-[10px] text-danger">{String(scope.error)}</p></If>
        </div>
      </If>
    </div>
  )
}
