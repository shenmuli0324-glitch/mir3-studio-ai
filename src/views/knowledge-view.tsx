import { BookOpen, Boxes3 } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { EmptyPanel, PhaseNotice, ViewFrame, ViewHeader } from './view-primitives'

type KnowledgeSection = 'records' | 'assets'

export function KnowledgeView() {
  const { t } = useTranslation()
  const [section, setSection] = useState<KnowledgeSection>('records')
  const recordsActive = section === 'records'
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.knowledge.eyebrow')}
        title={t('studio.knowledge.title')}
        description={t('studio.knowledge.description')}
      />
      <PhaseNotice />
      <div className="flex w-fit gap-1 rounded-xl border border-line bg-panel p-1">
        <Button className={knowledgeTabClass(recordsActive)} size="sm" variant="ghost" onPress={() => setSection('records')}>
          <BookOpen className="size-4" />
          {t('studio.knowledge.records')}
        </Button>
        <Button className={knowledgeTabClass(!recordsActive)} size="sm" variant="ghost" onPress={() => setSection('assets')}>
          <Boxes3 className="size-4" />
          {t('studio.knowledge.assets')}
        </Button>
      </div>
      <If
        cond={recordsActive}
        then={(
          <EmptyPanel
            icon={<BookOpen className="size-5" />}
            title={t('studio.knowledge.records_empty')}
            description={t('studio.knowledge.records_empty_desc')}
          />
        )}
        else={(
          <EmptyPanel
            icon={<Boxes3 className="size-5" />}
            title={t('studio.knowledge.assets_empty')}
            description={t('studio.knowledge.assets_empty_desc')}
          />
        )}
      />
    </ViewFrame>
  )
}

function knowledgeTabClass(active: boolean): string {
  if (active)
    return 'rounded-lg bg-panel2 text-ink'
  return 'rounded-lg text-muted'
}
