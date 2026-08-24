import { CirclePlay, Server } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { DisabledAction, EmptyPanel, PhaseNotice, SectionPanel, ViewFrame, ViewHeader } from './view-primitives'

export function RuntimeView() {
  const { t } = useTranslation()
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.runtime.eyebrow')}
        title={t('studio.runtime.title')}
        description={t('studio.runtime.description')}
        action={<DisabledAction>{t('studio.runtime.action')}</DisabledAction>}
      />
      <PhaseNotice />
      <div className="grid grid-cols-2 gap-4 max-[820px]:grid-cols-1">
        <SectionPanel title={t('studio.runtime.profile')} description={t('studio.runtime.profile_desc')}>
          <div className="p-4">
            <EmptyPanel icon={<Server className="size-5" />} title={t('studio.runtime.unconfigured')} description={t('studio.runtime.unconfigured_desc')} />
          </div>
        </SectionPanel>
        <SectionPanel title={t('studio.runtime.processes')} description={t('studio.runtime.processes_desc')}>
          <div className="p-4">
            <EmptyPanel icon={<CirclePlay className="size-5" />} title={t('studio.runtime.stopped')} description={t('studio.runtime.stopped_desc')} />
          </div>
        </SectionPanel>
      </div>
    </ViewFrame>
  )
}
