import type { DevToolDefinition, DevToolId } from '../devtool-registry'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { devToolDescriptionKey, devToolTitleKey } from '../devtool-registry'

export function DevToolCard({ tool, onOpen }: {
  tool: DevToolDefinition
  onOpen: (id: DevToolId) => void
}) {
  const { t } = useTranslation()
  const Icon = tool.icon

  return (
    <Button
      className="group h-auto min-h-40 w-full min-w-0 flex-col items-start justify-between gap-5 rounded-2xl border border-line bg-panel p-5 text-left text-ink shadow-none transition-colors hover:border-accent/45 hover:bg-panel-hover"
      variant="ghost"
      onPress={() => onOpen(tool.id as DevToolId)}
      aria-label={t('studio.devtools.open_tool', { name: t(devToolTitleKey(tool.id)) })}
    >
      <span className="flex w-full items-start justify-between gap-3">
        <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent/14 text-accent transition-colors group-hover:bg-accent group-hover:text-background">
          <Icon className="size-6" />
        </span>
        <span className={statusClass(tool.status)}>{t(`studio.devtools.status.${tool.status}`)}</span>
      </span>
      <span className="block min-w-0">
        <strong className="block text-sm font-semibold text-ink">
          <span className="mr-2 font-mono text-[11px] text-muted">{String(tool.order).padStart(2, '0')}</span>
          {t(devToolTitleKey(tool.id))}
        </strong>
        <small className="mt-1.5 line-clamp-2 block text-xs leading-5 text-muted">{t(devToolDescriptionKey(tool.id))}</small>
      </span>
    </Button>
  )
}

function statusClass(status: DevToolDefinition['status']): string {
  const base = 'rounded-full border px-2 py-1 text-[10px] font-medium'
  if (status === 'ready')
    return `${base} border-accent/30 bg-accent/10 text-accent`
  return `${base} border-line bg-panel2 text-muted`
}
