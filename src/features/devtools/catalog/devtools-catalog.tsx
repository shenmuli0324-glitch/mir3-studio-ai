import type { DevToolCategory, DevToolId } from '../devtool-registry'
import { Magnifier, Wrench } from '@gravity-ui/icons'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { EmptyPanel, ViewFrame, ViewHeader } from '@/views/view-primitives'
import {
  DEV_TOOL_CATEGORIES,
  DEV_TOOLS,
  devToolCategoryKey,
  devToolDescriptionKey,
  devToolTitleKey,
} from '../devtool-registry'
import { DevToolCard } from './devtool-card'

type CategoryFilter = DevToolCategory | 'all'

export function DevToolsCatalog({ onOpenTool }: { onOpenTool: (id: DevToolId) => void }) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<CategoryFilter>('all')
  const normalizedQuery = query.trim().toLocaleLowerCase()
  const visibleTools = DEV_TOOLS.filter((tool) => {
    const categoryMatches = category === 'all' || tool.category === category
    const title = t(devToolTitleKey(tool.id)).toLocaleLowerCase()
    const description = t(devToolDescriptionKey(tool.id)).toLocaleLowerCase()
    return categoryMatches && (normalizedQuery.length === 0 || title.includes(normalizedQuery) || description.includes(normalizedQuery))
  })

  return (
    <ViewFrame>
      <ViewHeader
        eyebrow={t('studio.devtools.eyebrow')}
        title={t('studio.devtools.title')}
        description={t('studio.devtools.description')}
      />
      <section className="flex items-center gap-3 rounded-2xl border border-line bg-panel p-3 max-[760px]:items-stretch max-[760px]:flex-col">
        <label className="flex h-10 min-w-0 flex-1 items-center gap-2 rounded-xl border border-line bg-panel2 px-3 text-muted focus-within:border-accent/45">
          <Magnifier className="size-4 shrink-0" />
          <input
            className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-muted"
            value={query}
            onChange={event => setQuery(event.target.value)}
            placeholder={t('studio.devtools.search')}
            aria-label={t('studio.devtools.search')}
          />
        </label>
        <select
          className="h-10 min-w-44 rounded-xl border border-line bg-panel2 px-3 text-sm text-ink outline-none focus:border-accent/45"
          value={category}
          onChange={event => setCategory(event.target.value as CategoryFilter)}
          aria-label={t('studio.devtools.filter')}
        >
          <option value="all">{t('studio.devtools.category.all')}</option>
          {DEV_TOOL_CATEGORIES.map(item => <option value={item} key={item}>{t(devToolCategoryKey(item))}</option>)}
        </select>
      </section>
      <If
        cond={visibleTools.length > 0}
        else={(
          <EmptyPanel
            icon={<Wrench className="size-5" />}
            title={t('studio.devtools.no_results')}
            description={t('studio.devtools.no_results_desc')}
          />
        )}
      >
        <div className="flex flex-col gap-7">
          {DEV_TOOL_CATEGORIES.map((categoryId) => {
            const tools = visibleTools.filter(tool => tool.category === categoryId)
            return (
              <If cond={tools.length > 0} key={categoryId}>
                <section>
                  <div className="mb-3 flex items-end justify-between gap-4 border-b border-line pb-3">
                    <h2 className="text-sm font-semibold text-ink">{t(devToolCategoryKey(categoryId))}</h2>
                    <span className="text-[11px] tabular-nums text-muted">{t('studio.devtools.system_count', { count: tools.length })}</span>
                  </div>
                  <div className="grid grid-cols-4 gap-4 max-[1120px]:grid-cols-3 max-[860px]:grid-cols-2 max-[620px]:grid-cols-1">
                    {tools.map(tool => <DevToolCard tool={tool} onOpen={onOpenTool} key={tool.id} />)}
                  </div>
                </section>
              </If>
            )
          })}
        </div>
      </If>
    </ViewFrame>
  )
}
