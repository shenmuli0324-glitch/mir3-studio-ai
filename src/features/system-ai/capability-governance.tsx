import type { CapabilityResolution, TaskReceipt, UserCapability } from '@/features/devtools/domain/types'
import { Button } from '@heroui/react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { compileGlobalUserCapability, listTaskReceipts, listUserCapabilityVersions, promoteUserCapability, resolveUserCapabilities, rollbackUserCapability, setSharedUserCapabilityStatus, setUserCapabilityStatus } from '@/features/devtools/domain/api'

export function CapabilityGovernance({ projectId, systemId, refreshToken = '' }: {
  projectId: string
  systemId: string
  refreshToken?: string
}) {
  const { t } = useTranslation()
  const [versions, setVersions] = useState<CapabilityResolution[]>([])
  const [resolved, setResolved] = useState<CapabilityResolution[]>([])
  const [receipts, setReceipts] = useState<TaskReceipt[]>([])
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    void loadCapabilityGovernance(projectId).then((data) => {
      if (cancelled)
        return
      setVersions(data.versions.filter(item => capabilityVisible(item.capability, systemId)))
      setResolved(data.resolved.filter(item => capabilityVisible(item.capability, systemId)))
      setReceipts(data.receipts)
      setError(null)
    }).catch((reason) => {
      if (!cancelled)
        setError(String(reason))
    })
    return () => {
      cancelled = true
    }
  }, [projectId, refreshToken, systemId])

  async function reload() {
    try {
      const [nextVersions, nextResolved, nextReceipts] = await Promise.all([
        listUserCapabilityVersions(projectId),
        resolveUserCapabilities(projectId),
        listTaskReceipts(projectId),
      ])
      setVersions(nextVersions.filter(item => capabilityVisible(item.capability, systemId)))
      setResolved(nextResolved.filter(item => capabilityVisible(item.capability, systemId)))
      setReceipts(nextReceipts)
      setError(null)
    }
    catch (reason) {
      setError(String(reason))
    }
  }

  async function run(action: () => Promise<unknown>) {
    setPending(true)
    try {
      await action()
      await reload()
    }
    catch (reason) {
      setError(String(reason))
    }
    finally {
      setPending(false)
    }
  }

  function changeStatus(item: CapabilityResolution, status: UserCapability['status']) {
    const capability = item.capability
    if (item.resolvedScope === 'project') {
      void run(() => setUserCapabilityStatus(projectId, capability.id, capability.version, status, status === 'active'))
      return
    }
    const scope = item.resolvedScope
    if (!isSharedScope(scope))
      return
    void run(() => setSharedUserCapabilityStatus(scope, capability.id, capability.version, status, status === 'active'))
  }

  function promote(item: CapabilityResolution, targetScope: 'personal' | 'team') {
    void run(() => promoteUserCapability(projectId, item.capability.id, item.capability.version, targetScope))
  }

  function rollback(item: CapabilityResolution) {
    const current = versions.find(candidate => (
      candidate.capability.id === item.capability.id
      && candidate.resolvedScope === item.resolvedScope
      && candidate.capability.status === 'active'
    ))
    if (!current) {
      setError(t('studio.devtools.ai.capability_rollback_unavailable'))
      return
    }
    void run(() => rollbackUserCapability(projectId, {
      capabilityId: item.capability.id,
      scope: item.resolvedScope,
      fromVersion: current.capability.version,
      toVersion: item.capability.version,
    }))
  }

  function compileGlobal() {
    const source = latestAtomicReceiptGroup(receipts)
    if (source.length < 2) {
      setError(t('studio.devtools.ai.capability_global_receipts_required'))
      return
    }
    const createdAt = Date.now()
    void run(() => compileGlobalUserCapability(projectId, {
      receiptIds: source.map(receipt => receipt.id),
      id: `global-workflow-${createdAt}`,
      name: t('studio.devtools.ai.capability_global_name'),
      description: t('studio.devtools.ai.capability_global_description', {
        systems: source.map(receipt => receipt.systemId).join(', '),
      }),
    }))
  }

  const globalReceiptGroup = latestAtomicReceiptGroup(receipts)
  return (
    <details className="rounded-xl border border-line bg-panel2 p-3">
      <summary className="cursor-pointer text-[10px] font-semibold uppercase tracking-wider text-muted">
        {t('studio.devtools.ai.capability_governance')}
      </summary>
      <div className="mt-3 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[10px] text-muted">{t('studio.devtools.ai.capability_resolved_count', { count: resolved.length })}</span>
          <Button size="sm" variant="ghost" isDisabled={globalReceiptGroup.length < 2} isPending={pending} onPress={compileGlobal}>
            {t('studio.devtools.ai.capability_compile_global')}
          </Button>
        </div>
        <If cond={versions.length > 0} else={<p className="text-[10px] text-muted">{t('studio.devtools.ai.capability_empty')}</p>}>
          <div className="max-h-56 space-y-2 overflow-auto">
            {versions.map(item => (
              <CapabilityVersionRow
                key={`${item.resolvedScope}:${item.capability.id}:${item.capability.version}`}
                item={item}
                isResolved={isResolvedVersion(item, resolved)}
                pending={pending}
                onStatus={status => changeStatus(item, status)}
                onPromote={scope => promote(item, scope)}
                onRollback={() => rollback(item)}
              />
            ))}
          </div>
        </If>
        <If cond={error != null}><p className="rounded-lg border border-danger/30 bg-danger/8 p-2 text-[10px] text-danger">{error}</p></If>
      </div>
    </details>
  )
}

function CapabilityVersionRow({ item, isResolved, pending, onStatus, onPromote, onRollback }: {
  item: CapabilityResolution
  isResolved: boolean
  pending: boolean
  onStatus: (status: UserCapability['status']) => void
  onPromote: (scope: 'personal' | 'team') => void
  onRollback: () => void
}) {
  const { t } = useTranslation()
  const capability = item.capability
  return (
    <div className="rounded-lg border border-line bg-panel p-2">
      <div className="flex items-start justify-between gap-2">
        <span>
          <strong className="block text-[10px] text-ink">{capability.name}</strong>
          <small className="font-mono text-[9px] text-muted">{`${capability.id}@${capability.version}`}</small>
        </span>
        <span className="rounded-full border border-line px-2 py-0.5 text-[8px] text-muted">
          {`${item.resolvedScope} · ${capability.status}`}
        </span>
      </div>
      <If cond={isResolved}><p className="mt-1 text-[9px] text-accent">{t('studio.devtools.ai.capability_resolved')}</p></If>
      <div className="mt-2 flex flex-wrap gap-1">
        <If cond={capability.status === 'active'}>
          <Button size="sm" variant="ghost" isDisabled={pending} onPress={() => onStatus('disabled')}>{t('studio.devtools.ai.capability_disable')}</Button>
        </If>
        <If cond={capability.status === 'disabled'}>
          <Button size="sm" variant="ghost" isDisabled={pending} onPress={() => onStatus('active')}>{t('studio.devtools.ai.capability_enable')}</Button>
          <Button size="sm" variant="ghost" isDisabled={pending} onPress={onRollback}>{t('studio.devtools.ai.capability_rollback')}</Button>
        </If>
        <If cond={canPromote(item)}>
          <Button size="sm" variant="ghost" isDisabled={pending} onPress={() => onPromote('personal')}>{t('studio.devtools.ai.capability_promote_personal')}</Button>
          <Button size="sm" variant="ghost" isDisabled={pending} onPress={() => onPromote('team')}>{t('studio.devtools.ai.capability_promote_team')}</Button>
        </If>
      </div>
    </div>
  )
}

function capabilityVisible(capability: UserCapability, systemId: string) {
  return capability.systemId === systemId || capability.systemId === '__global__'
}

function isResolvedVersion(item: CapabilityResolution, resolved: CapabilityResolution[]) {
  return resolved.some(candidate => (
    candidate.capability.id === item.capability.id
    && candidate.capability.version === item.capability.version
    && candidate.resolvedScope === item.resolvedScope
  ))
}

function canPromote(item: CapabilityResolution) {
  return item.resolvedScope === 'project' && item.capability.status === 'active'
}

function isSharedScope(scope: UserCapability['scope']): scope is 'personal' | 'team' {
  return scope === 'personal' || scope === 'team'
}

async function loadCapabilityGovernance(projectId: string) {
  const [versions, resolved, receipts] = await Promise.all([
    listUserCapabilityVersions(projectId),
    resolveUserCapabilities(projectId),
    listTaskReceipts(projectId),
  ])
  return { versions, resolved, receipts }
}

function latestAtomicReceiptGroup(receipts: TaskReceipt[]) {
  const groups = new Map<string, TaskReceipt[]>()
  for (const receipt of receipts) {
    const snapshotId = typeof receipt.evidence.snapshotId === 'string' ? receipt.evidence.snapshotId : ''
    if (receipt.status !== 'applied' || !snapshotId)
      continue
    const group = groups.get(snapshotId) ?? []
    group.push(receipt)
    groups.set(snapshotId, group)
  }
  return [...groups.values()]
    .filter(group => new Set(group.map(receipt => receipt.systemId)).size >= 2)
    .sort((left, right) => Math.max(...right.map(receipt => receipt.createdAt)) - Math.max(...left.map(receipt => receipt.createdAt)))[0] ?? []
}
