import type { ReactNode, PointerEvent as ReactPointerEvent } from 'react'
import type { DevToolDefinition } from '../devtool-registry'
import { ArrowLeft } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { devToolTitleKey } from '../devtool-registry'

export function DevToolWorkspace({ tool, onBack, sidebar, toolbar, rightPanel, children }: {
  tool: DevToolDefinition
  onBack: () => void
  sidebar: ReactNode
  toolbar: ReactNode
  rightPanel?: ReactNode
  children: ReactNode
}) {
  const { t } = useTranslation()
  const Icon = tool.icon
  const [mobilePanel, setMobilePanel] = useState<'resources' | 'ai' | null>(null)
  const [leftWidth, setLeftWidth] = useState(280)
  const [rightWidth, setRightWidth] = useState(360)
  const [leftCollapsed, setLeftCollapsed] = useState(false)
  const [rightCollapsed, setRightCollapsed] = useState(false)

  function toggleMobilePanel(panel: 'resources' | 'ai') {
    setMobilePanel((value) => {
      if (value === panel)
        return null
      return panel
    })
  }

  function beginResize(side: 'left' | 'right', event: ReactPointerEvent<HTMLButtonElement>) {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = resizeStartWidth(side, leftWidth, rightWidth)
    function handleMove(moveEvent: PointerEvent) {
      const delta = resizeDelta(side, startX, moveEvent.clientX)
      const next = Math.min(520, Math.max(220, startWidth + delta))
      if (side === 'left')
        setLeftWidth(next)
      else
        setRightWidth(next)
    }
    function handleUp() {
      window.removeEventListener('pointermove', handleMove)
      window.removeEventListener('pointerup', handleUp)
    }
    window.addEventListener('pointermove', handleMove)
    window.addEventListener('pointerup', handleUp)
  }

  return (
    <div className="flex h-full min-h-0 bg-canvas">
      <aside className={resourcePanelClass(mobilePanel, leftCollapsed)} style={{ width: leftWidth }}>
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
        <button type="button" className="absolute inset-y-0 right-0 z-10 w-1 cursor-col-resize bg-transparent hover:bg-accent/40 max-[900px]:hidden" aria-label={t('studio.devtools.resize.resources')} onPointerDown={event => beginResize('left', event)} />
      </aside>
      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="relative flex min-h-14 shrink-0 items-center border-b border-line bg-panel px-4">
          <div className="mr-2 hidden shrink-0 gap-1 max-[900px]:flex">
            <Button size="sm" variant="ghost" onPress={() => toggleMobilePanel('resources')}>{t('studio.devtools.mobile.resources')}</Button>
            <Button size="sm" variant="ghost" onPress={() => toggleMobilePanel('ai')}>{t('studio.devtools.mobile.ai')}</Button>
          </div>
          <div className="mr-2 flex shrink-0 gap-1 max-[900px]:hidden">
            <Button size="sm" variant="ghost" aria-label={t(resourceToggleKey(leftCollapsed))} onPress={() => setLeftCollapsed(value => !value)}>{t('studio.devtools.mobile.resources')}</Button>
            <Button size="sm" variant="ghost" aria-label={t(aiToggleKey(rightCollapsed))} onPress={() => setRightCollapsed(value => !value)}>{t('studio.devtools.mobile.ai')}</Button>
          </div>
          {toolbar}
        </header>
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden bg-canvas">{children}</div>
      </section>
      <div className={aiPanelClass(mobilePanel, rightCollapsed)} style={{ width: rightWidth }}>
        <button type="button" className="absolute inset-y-0 left-0 z-30 w-1 cursor-col-resize bg-transparent hover:bg-accent/40 max-[900px]:hidden" aria-label={t('studio.devtools.resize.ai')} onPointerDown={event => beginResize('right', event)} />
        {rightPanel}
      </div>
    </div>
  )
}

function resizeStartWidth(side: 'left' | 'right', leftWidth: number, rightWidth: number) {
  if (side === 'left')
    return leftWidth
  return rightWidth
}

function resizeDelta(side: 'left' | 'right', startX: number, currentX: number) {
  if (side === 'left')
    return currentX - startX
  return startX - currentX
}

function resourcePanelClass(panel: 'resources' | 'ai' | null, collapsed: boolean) {
  if (panel === 'resources')
    return 'relative flex min-h-0 shrink-0 flex-col border-r border-line bg-panel max-[900px]:absolute max-[900px]:inset-y-0 max-[900px]:left-0 max-[900px]:z-20 max-[900px]:w-[280px] max-[900px]:shadow-2xl'
  if (collapsed)
    return 'hidden max-[900px]:hidden'
  return 'relative flex min-h-0 shrink-0 flex-col border-r border-line bg-panel max-[900px]:hidden'
}

function aiPanelClass(panel: 'resources' | 'ai' | null, collapsed: boolean) {
  if (panel === 'ai')
    return 'relative h-full shrink-0 max-[900px]:absolute max-[900px]:inset-y-0 max-[900px]:right-0 max-[900px]:z-20 max-[900px]:max-w-[85vw] max-[900px]:shadow-2xl'
  if (collapsed)
    return 'hidden max-[900px]:hidden'
  return 'relative h-full shrink-0 max-[900px]:hidden'
}

function resourceToggleKey(collapsed: boolean) {
  if (collapsed)
    return 'studio.devtools.panels.resources_expand'
  return 'studio.devtools.panels.resources_collapse'
}

function aiToggleKey(collapsed: boolean) {
  if (collapsed)
    return 'studio.devtools.panels.ai_expand'
  return 'studio.devtools.panels.ai_collapse'
}
