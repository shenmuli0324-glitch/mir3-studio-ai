import type { CompositeDraftApplyResult, CompositeDraftReview, CompositeDraftReviewItem } from './types'
import { Button, Modal, Spinner } from '@heroui/react'
import { useOverlay } from '@overlastic/react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { Modal as ConfirmationModal } from '@/components/modal'
import { toast } from '@/utils'
import { applyCompositeDrafts, previewCompositeDrafts } from './api'

export interface CompositeDraftReviewRequest {
  projectId: string
  compositeId: string
  taskId: string
  sessionId: string
}

export function CompositeDraftReviewDialog({ request, onClose, onApplied }: {
  request: CompositeDraftReviewRequest
  onClose: () => void
  onApplied: (result: CompositeDraftApplyResult) => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [confirmationHolder, openConfirmation] = useOverlay(ConfirmationModal, { type: 'holder' })
  const review = useQuery({
    queryKey: ['composite-draft-review', request.projectId, request.compositeId],
    queryFn: () => previewCompositeDrafts(request.projectId, request.compositeId),
  })
  const apply = useMutation({
    mutationFn: (value: CompositeDraftReview) => applyCompositeDrafts(
      request.projectId,
      request.compositeId,
      value.drafts.map(draft => ({
        draftId: draft.draftId,
        confirmationToken: draft.confirmation.confirmationToken,
      })),
    ),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: ['domain-drafts', request.projectId] })
      void queryClient.invalidateQueries({ queryKey: ['domain-files', request.projectId] })
      void queryClient.invalidateQueries({ queryKey: ['domain-resources', request.projectId] })
      void queryClient.invalidateQueries({ queryKey: ['domain-source', request.projectId] })
      toast(t('studio.composite_review.applied'), {})
      onApplied(result)
    },
    onError: (reason) => {
      toast(String(reason), { variant: 'danger' })
      void review.refetch()
    },
  })

  async function confirmApply() {
    if (!review.data || !reviewCanApply(review.data))
      return
    try {
      await openConfirmation({
        status: 'warning',
        title: t('studio.composite_review.apply'),
        description: <p>{t('studio.composite_review.apply_confirm', { count: review.data.drafts.length })}</p>,
      })
      apply.mutate(review.data)
    }
    catch {
      // 用户取消后保留复核内容。
    }
  }

  function closeDialog(open: boolean) {
    if (!open && !apply.isPending)
      onClose()
  }

  function body() {
    if (review.isLoading)
      return <ReviewLoading />
    if (review.error)
      return <ReviewError message={String(review.error)} onRetry={() => void review.refetch()} />
    if (!review.data)
      return null
    return <ReviewContent review={review.data} />
  }

  const validCount = review.data?.drafts.filter(draft => draft.validation.valid).length ?? 0
  return (
    <>
      <Modal isOpen onOpenChange={closeDialog}>
        <Modal.Backdrop>
          <Modal.Container size="lg">
            <Modal.Dialog className="max-h-[min(820px,calc(100vh-48px))] w-[960px] max-w-[calc(100vw-48px)]">
              <Modal.CloseTrigger isDisabled={apply.isPending} />
              <Modal.Header>
                <Modal.Heading>{t('studio.composite_review.title')}</Modal.Heading>
              </Modal.Header>
              <Modal.Body className="min-h-0 overflow-y-auto">
                <p className="mb-4 text-xs leading-5 text-muted">
                  {t('studio.composite_review.description', { compositeId: request.compositeId })}
                </p>
                {body()}
              </Modal.Body>
              <Modal.Footer className="items-center justify-between gap-3">
                <span className="text-[10px] text-muted">
                  {t('studio.composite_review.validation_summary', {
                    valid: validCount,
                    total: review.data?.drafts.length ?? 0,
                  })}
                </span>
                <div className="flex gap-2">
                  <Button variant="ghost" isDisabled={apply.isPending} onPress={onClose}>{t('studio.composite_review.close')}</Button>
                  <Button
                    className="bg-accent text-white"
                    isDisabled={!review.data || !reviewCanApply(review.data)}
                    isPending={apply.isPending}
                    onPress={() => void confirmApply()}
                  >
                    {t('studio.composite_review.apply')}
                  </Button>
                </div>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
      {confirmationHolder}
    </>
  )
}

function ReviewContent({ review }: { review: CompositeDraftReview }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-4" data-composite-review={review.compositeId}>
      {review.drafts.map(draft => <DraftReviewCard key={draft.draftId} draft={draft} />)}
      <If cond={review.drafts.length < 2}>
        <div className="rounded-xl border border-danger/30 bg-danger/10 p-3 text-xs text-danger">{t('studio.composite_review.incomplete')}</div>
      </If>
    </div>
  )
}

function DraftReviewCard({ draft }: { draft: CompositeDraftReviewItem }) {
  const { t } = useTranslation()
  const title = t(`studio.devtools.tool.${draft.systemId}.title`, { defaultValue: draft.systemId })
  return (
    <section className="overflow-hidden rounded-xl border border-line bg-panel2" data-composite-draft={draft.draftId}>
      <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-3">
        <span className="min-w-0">
          <strong className="block truncate text-xs text-ink">{title}</strong>
          <small className="mt-1 block truncate text-[10px] text-muted">{draft.confirmation.preview.draft.intent}</small>
          <small className="mt-0.5 block text-[9px] text-muted">
            {draft.pluginVersion}
            {' · r'}
            {draft.confirmation.preview.draft.revision}
            {' · '}
            {t('studio.composite_review.change_count', { count: draft.confirmation.preview.changes.length })}
          </small>
        </span>
        <span className={validationBadgeClass(draft.validation.valid)}>
          {t(validationLabelKey(draft.validation.valid))}
        </span>
      </header>
      <If cond={draft.validation.diagnostics.length > 0}>
        <div className="space-y-1 border-b border-line px-4 py-3">
          {draft.validation.diagnostics.map(diagnostic => <p key={diagnostic} className="text-[10px] leading-5 text-muted">{diagnostic}</p>)}
        </div>
      </If>
      <div className="space-y-2 p-3">
        {draft.confirmation.preview.changes.map(change => (
          <article key={change.path} className="overflow-hidden rounded-lg border border-line bg-panel">
            <h3 className="border-b border-line px-3 py-2 font-mono text-[10px] text-ink">{change.path}</h3>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap p-3 text-[10px] leading-5 text-muted">{change.unifiedDiff ?? t('studio.devtools.diff.binary')}</pre>
          </article>
        ))}
        <If cond={draft.confirmation.preview.changes.length === 0}>
          <p className="rounded-lg border border-line px-3 py-3 text-[10px] text-muted">{t('studio.composite_review.no_changes')}</p>
        </If>
      </div>
    </section>
  )
}

function ReviewLoading() {
  const { t } = useTranslation()
  return (
    <div className="grid min-h-64 place-items-center">
      <span className="flex items-center gap-2 text-xs text-muted" role="status">
        <Spinner size="sm" />
        {t('studio.composite_review.loading')}
      </span>
    </div>
  )
}

function ReviewError({ message, onRetry }: { message: string, onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="grid min-h-64 place-items-center text-center">
      <div>
        <strong className="text-sm text-danger">{t('studio.composite_review.load_failed')}</strong>
        <p className="mt-2 max-w-xl text-xs leading-5 text-muted">{message}</p>
        <Button className="mt-4" variant="secondary" onPress={onRetry}>{t('studio.composite_review.retry')}</Button>
      </div>
    </div>
  )
}

function reviewCanApply(review: CompositeDraftReview): boolean {
  return review.drafts.length >= 2 && review.drafts.every(draft => draft.validation.valid)
}

function validationLabelKey(valid: boolean): string {
  if (valid)
    return 'studio.composite_review.valid'
  return 'studio.composite_review.invalid'
}

function validationBadgeClass(valid: boolean): string {
  const base = 'shrink-0 rounded-full px-2 py-1 text-[9px] font-medium'
  if (valid)
    return `${base} bg-success/10 text-success`
  return `${base} bg-danger/10 text-danger`
}
