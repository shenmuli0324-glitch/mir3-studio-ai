import type { DomainDraftPreview, DomainValidationReport } from './types'
import { Button, Modal } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'

export function DomainChangeReviewDialog({ preview, validation, onClose }: {
  preview: DomainDraftPreview | null
  validation: DomainValidationReport | null
  onClose: () => void
}) {
  const { t } = useTranslation()
  if (!preview)
    return null

  function closeDialog(open: boolean) {
    if (!open)
      onClose()
  }

  return (
    <Modal isOpen onOpenChange={closeDialog}>
      <Modal.Backdrop>
        <Modal.Container size="lg">
          <Modal.Dialog className="max-h-[min(820px,calc(100vh-48px))] w-[960px] max-w-[calc(100vw-48px)]">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{t('studio.devtools.working.review_title')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body className="min-h-0 overflow-y-auto">
              <ChangeSummary preview={preview} validation={validation} />
              <div className="mt-4 space-y-2">
                {preview.changes.map(change => (
                  <article key={change.path} className="overflow-hidden rounded-xl border border-line bg-panel2">
                    <header className="flex items-center justify-between gap-3 border-b border-line px-3 py-2">
                      <strong className="min-w-0 truncate font-mono text-[10px] text-ink">{change.path}</strong>
                      <If cond={change.deleted}>
                        <span className="shrink-0 rounded-full bg-danger/10 px-2 py-1 text-[9px] text-danger">{t('studio.devtools.working.deleted')}</span>
                      </If>
                    </header>
                    <pre className="max-h-72 overflow-auto whitespace-pre-wrap p-3 font-mono text-[10px] leading-5 text-muted">{change.unifiedDiff ?? t('studio.devtools.working.binary_change')}</pre>
                  </article>
                ))}
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="secondary" onPress={onClose}>{t('studio.devtools.working.review_close')}</Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}

function ChangeSummary({ preview, validation }: { preview: DomainDraftPreview, validation: DomainValidationReport | null }) {
  const { t } = useTranslation()
  return (
    <section className="rounded-xl border border-line bg-panel p-3">
      <strong className="text-xs text-ink">{t('studio.devtools.working.change_count', { count: preview.changes.length })}</strong>
      <If cond={validation?.valid === true}>
        <p className="mt-1 text-[10px] text-success">{t('studio.devtools.working.validation_passed')}</p>
      </If>
      <If cond={validation?.valid === false}>
        <div className="mt-2 space-y-1">
          {validation?.diagnostics.map(diagnostic => <p key={diagnostic} className="text-[10px] leading-5 text-danger">{diagnostic}</p>)}
        </div>
      </If>
    </section>
  )
}
