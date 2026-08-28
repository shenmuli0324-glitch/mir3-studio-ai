import type { GuiTemplateRequest } from './types'
import { Button, Modal } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { GuiDesignerScope } from './gui-designer-scope'
import { isValidGuiRelativePath } from './gui-relative-path'

export function DesignerDialogs() {
  const scope = useScope(GuiDesignerScope)
  return (
    <If cond={scope.newDialogOpen}><NewPageDialog /></If>
  )
}

function NewPageDialog() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const [relativePath, setRelativePath] = useState(() => t('studio.gui.new_dialog.default_path'))
  const [targets, setTargets] = useState<GuiTemplateRequest['targets']>('mobile')
  const valid = isValidGuiRelativePath(relativePath)

  function closeDialog(open: boolean) {
    if (!open)
      scope.setNewDialogOpen(false)
  }

  return (
    <Modal isOpen onOpenChange={closeDialog}>
      <Modal.Backdrop>
        <Modal.Container size="md">
          <Modal.Dialog className="max-h-[calc(100vh-64px)] w-[min(440px,calc(100vw-64px))]">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{t('studio.gui.new_dialog.title')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <p className="mb-5 text-xs leading-5 text-muted">{t('studio.gui.new_dialog.description')}</p>
              <label className="block">
                <span className="mb-1.5 block text-[10px] font-medium text-muted">{t('studio.gui.new_dialog.path')}</span>
                <div className="flex h-9 items-center rounded-lg bg-panel-2 px-3 ring-1 ring-line focus-within:ring-accent">
                  <span className="text-[11px] text-muted">GUIExport/</span>
                  <input className="min-w-0 flex-1 bg-transparent text-[11px] text-ink outline-none" value={relativePath} onChange={event => setRelativePath(event.target.value)} />
                </div>
              </label>
              <If cond={!valid}><p className="mt-2 text-[10px] text-danger">{t('studio.gui.new_dialog.path_invalid')}</p></If>
              <div className="mt-5">
                <span className="mb-2 block text-[10px] font-medium text-muted">{t('studio.gui.new_dialog.targets')}</span>
                <div className="grid grid-cols-3 gap-2">
                  {(['mobile', 'pc', 'both'] as const).map(target => (
                    <Button className={targetClass(targets === target)} variant="ghost" onPress={() => setTargets(target)} key={target}>{t(`studio.gui.new_dialog.target.${target}`)}</Button>
                  ))}
                </div>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="ghost" onPress={() => scope.setNewDialogOpen(false)}>{t('studio.gui.cancel')}</Button>
              <Button variant="primary" isDisabled={!valid || scope.busy} isPending={scope.busy} onPress={() => void scope.createPage({ relativePath, targets }).catch(() => {})}>{t('studio.gui.create')}</Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}

function targetClass(active: boolean): string {
  const base = 'h-16 rounded-xl text-[11px] ring-1'
  if (active)
    return `${base} bg-accent/12 text-accent ring-accent/40`
  return `${base} bg-panel-2 text-muted ring-line hover:text-ink`
}
