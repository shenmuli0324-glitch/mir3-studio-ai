import type { StudioView } from '../studio-types'
import {
  Boxes3,
  FileCheck,
  FileText,
  FolderCode,
  Gear,
  PencilToSquare,
  PlugConnection,
  Sparkles,
  Wrench,
} from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'

const NAV_ITEMS = [
  { id: 'project', icon: FolderCode },
  { id: 'gui-designer', icon: PencilToSquare },
  { id: 'workbench', icon: Sparkles },
  { id: 'devtools', icon: Wrench },
  { id: 'builds', icon: Boxes3 },
  { id: 'runtime', icon: PlugConnection },
  { id: 'feedback', icon: FileCheck },
  { id: 'settings', icon: Gear },
  { id: 'logs', icon: FileText },
] as const

export function StudioSidebar({ activeView, collapsed, guiDirty = false, onNavigate }: {
  activeView: StudioView
  collapsed: boolean
  guiDirty?: boolean
  onNavigate: (view: StudioView) => void
}) {
  const { t } = useTranslation()
  return (
    <aside className={sidebarClass(collapsed)}>
      <div className="flex h-16 shrink-0 items-center gap-3 border-b border-line px-4">
        <img className="size-9 shrink-0 rounded-xl" src="/brand/mir3-studio-ai.svg" alt={t('app.title')} />
        <div className={sidebarLabelClass(collapsed)}>
          <strong className="block whitespace-nowrap text-sm font-semibold text-ink">{t('app.title')}</strong>
          <small className="mt-0.5 block whitespace-nowrap text-[10px] uppercase tracking-[0.16em] text-muted">{t('studio.shell.subtitle')}</small>
        </div>
      </div>
      <nav className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto p-2" aria-label={t('studio.nav.label')}>
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon
          const active = activeView === item.id
          return (
            <Button
              className={navItemClass(active, collapsed)}
              variant="ghost"
              size="sm"
              onPress={() => onNavigate(item.id)}
              aria-current={active ? 'page' : undefined}
              aria-label={t(`studio.nav.${item.id}`)}
              key={item.id}
            >
              <Icon className="size-[18px] shrink-0" />
              <If cond={item.id === 'gui-designer' && guiDirty}>
                <span className="absolute ml-3 mt-[-14px] size-1.5 rounded-full bg-accent ring-2 ring-panel" aria-label={t('studio.gui.dirty')} />
              </If>
              <span className={sidebarLabelClass(collapsed)}>
                <strong className="block whitespace-nowrap text-left text-xs font-medium">{t(`studio.nav.${item.id}`)}</strong>
                <small className="mt-0.5 block whitespace-nowrap text-left text-[10px] text-muted">{t(`studio.nav.${item.id}_hint`)}</small>
              </span>
            </Button>
          )
        })}
      </nav>
      <div className="shrink-0 border-t border-line p-3">
        <div className="flex items-center justify-center gap-2 rounded-lg border border-line bg-canvas px-2 py-2 text-xs text-muted">
          <span className="size-1.5 shrink-0 rounded-full bg-ok" />
          <If cond={!collapsed}>
            <span className="max-[1040px]:hidden">{t('studio.shell.core_managed')}</span>
          </If>
        </div>
      </div>
    </aside>
  )
}

function sidebarClass(collapsed: boolean): string {
  const base = 'flex min-h-0 shrink-0 flex-col border-r border-line bg-panel transition-[width] duration-200'
  if (collapsed)
    return `${base} w-[72px]`
  return `${base} w-56 max-[1040px]:w-[72px]`
}

function sidebarLabelClass(collapsed: boolean): string {
  if (collapsed)
    return 'hidden'
  return 'min-w-0 max-[1040px]:hidden'
}

function navItemClass(active: boolean, collapsed: boolean): string {
  const alignment = collapsed ? 'justify-center px-0' : 'justify-start px-3 max-[1040px]:justify-center max-[1040px]:px-0'
  const tone = active ? 'bg-accent/12 text-accent' : 'text-muted hover:bg-panel-hover hover:text-ink'
  return `relative h-12 w-full min-w-0 gap-3 rounded-xl ${alignment} ${tone}`
}
