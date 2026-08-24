import type { PropsWithOverlays } from '@overlastic/react'
import { Button, Modal } from '@heroui/react'
import { useDisclosure } from '@overlastic/react'
import { useTranslation } from 'react-i18next'

export interface PluginChangelogDialogProps extends PropsWithOverlays {
  pluginName: string
  version: string
  changelog: string
  system: boolean
}

/** 第一方内置插件的离线说明与更新记录，不依赖仓库或网络。 */
export function PluginChangelogDialog(props: PluginChangelogDialogProps) {
  const disclosure = useDisclosure({ props })
  const { t } = useTranslation()

  return (
    <Modal isOpen={disclosure.visible} onOpenChange={disclosure.cancel}>
      <Modal.Backdrop>
        <Modal.Container size="lg">
          <Modal.Dialog className="max-h-[min(720px,calc(100vh-64px))] w-[680px] max-w-[calc(100vw-48px)]">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{t('plugins.changelog_title')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body className="overflow-y-auto">
              <div className="rounded-md bg-panel2 px-4 py-3">
                <div className="flex items-center justify-between gap-3">
                  <span className="min-w-0 truncate text-sm font-medium text-ink">
                    {props.pluginName}
                  </span>
                  <code className="shrink-0 rounded bg-default px-2 py-1 font-mono text-xs text-muted">
                    {props.version}
                  </code>
                </div>
                <p className="mt-2 text-xs leading-5 text-muted">
                  {t(props.system ? 'plugins.changelog_local_hint' : 'plugins.changelog_optional_hint')}
                </p>
              </div>
              <pre className="mt-4 whitespace-pre-wrap break-words rounded-md bg-panel2 p-4 font-sans text-sm leading-6 text-ink">
                {props.changelog || t('plugins.changelog_empty')}
              </pre>
            </Modal.Body>
            <Modal.Footer>
              <Button className="rounded-md" variant="primary" onPress={disclosure.confirm}>
                {t('plugins.close')}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}
