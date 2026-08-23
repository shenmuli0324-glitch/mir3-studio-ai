import { FileZipper } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { DisabledAction, EmptyPanel, PhaseNotice, ViewFrame, ViewHeader } from './view-primitives'

export function BuildsView() {
  const { t } = useTranslation()
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.builds.eyebrow')}
        title={t('studio.builds.title')}
        description={t('studio.builds.description')}
        action={<DisabledAction>{t('studio.builds.action')}</DisabledAction>}
      />
      <PhaseNotice />
      <EmptyPanel
        icon={<FileZipper className="size-5" />}
        title={t('studio.builds.empty')}
        description={t('studio.builds.empty_desc')}
      />
    </ViewFrame>
  )
}
