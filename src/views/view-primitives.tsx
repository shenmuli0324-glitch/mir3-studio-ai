import type { ReactNode } from 'react'
import { CircleInfo, Clock, Lock, ShieldCheck } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'

export function ViewFrame({ children }: { children: ReactNode }) {
  return (
    <div className="h-full overflow-y-auto bg-canvas">
      <div className="mx-auto flex w-full max-w-[1180px] flex-col gap-5 p-6 max-[900px]:p-4">
        {children}
      </div>
    </div>
  )
}

export function ViewHeader({ eyebrow, title, description, action }: {
  eyebrow: string
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <header className="flex items-start justify-between gap-6 border-b border-line pb-5">
      <div className="min-w-0">
        <span className="text-[11px] font-semibold uppercase tracking-[0.18em] text-accent">{eyebrow}</span>
        <h1 className="mt-2 text-2xl font-semibold tracking-tight text-ink">{title}</h1>
        <p className="mt-2 max-w-3xl text-sm leading-6 text-muted">{description}</p>
      </div>
      {action}
    </header>
  )
}

export function PhaseNotice() {
  const { t } = useTranslation()
  return (
    <div className="flex items-start gap-3 rounded-xl border border-accent/25 bg-accent/8 px-4 py-3 text-sm text-ink">
      <CircleInfo className="mt-0.5 size-4 shrink-0 text-accent" />
      <div>
        <strong className="font-medium">{t('studio.placeholder.title')}</strong>
        <p className="mt-0.5 leading-5 text-muted">{t('studio.placeholder.description')}</p>
      </div>
    </div>
  )
}

export function DisabledAction({ children }: { children: ReactNode }) {
  return (
    <Button className="rounded-lg" size="sm" variant="primary" isDisabled>
      {children}
    </Button>
  )
}

export function EmptyPanel({ icon, title, description }: {
  icon: ReactNode
  title: string
  description: string
}) {
  return (
    <section className="flex min-h-52 flex-col items-center justify-center rounded-2xl border border-line bg-panel px-6 py-10 text-center">
      <span className="mb-4 grid size-11 place-items-center rounded-xl border border-line bg-panel2 text-muted">{icon}</span>
      <strong className="text-base font-semibold text-ink">{title}</strong>
      <p className="mt-2 max-w-lg text-sm leading-6 text-muted">{description}</p>
    </section>
  )
}

export function PrincipleStrip() {
  const { t } = useTranslation()
  const items = [
    { icon: Lock, title: t('studio.project.guard.source'), description: t('studio.project.guard.source_desc') },
    { icon: ShieldCheck, title: t('studio.project.guard.draft'), description: t('studio.project.guard.draft_desc') },
    { icon: Clock, title: t('studio.project.guard.acceptance'), description: t('studio.project.guard.acceptance_desc') },
  ]
  return (
    <section className="grid grid-cols-3 overflow-hidden rounded-2xl border border-line bg-panel max-[820px]:grid-cols-1">
      {items.map((item) => {
        const Icon = item.icon
        return (
          <div className="flex items-start gap-3 border-r border-line p-4 last:border-r-0 max-[820px]:border-r-0 max-[820px]:border-b max-[820px]:last:border-b-0" key={item.title}>
            <Icon className="mt-0.5 size-4 shrink-0 text-accent" />
            <span>
              <strong className="block text-sm font-medium text-ink">{item.title}</strong>
              <small className="mt-1 block text-xs leading-5 text-muted">{item.description}</small>
            </span>
          </div>
        )
      })}
    </section>
  )
}

export function SectionPanel({ title, description, children }: {
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <section className="overflow-hidden rounded-2xl border border-line bg-panel">
      <header className="border-b border-line px-5 py-4">
        <strong className="text-sm font-semibold text-ink">{title}</strong>
        <p className="mt-1 text-xs leading-5 text-muted">{description}</p>
      </header>
      {children}
    </section>
  )
}
