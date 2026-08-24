import type { DevToolDefinition } from '../devtool-registry'
import { CircleInfo, Wrench } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { devToolDescriptionKey, devToolTitleKey } from '../devtool-registry'
import { DevToolWorkspace } from './devtool-workspace'

export function PlannedToolView({ tool, onBack }: { tool: DevToolDefinition, onBack: () => void }) {
  const { t } = useTranslation()
  const Icon = tool.icon

  return (
    <DevToolWorkspace
      tool={tool}
      onBack={onBack}
      sidebar={(
        <div className="flex h-full flex-col p-3">
          <span className="px-2 pb-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">{t('studio.devtools.system_menu')}</span>
          <div className="flex items-center gap-3 rounded-xl bg-accent/12 px-3 py-3 text-accent">
            <CircleInfo className="size-4 shrink-0" />
            <span className="text-xs font-medium">{t('studio.devtools.overview')}</span>
          </div>
        </div>
      )}
      toolbar={(
        <div className="flex w-full items-center justify-between gap-4">
          <span className="min-w-0">
            <strong className="block truncate text-sm font-semibold text-ink">{t(devToolTitleKey(tool.id))}</strong>
            <small className="mt-0.5 block truncate text-[11px] text-muted">{t(devToolDescriptionKey(tool.id))}</small>
          </span>
          <Button className="rounded-lg border border-line text-muted disabled:text-muted" size="sm" variant="ghost" isDisabled>{t('studio.devtools.planned_action')}</Button>
        </div>
      )}
    >
      <div className="grid h-full place-items-center p-6">
        <div className="flex max-w-md flex-col items-center text-center">
          <span className="grid size-14 place-items-center rounded-2xl border border-line bg-panel text-accent"><Icon className="size-7" /></span>
          <h2 className="mt-5 text-lg font-semibold text-ink">{t(devToolTitleKey(tool.id))}</h2>
          <p className="mt-2 text-sm leading-6 text-muted">{t('studio.devtools.planned_desc')}</p>
          <span className="mt-5 inline-flex items-center gap-2 rounded-full border border-line bg-panel px-3 py-1.5 text-xs text-muted">
            <Wrench className="size-3.5" />
            {t('studio.devtools.status.planned')}
          </span>
        </div>
      </div>
    </DevToolWorkspace>
  )
}
