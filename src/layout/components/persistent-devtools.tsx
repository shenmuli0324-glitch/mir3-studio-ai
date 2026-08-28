import type { VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { lazy, Suspense } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'

const DevToolsView = lazy(() => import('@/views/devtools-view').then(module => ({ default: module.DevToolsView })))

export function PersistentDevTools({ mounted, active, target }: {
  mounted: boolean
  active: boolean
  target?: VerifiedDevtoolsTarget | null
}) {
  const { t } = useTranslation()
  return (
    <If cond={mounted}>
      <div className={persistentDevtoolsClass(active)} aria-hidden={!active}>
        <Suspense fallback={<div className="grid h-full place-items-center text-xs text-muted" role="status">{t('studio.devtools.loading')}</div>}>
          <DevToolsView target={target} />
        </Suspense>
      </div>
    </If>
  )
}

function persistentDevtoolsClass(active: boolean): string {
  const base = 'absolute inset-0 min-h-0 min-w-0'
  if (active)
    return `${base} visible`
  return `${base} invisible pointer-events-none`
}
