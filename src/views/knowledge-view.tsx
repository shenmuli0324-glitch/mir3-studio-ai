import { BookOpen } from '@gravity-ui/icons'
import { useTranslation } from 'react-i18next'
import { EmptyPanel, ViewFrame, ViewHeader } from './view-primitives'

export function KnowledgeView() {
  const { t } = useTranslation()

  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.knowledge.eyebrow')}
        title={t('studio.knowledge.title')}
        description={t('studio.knowledge.description')}
      />
      <EmptyPanel
        icon={<BookOpen className="size-5" />}
        title={t('studio.knowledge.empty')}
        description={t('studio.knowledge.empty_desc')}
      />
    </ViewFrame>
  )
}
