import type { DomainWorkbookCellEdit } from '@/features/devtools/domain/domain-workbook-model'
import type { DomainFileRecord, DomainWorkbookData, DomainWorkingCopy } from '@/features/devtools/domain/types'
import type { Mir3Project } from '@/features/projects/types'
import { Button } from '@heroui/react'
import { useOverlay } from '@overlastic/react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { Modal as ConfirmationModal } from '@/components/modal'
import { listDomainSaveNodes, patchDomainWorkingXls, previewDomainWorkingCopy, restoreDomainSaveNode, saveDomainWorkingCopy } from '@/features/devtools/domain/api'
import { isSaveConfirmationRequired } from '@/features/devtools/domain/domain-save-policy'
import { DomainWorkbookEditor } from '@/features/devtools/domain/domain-workbook-editor'
import { xlsCellKey } from '@/features/devtools/domain/domain-workbook-model'
import { subscribeHarnessBridge } from '@/features/projects/workspace-bridge'

interface WorkbookOpen {
  data: DomainWorkbookData
  workingCopy: DomainWorkingCopy | null
}

export function WorkbenchWorkbookHost({ project }: { project: Mir3Project | null }) {
  const [target, setTarget] = useState<{ projectId: string, path: string } | null>(null)
  useEffect(() => subscribeHarnessBridge((message) => {
    if (message.type !== 'mir3/workbook.open' || message.projectId !== project?.id)
      return
    const payload = message.payload as { path?: unknown }
    if (typeof payload.path === 'string')
      setTarget(previous => previous?.projectId === message.projectId ? previous : { projectId: message.projectId, path: payload.path as string })
  }), [project?.id])
  if (!target || target.projectId !== project?.id)
    return null
  return <WorkbookDialog key={`${target.projectId}:${target.path}`} projectId={target.projectId} path={target.path} onClose={() => setTarget(null)} />
}

function WorkbookDialog({ projectId, path, onClose }: { projectId: string, path: string, onClose: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [confirmationHolder, openConfirmation] = useOverlay(ConfirmationModal, { type: 'holder' })
  const [sheet, setSheet] = useState('')
  const [edits, setEdits] = useState<Record<string, DomainWorkbookCellEdit>>({})
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState('')
  const opened = useQuery({
    queryKey: ['workbench-workbook', projectId, path],
    queryFn: () => invoke<WorkbookOpen>('workbench_xls_open', { projectId, relativePath: path }),
    staleTime: 0,
    refetchOnWindowFocus: false,
  })
  const nodes = useQuery({
    queryKey: ['workbench-workbook-nodes', projectId, path],
    queryFn: () => listDomainSaveNodes(projectId),
  })
  const lastSave = nodes.data?.find(node => node.files.some(file => file.path === path))
  const file: DomainFileRecord = {
    path,
    role: 'engine',
    category: 'Other',
    extension: 'xls',
    size: 0,
    modifiedAt: 0,
    resourceId: path,
    ownership: 'unknown',
    access: opened.data?.workingCopy ? 'structured' : 'readonly',
    systems: [],
  }

  async function save() {
    const snapshot = opened.data
    const copy = snapshot?.workingCopy
    if (!snapshot || !copy || busy)
      return
    setBusy(true)
    setFailure('')
    try {
      let revision = snapshot.data.revision
      if (Object.keys(edits).length) {
        const patched = await patchDomainWorkingXls(projectId, copy.id, path, revision, snapshot.data.workbook.sha256, Object.values(edits))
        revision = patched.revision
        setEdits({})
        await opened.refetch()
      }
      try {
        await saveDomainWorkingCopy(projectId, copy.id, revision)
      }
      catch (error) {
        if (!isSaveConfirmationRequired(error))
          throw error
        const preview = await previewDomainWorkingCopy(projectId, copy.id)
        await openConfirmation({ status: 'warning', title: t('studio.devtools.working.risk_title'), description: <p>{t('studio.devtools.working.risk_confirm', { count: preview.changes.length })}</p> })
        await saveDomainWorkingCopy(projectId, copy.id, revision, true)
      }
      await client.invalidateQueries({ predicate: query => query.queryKey.includes(projectId) })
    }
    catch (error) {
      setFailure(String(error))
    }
    finally {
      setBusy(false)
    }
  }

  async function restore() {
    if (!lastSave || busy)
      return
    setBusy(true)
    setFailure('')
    try {
      await openConfirmation({ status: 'warning', title: t('studio.workbook.restore'), description: <p>{t('studio.workbook.restore_confirm', { count: lastSave.files.length })}</p> })
      await restoreDomainSaveNode(projectId, lastSave.id)
      await client.invalidateQueries({ predicate: query => query.queryKey.includes(projectId) })
    }
    catch (error) {
      if (error)
        setFailure(String(error))
    }
    finally {
      setBusy(false)
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex min-h-0 flex-col bg-canvas" role="dialog" aria-modal="true" aria-label={t('studio.workbook.title')}>
      <header className="flex items-center justify-between gap-3 border-b border-line px-4 py-2">
        <strong className="text-sm text-ink">{t('studio.workbook.title')}</strong>
        <div className="flex gap-2">
          <Button size="sm" isDisabled={busy || !lastSave || Object.keys(edits).length > 0 || Boolean(opened.data?.workingCopy?.dirty)} onPress={() => void restore()}>{t('studio.workbook.restore')}</Button>
          <Button size="sm" isDisabled={busy || !opened.data?.workingCopy || (!Object.keys(edits).length && !opened.data?.workingCopy?.dirty)} onPress={() => void save()}>{t('buttons.save')}</Button>
          <Button size="sm" isDisabled={busy || Object.keys(edits).length > 0} onPress={onClose}>{t('plugins.close')}</Button>
          <If cond={Object.keys(edits).length > 0}>
            <Button size="sm" isDisabled={busy} onPress={() => setEdits({})}>{t('studio.workbook.discard')}</Button>
          </If>
        </div>
      </header>
      <If cond={failure.length > 0}><p role="alert" className="px-4 py-2 text-sm text-danger">{failure}</p></If>
      <DomainWorkbookEditor file={file} data={opened.data?.data} loading={opened.isLoading} error={opened.error} editable={Boolean(opened.data?.workingCopy)} busy={busy} sheetName={sheet} edits={edits} onSheet={setSheet} onCellChange={edit => setEdits(previous => ({ ...previous, [xlsCellKey(path, edit.sheet, edit.row, edit.column)]: edit }))} />
      {confirmationHolder}
    </div>
  )
}
