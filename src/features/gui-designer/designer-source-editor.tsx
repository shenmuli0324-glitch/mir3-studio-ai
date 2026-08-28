import { StreamLanguage } from '@codemirror/language'
import { lua } from '@codemirror/legacy-modes/mode/lua'
import { TriangleExclamation } from '@gravity-ui/icons'
import CodeMirror from '@uiw/react-codemirror'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'

export default function DesignerSourceEditor() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const file = scope.currentFile
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
