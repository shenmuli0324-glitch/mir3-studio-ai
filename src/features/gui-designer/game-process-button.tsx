import type { GuiGameProcessStatus } from './types'
import { ArrowRotateRight } from '@gravity-ui/icons'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'

const GAME_STATUS_POLL_INTERVAL = 2_000

export function GameProcessButton() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const projectId = scope.activeProject?.id
  const [status, setStatus] = useState<GuiGameProcessStatus | null>(null)
  const [checking, setChecking] = useState(false)
  const [failed, setFailed] = useState(false)
  const requestSequenceRef = useRef(0)
  const scopeRef = useRef(scope)
  scopeRef.current = scope

  useEffect(() => {
    if (!projectId)
      return
    let cancelled = false

    async function detect() {
      const sequence = ++requestSequenceRef.current
      setChecking(true)
      try {
        const next = await scopeRef.current.gameProcessStatus()
        if (!cancelled && sequence === requestSequenceRef.current) {
          setStatus(next)
          setFailed(false)
        }
      }
      catch {
        if (!cancelled && sequence === requestSequenceRef.current)
          setFailed(true)
      }
      finally {
        if (!cancelled && sequence === requestSequenceRef.current)
          setChecking(false)
      }
    }

    void detect()
    const timer = window.setInterval(() => void detect(), GAME_STATUS_POLL_INTERVAL)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [projectId])

  function detectNow() {
    const sequence = ++requestSequenceRef.current
    setChecking(true)
    void scopeRef.current.gameProcessStatus().then((next) => {
      if (sequence !== requestSequenceRef.current)
        return
      setStatus(next)
      setFailed(false)
    }).catch(() => {
      if (sequence === requestSequenceRef.current)
        setFailed(true)
    }).finally(() => {
      if (sequence === requestSequenceRef.current)
        setChecking(false)
    })
  }

  const label = t(gameStatusKey(status, checking, failed))
  const title = gameStatusTitle(label, status, t('studio.gui.game.refresh_hint'))
  return (
    <button
      className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-panel-2 px-2.5 text-[11px] text-muted ring-1 ring-line transition-colors hover:bg-panel-hover hover:text-ink"
      type="button"
      title={title}
      aria-label={t('studio.gui.game.refresh')}
      onClick={detectNow}
    >
      <span className={gameStatusClass(status, checking, failed)} />
      <span>{label}</span>
      <ArrowRotateRight className={refreshIconClass(checking)} />
    </button>
  )
}

function gameStatusTitle(label: string, status: GuiGameProcessStatus | null, refreshHint: string): string {
  if (status?.executablePath)
    return `${label}\n${status.executablePath}\n${refreshHint}`
  return `${label}\n${refreshHint}`
}

function refreshIconClass(checking: boolean): string {
  if (checking)
    return 'size-3 animate-spin'
  return 'size-3 opacity-60'
}

function gameStatusKey(status: GuiGameProcessStatus | null, checking: boolean, failed: boolean): string {
  if (checking && status == null)
    return 'studio.gui.game.detecting'
  if (failed)
    return 'studio.gui.game.detect_failed'
  if (!status?.supported)
    return 'studio.gui.game.unsupported'
  if (!status.configured)
    return 'studio.gui.game.not_configured'
  if (status.running)
    return 'studio.gui.game.running'
  return 'studio.gui.game.stopped'
}

function gameStatusClass(status: GuiGameProcessStatus | null, checking: boolean, failed: boolean): string {
  const base = 'size-1.5 shrink-0 rounded-full'
  if (failed)
    return `${base} bg-danger`
  if (checking && status == null)
    return `${base} bg-muted`
  if (status?.running)
    return `${base} bg-success shadow-[0_0_8px_var(--color-success)]`
  if (status?.configured)
    return `${base} bg-warning`
  return `${base} bg-muted`
}
