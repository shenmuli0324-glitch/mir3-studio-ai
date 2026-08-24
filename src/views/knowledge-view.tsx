import type { Mir3Project } from '@/features/projects/types'
import { BookOpen, Boxes3 } from '@gravity-ui/icons'
import { Button, Chip } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useProjectDetails } from '@/features/projects/use-project-details'
import { EmptyPanel, SectionPanel, ViewFrame, ViewHeader } from './view-primitives'

type KnowledgeSection = 'records' | 'assets'

export function KnowledgeView({ project }: { project: Mir3Project | null }) {
  const { t } = useTranslation()
  const [section, setSection] = useState<KnowledgeSection>('records')
  const details = useProjectDetails(project?.id)
  const recordsActive = section === 'records'
  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.knowledge.eyebrow')}
        title={t('studio.knowledge.title')}
        description={t('studio.knowledge.description')}
      />
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
          <If cond={project != null} else={<EmptyPanel icon={<BookOpen className="size-5" />} title={t('studio.knowledge.no_project')} description={t('studio.knowledge.no_project_desc')} />}>
            <If
              cond={details.knowledge.length > 0}
              else={<EmptyPanel icon={<BookOpen className="size-5" />} title={t('studio.knowledge.records_empty')} description={t('studio.knowledge.records_empty_desc')} />}
            >
              <SectionPanel title={t('studio.knowledge.records')} description={t('studio.knowledge.records_desc')}>
                <div className="divide-y divide-line">
                  {details.knowledge.map(record => (
                    <article className="px-5 py-4" key={record.id}>
                      <div className="flex items-start gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <strong className="text-sm text-ink">{record.summary}</strong>
                            <Chip size="sm" color={record.status === 'ACTIVE' ? 'success' : 'default'}>{record.status}</Chip>
                            <Chip size="sm">{record.kind}</Chip>
                          </div>
                          <p className="mt-2 whitespace-pre-wrap text-xs leading-5 text-muted">{record.body}</p>
                          <If cond={record.evidence.length > 0}>
                            <p className="mt-2 text-[11px] text-muted">{t('studio.knowledge.evidence', { count: record.evidence.length })}</p>
                          </If>
                        </div>
                        <div className="flex shrink-0 gap-2">
                          <If cond={record.status === 'PROPOSED' || record.status === 'CONTESTED'}>
                            <Button size="sm" variant="ghost" onPress={() => void details.setKnowledgeStatus({ knowledgeId: record.id, status: 'ACTIVE' })}>{t('studio.knowledge.activate')}</Button>
                          </If>
                          <If cond={record.status === 'ACTIVE'}>
                            <Button size="sm" variant="ghost" onPress={() => void details.setKnowledgeStatus({ knowledgeId: record.id, status: 'CONTESTED' })}>{t('studio.knowledge.contest')}</Button>
                          </If>
                          <If cond={record.status !== 'REVOKED' && record.status !== 'SUPERSEDED'}>
                            <Button size="sm" variant="ghost" onPress={() => void details.setKnowledgeStatus({ knowledgeId: record.id, status: 'REVOKED' })}>{t('studio.knowledge.revoke')}</Button>
                          </If>
                        </div>
                      </div>
                    </article>
                  ))}
                </div>
              </SectionPanel>
            </If>
          </If>
        )}
        else={(
          <section className="grid grid-cols-2 gap-4 max-[760px]:grid-cols-1">
            <AssetCard title={t('studio.knowledge.skill_title')} description={t('studio.knowledge.skill_desc')} status={t('studio.knowledge.preinstalled')} />
            <AssetCard title={t('studio.knowledge.mcp_title')} description={t('studio.knowledge.mcp_desc')} status={project ? t('studio.knowledge.project_bound') : t('studio.knowledge.waiting_project')} />
          </section>
        )}
      />
    </ViewFrame>
  )
}

function AssetCard({ title, description, status }: { title: string, description: string, status: string }) {
  return (
    <article className="rounded-2xl border border-line bg-panel p-5">
      <div className="flex items-center justify-between gap-3">
        <strong className="text-sm text-ink">{title}</strong>
        <Chip size="sm" color="accent">{status}</Chip>
      </div>
      <p className="mt-3 text-xs leading-5 text-muted">{description}</p>
    </article>
  )
}

function knowledgeTabClass(active: boolean): string {
  if (active)
    return 'rounded-lg bg-panel2 text-ink'
  return 'rounded-lg text-muted'
}
