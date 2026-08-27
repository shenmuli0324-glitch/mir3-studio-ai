import type { DomainPackState } from '@/features/devtools/domain/types'
import { ArrowRotateRight, CircleCheck, CircleExclamation } from '@gravity-ui/icons'
import { Button, Chip, Spinner } from '@heroui/react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { DEV_TOOLS } from '@/features/devtools/devtool-registry'
import { activateDomainPack, listDomainPacks, rollbackDomainPack, setDomainPackEnabled } from '@/features/devtools/domain/api'
import { toast } from '@/utils'

export function DomainPackManager() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const packs = useQuery({ queryKey: ['domain-packs'], queryFn: listDomainPacks })
  const transition = useMutation({
    mutationFn: runTransition,
    onSuccess: async (state) => {
      await queryClient.invalidateQueries({ queryKey: ['domain-packs'] })
      toast(t('plugins.domain_action_done', { system: domainTitle(t, state.systemId) }), {})
    },
    onError: reason => toast(String(reason), { variant: 'danger' }),
  })

  function runPackAction(action: DomainPackAction) {
    if (transition.isPending)
      return
    transition.mutate(action)
  }

  return (
    <section className="mb-6 overflow-hidden rounded-xl border border-line bg-panel">
      <header className="flex items-center gap-3 border-b border-line px-4 py-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-accent/12 text-accent">
          <CircleCheck className="size-4" />
        </span>
        <span className="min-w-0 flex-1">
          <strong className="block text-sm text-ink">{t('plugins.domain_title')}</strong>
          <small className="block text-xs text-muted">{t('plugins.domain_description', { count: packs.data?.length ?? 33 })}</small>
        </span>
        <Button isIconOnly size="sm" variant="ghost" aria-label={t('plugins.domain_refresh')} onPress={() => void packs.refetch()}>
          <ArrowRotateRight className="size-4" />
        </Button>
      </header>
      <If cond={!packs.isLoading} else={<div className="grid h-24 place-items-center"><Spinner size="sm" /></div>}>
        <If cond={packs.error == null} else={<p className="px-4 py-5 text-xs text-danger">{String(packs.error)}</p>}>
          <div className="max-h-[480px] divide-y divide-line overflow-auto">
            {(packs.data ?? []).map(pack => (
              <DomainPackRow
                key={pack.systemId}
                pack={pack}
                busy={transition.isPending}
                onAction={runPackAction}
              />
            ))}
          </div>
        </If>
      </If>
    </section>
  )
}

type DomainPackAction
  = | { kind: 'activate', pack: DomainPackState }
    | { kind: 'rollback', pack: DomainPackState }
    | { kind: 'toggle', pack: DomainPackState }

async function runTransition(action: DomainPackAction) {
  if (action.kind === 'activate') {
    const candidate = action.pack.candidate
    if (!candidate)
      throw new Error('DOMAIN_PACK_CANDIDATE_MISSING')
    return activateDomainPack(action.pack.systemId, candidate.version, candidate.hash)
  }
  if (action.kind === 'rollback')
    return rollbackDomainPack(action.pack.systemId)
  return setDomainPackEnabled(action.pack.systemId, !action.pack.enabled)
}

function DomainPackRow({ pack, busy, onAction }: { pack: DomainPackState, busy: boolean, onAction: (action: DomainPackAction) => void }) {
  const { t } = useTranslation()
  return (
    <div className="flex min-h-14 items-center gap-3 px-4 py-2.5">
      <If cond={pack.enabled} then={<CircleCheck className="size-4 shrink-0 text-success" />} else={<CircleExclamation className="size-4 shrink-0 text-muted" />} />
      <span className="min-w-0 flex-1">
        <strong className="block truncate text-xs font-medium text-ink">{domainTitle(t, pack.systemId)}</strong>
        <small className="block truncate font-mono text-[10px] text-muted">
          {pack.systemId}
          {' · '}
          {t('plugins.domain_current')}
          {' '}
          {pack.current?.version ?? '—'}
          {' · LKG '}
          {pack.lkg?.version ?? '—'}
        </small>
      </span>
      <If cond={pack.candidate != null}>
        <Chip size="sm" color="accent" variant="soft">{t('plugins.domain_candidate', { version: pack.candidate?.version })}</Chip>
      </If>
      <If cond={pack.candidate != null}>
        <Button size="sm" variant="ghost" isDisabled={busy} onPress={() => onAction({ kind: 'activate', pack })}>{t('plugins.domain_activate')}</Button>
      </If>
      <If cond={pack.previous != null}>
        <Button size="sm" variant="ghost" isDisabled={busy} onPress={() => onAction({ kind: 'rollback', pack })}>{t('plugins.domain_rollback')}</Button>
      </If>
      <Button size="sm" variant="ghost" className={pack.enabled ? 'text-danger' : 'text-accent'} isDisabled={busy} onPress={() => onAction({ kind: 'toggle', pack })}>
        {t(pack.enabled ? 'plugins.domain_disable' : 'plugins.domain_enable')}
      </Button>
    </div>
  )
}

function domainTitle(t: ReturnType<typeof useTranslation>['t'], systemId: string) {
  if (DEV_TOOLS.some(tool => tool.id === systemId))
    return t(`studio.devtools.tool.${systemId}.title`)
  return systemId
}
