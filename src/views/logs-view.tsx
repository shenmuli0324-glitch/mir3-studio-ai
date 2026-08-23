import { FileMagnifier, Magnifier } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { EmptyPanel, PhaseNotice, ViewFrame, ViewHeader } from './view-primitives'

export function LogsView() {
  const { t } = useTranslation()
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.logs.eyebrow')}
        title={t('studio.logs.title')}
        description={t('studio.logs.description')}
      />
      <PhaseNotice />
      <section className="flex gap-3 rounded-2xl border border-line bg-panel p-4 opacity-60 max-[720px]:flex-col">
        <label className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg border border-line bg-panel2 px-3 text-muted">
          <Magnifier className="size-4" />
          <input className="min-w-0 flex-1 bg-transparent text-sm outline-none" placeholder={t('studio.logs.search')} disabled />
        </label>
        <select className="h-9 rounded-lg border border-line bg-panel2 px-3 text-sm text-muted" disabled aria-label={t('studio.logs.level')}>
          <option>{t('studio.logs.all_levels')}</option>
        </select>
      </section>
      <EmptyPanel
        icon={<FileMagnifier className="size-5" />}
        title={t('studio.logs.empty')}
        description={t('studio.logs.empty_desc')}
      />
    </ViewFrame>
  )
}
