import type { ReactNode } from 'react'
import type { DevToolDefinition } from '../devtool-registry'
import { ArrowLeft } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { devToolTitleKey } from '../devtool-registry'

export function DevToolWorkspace({ tool, onBack, sidebar, toolbar, children }: {
  tool: DevToolDefinition
  onBack: () => void
  sidebar: ReactNode
  toolbar: ReactNode
  children: ReactNode
}) {
  const { t } = useTranslation()
  const Icon = tool.icon

  return (
    <div className="flex h-full min-h-0 bg-canvas">
      <aside className="flex w-[272px] min-h-0 shrink-0 flex-col border-r border-line bg-panel max-[880px]:w-[232px]">
        <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line px-3">
          <Button
            className="size-8 shrink-0 rounded-lg"
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={t('studio.devtools.back')}
            onPress={onBack}
          >
            <ArrowLeft className="size-4" />
          </Button>
          <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-accent/14 text-accent"><Icon className="size-4" /></span>
          <span className="min-w-0">
            <strong className="block truncate text-xs font-semibold text-ink">{t(devToolTitleKey(tool.id))}</strong>
            <small className="mt-0.5 block truncate text-[10px] text-muted">{t('studio.devtools.workspace')}</small>
          </span>
        </header>
        <div className="min-h-0 flex-1">{sidebar}</div>
      </aside>
      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex min-h-14 shrink-0 items-center border-b border-line bg-panel px-4">{toolbar}</header>
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden bg-canvas">{children}</div>
      </section>
    </div>
  )
}
