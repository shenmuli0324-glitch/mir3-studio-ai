import { ClockArrowRotateLeft, Eye, FloppyDisk } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'

export function DomainWorkingToolbar({ projectName, systemId, selectedPath, dirty, changeCount, canRestore, busy, onSave, onRestore, onReview }: {
  projectName?: string
  systemId: string
  selectedPath?: string
  dirty: boolean
  changeCount: number
  canRestore: boolean
  busy: boolean
  onSave: () => void
  onRestore: () => void
  onReview: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <span className="min-w-0 flex-1 truncate text-[10px] text-muted">
        <span>{projectName ?? systemId}</span>
        <If cond={selectedPath != null}>
          <span className="ml-3 border-l border-line pl-3 font-mono">{selectedPath}</span>
        </If>
        <If cond={dirty}>
          <span className="ml-2 text-accent">
            ●
            {' '}
            {t('studio.devtools.working.unsaved')}
          </span>
        </If>
      </span>
      <button className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-muted hover:bg-panel-hover hover:text-ink disabled:opacity-40" type="button" disabled={!dirty || busy} onClick={onReview}>
        <Eye className="size-3.5" />
        {t('studio.devtools.working.review')}
        <If cond={changeCount > 0}><span>{changeCount}</span></If>
      </button>
      <button className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-muted hover:bg-panel-hover hover:text-ink disabled:opacity-40" type="button" disabled={!canRestore || dirty || busy} onClick={onRestore}>
        <ClockArrowRotateLeft className="size-3.5" />
        {t('studio.devtools.working.restore')}
      </button>
      <button className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-accent px-3 text-[11px] font-medium text-white transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97] disabled:opacity-40" type="button" disabled={!dirty || busy} onClick={onSave}>
        <FloppyDisk className="size-3.5" />
        {t('studio.devtools.working.save')}
      </button>
    </div>
  )
}
