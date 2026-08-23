import { FileArrowDown, FileArrowUp } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { EmptyPanel, PhaseNotice, ViewFrame, ViewHeader } from './view-primitives'

export function FeedbackView() {
  const { t } = useTranslation()
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.feedback.eyebrow')}
        title={t('studio.feedback.title')}
        description={t('studio.feedback.description')}
        action={(
          <Button className="rounded-lg" size="sm" variant="tertiary" isDisabled>
            <FileArrowUp className="size-4" />
            {t('studio.feedback.import')}
          </Button>
        )}
      />
      <PhaseNotice />
      <section className="flex items-center justify-between gap-5 rounded-2xl border border-line bg-panel p-5 max-[720px]:items-start max-[720px]:flex-col">
        <div>
          <span className="text-[11px] font-semibold uppercase tracking-[0.16em] text-accent">{t('studio.feedback.compose_eyebrow')}</span>
          <h2 className="mt-2 text-lg font-semibold text-ink">{t('studio.feedback.compose_title')}</h2>
          <p className="mt-1 text-sm text-muted">{t('studio.feedback.compose_desc')}</p>
        </div>
        <Button className="shrink-0 rounded-lg" size="sm" variant="primary" isDisabled>
          <FileArrowDown className="size-4" />
          {t('studio.feedback.export')}
        </Button>
      </section>
      <EmptyPanel
        icon={<FileArrowUp className="size-5" />}
        title={t('studio.feedback.empty')}
        description={t('studio.feedback.empty_desc')}
      />
    </ViewFrame>
  )
}
