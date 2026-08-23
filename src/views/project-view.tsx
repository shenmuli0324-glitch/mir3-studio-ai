import { ClockArrowRotateLeft, FolderOpen, FolderPlus } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { EmptyPanel, PhaseNotice, PrincipleStrip, ViewFrame, ViewHeader } from './view-primitives'

export function ProjectView() {
  const { t } = useTranslation()
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.project.eyebrow')}
        title={t('studio.project.title')}
        description={t('studio.project.description')}
      />
      <PhaseNotice />
      <section className="grid grid-cols-2 gap-4 max-[760px]:grid-cols-1">
        <Button className="h-auto min-h-32 justify-start rounded-2xl border border-line bg-panel p-5 text-left opacity-60" variant="ghost" isDisabled>
          <FolderPlus className="size-7 shrink-0 text-accent" />
          <span className="ml-3 whitespace-normal">
            <strong className="block text-base text-ink">{t('studio.project.create')}</strong>
            <small className="mt-2 block text-xs leading-5 text-muted">{t('studio.project.create_desc')}</small>
          </span>
        </Button>
        <Button className="h-auto min-h-32 justify-start rounded-2xl border border-line bg-panel p-5 text-left opacity-60" variant="ghost" isDisabled>
          <FolderOpen className="size-7 shrink-0 text-accent" />
          <span className="ml-3 whitespace-normal">
            <strong className="block text-base text-ink">{t('studio.project.open')}</strong>
            <small className="mt-2 block text-xs leading-5 text-muted">{t('studio.project.open_desc')}</small>
          </span>
        </Button>
      </section>
      <PrincipleStrip />
      <EmptyPanel
        icon={<ClockArrowRotateLeft className="size-5" />}
        title={t('studio.project.recent_empty')}
        description={t('studio.project.recent_empty_desc')}
      />
    </ViewFrame>
  )
}
