import type { ReactNode } from 'react'
import type { DevToolDefinition } from '../devtool-registry'
import { ArrowsRotateRight, FolderOpen, Geo, Layers, Magnifier, Plus, Sliders, Wrench } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { DevToolWorkspace } from '../shell/devtool-workspace'

export function MapToolView({ tool, onBack, hasProject }: {
  tool: DevToolDefinition
  onBack: () => void
  hasProject: boolean
}) {
  return (
    <DevToolWorkspace
      tool={tool}
      onBack={onBack}
      sidebar={<MapListPanel hasProject={hasProject} />}
      toolbar={<MapToolbar />}
    >
      <MapContent hasProject={hasProject} />
    </DevToolWorkspace>
  )
}

function MapListPanel({ hasProject }: { hasProject: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-line p-3">
        <div className="mb-3 flex items-center justify-between gap-3 px-1">
          <strong className="text-xs font-semibold text-ink">{t('studio.devtools.map.list')}</strong>
          <span className="rounded-full border border-line bg-panel2 px-2 py-0.5 text-[10px] tabular-nums text-muted">0</span>
        </div>
        <label className="flex h-9 items-center gap-2 rounded-lg border border-line bg-panel2 px-3 text-muted">
          <Magnifier className="size-3.5" />
          <input
            className="min-w-0 flex-1 bg-transparent text-xs text-ink outline-none placeholder:text-muted"
            placeholder={t('studio.devtools.map.search')}
            aria-label={t('studio.devtools.map.search')}
            disabled
          />
        </label>
      </div>
      <div className="grid min-h-0 flex-1 place-items-center overflow-y-auto p-5 text-center">
        <If
          cond={hasProject}
          else={(
            <div>
              <FolderOpen className="mx-auto size-6 text-muted" />
              <strong className="mt-3 block text-xs font-medium text-ink">{t('studio.devtools.map.no_project')}</strong>
              <p className="mt-1 text-[11px] leading-5 text-muted">{t('studio.devtools.map.no_project_desc')}</p>
            </div>
          )}
        >
          <div>
            <Geo className="mx-auto size-6 text-muted" />
            <strong className="mt-3 block text-xs font-medium text-ink">{t('studio.devtools.map.list_pending')}</strong>
            <p className="mt-1 text-[11px] leading-5 text-muted">{t('studio.devtools.map.list_pending_desc')}</p>
          </div>
        </If>
      </div>
      <div className="shrink-0 border-t border-line p-3">
        <Button className="w-full rounded-lg border border-line text-muted disabled:text-muted" size="sm" variant="ghost" isDisabled>
          <Plus className="size-3.5" />
          {t('studio.devtools.map.add')}
        </Button>
      </div>
    </div>
  )
}

function MapToolbar() {
  const { t } = useTranslation()
  return (
    <div className="flex w-full items-center justify-between gap-4">
      <span className="min-w-0">
        <strong className="block truncate text-sm font-semibold text-ink">{t('studio.devtools.map.title')}</strong>
        <small className="mt-0.5 block truncate text-[11px] text-muted">{t('studio.devtools.map.toolbar_desc')}</small>
      </span>
      <div className="flex shrink-0 items-center gap-1">
        <ToolButton icon={<ArrowsRotateRight />} label={t('studio.devtools.map.refresh')} />
        <ToolButton icon={<Layers />} label={t('studio.devtools.map.layers')} />
        <ToolButton icon={<Sliders />} label={t('studio.devtools.map.properties')} />
      </div>
    </div>
  )
}

function ToolButton({ icon, label }: { icon: ReactNode, label: string }) {
  return (
    <Button className="h-8 rounded-lg px-2 text-xs text-muted disabled:text-muted" size="sm" variant="ghost" isDisabled>
      <span className="size-3.5">{icon}</span>
      <span className="max-[980px]:hidden">{label}</span>
    </Button>
  )
}

function MapContent({ hasProject }: { hasProject: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="relative grid h-full place-items-center overflow-hidden bg-panel2/30 p-6">
      <div className="flex max-w-md flex-col items-center text-center">
        <span className="grid size-16 place-items-center rounded-2xl border border-line bg-panel text-accent"><Geo className="size-8" /></span>
        <If
          cond={hasProject}
          then={(
            <>
              <h2 className="mt-5 text-lg font-semibold text-ink">{t('studio.devtools.map.canvas_pending')}</h2>
              <p className="mt-2 text-sm leading-6 text-muted">{t('studio.devtools.map.canvas_pending_desc')}</p>
            </>
          )}
          else={(
            <>
              <h2 className="mt-5 text-lg font-semibold text-ink">{t('studio.devtools.map.open_project')}</h2>
              <p className="mt-2 text-sm leading-6 text-muted">{t('studio.devtools.map.open_project_desc')}</p>
            </>
          )}
        />
        <span className="mt-5 inline-flex items-center gap-2 rounded-full border border-line bg-panel px-3 py-1.5 text-xs text-muted">
          <Wrench className="size-3.5" />
          {t('studio.devtools.status.developing')}
        </span>
      </div>
    </div>
  )
}
