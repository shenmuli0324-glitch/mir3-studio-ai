/* eslint-disable react/dom-no-unsafe-iframe-sandbox */
import type { RefObject } from 'react'
import type { Mir3Project } from '@/features/projects/types'
import type { HarnessSurface, HarnessWorkbenchState } from '@/layout/studio-types'
import { CircleExclamation } from '@gravity-ui/icons'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useStore } from 'valtio-define'
import { bootstrapHarnessBridge, connectHarnessBridge, ensureHarnessProjectActive, postHarnessBridge } from '@/features/projects/workspace-bridge'
import { useIframeShim } from '@/hooks/use-iframe-shim'
import { Loadable } from '@/layout/components/loadable'
import { store } from '@/store'
import { getIframeOrigin } from '@/utils/iframe-origin'

export function HarnessWorkbench({ active, iframeRef, surface, project }: {
  active: boolean
  iframeRef: RefObject<HTMLIFrameElement | null>
  surface: HarnessSurface
  project: Mir3Project | null
}) {
  const { t } = useTranslation()
  const harnessState: HarnessWorkbenchState = useStore(store.harness)
  const { serviceHealthy, iframeError, iframeKey, iframeLoaded, iframeSrc } = harnessState
  const serviceUrl = serviceOrigin(iframeSrc)
  const projectId = project?.id
  const projectRoot = project?.root
  const projectWorkspaceRoot = project?.activeWorkspaceRoot
  useIframeShim(iframeRef)

  useEffect(() => connectHarnessBridge(iframeRef), [iframeRef])

  useEffect(() => {
    if (!iframeLoaded)
      return
    const iframeOrigin = getIframeOrigin(iframeRef)
    if (!iframeOrigin)
      return
    iframeRef.current?.contentWindow?.postMessage(
      { source: 'dsh-desktop', type: 'mir3://surface:set', surface },
      iframeOrigin,
    )
  }, [iframeKey, iframeLoaded, iframeRef, surface])

  useEffect(() => {
    if (!iframeLoaded)
      return
    if (!bootstrapHarnessBridge(iframeRef))
      return
    postHarnessBridge({
      type: 'mir3/bridge.describe',
      projectId: projectId ?? '',
      systemId: '',
      taskId: '',
      sessionId: '',
      payload: {},
    })
    if (projectId && projectRoot && projectWorkspaceRoot) {
      void ensureHarnessProjectActive({ id: projectId, root: projectRoot, activeWorkspaceRoot: projectWorkspaceRoot })
        .catch(error => console.error('[MIR3 Core Plugin] project activation failed:', error))
    }
  }, [iframeKey, iframeLoaded, iframeRef, projectId, projectRoot, projectWorkspaceRoot])

  return (
    <section className={workbenchClass(active)} aria-hidden={!active}>
      <If cond={serviceHealthy} else={<Loadable subtitle={t('status.loading')} />}>
        <iframe
          key={iframeKey}
          ref={iframeRef}
          className="block h-full w-full border-none bg-load-bg"
          src={iframeSrc}
          allow="clipboard-read; clipboard-write; fullscreen"
          sandbox="allow-same-origin allow-scripts allow-popups allow-forms allow-modals allow-downloads allow-storage-access-by-user-activation"
          onLoad={store.harness.markIframeLoaded}
          onError={store.harness.markIframeError}
          title={t('app.open_editor')}
        />
      </If>
      <If cond={showIframeError(serviceHealthy, iframeError)}>
        <div className="absolute inset-0 z-[1]">
          <Loadable
            icon={CircleExclamation}
            title={t('ui.iframe_error')}
            errorMsg={t('ui.ensure_running', { url: serviceUrl })}
            onRetry={store.harness.refreshIframe}
          />
        </div>
      </If>
    </section>
  )
}

function showIframeError(serviceHealthy: boolean, iframeError: boolean) {
  return serviceHealthy && iframeError
}

function workbenchClass(active: boolean): string {
  const base = 'absolute inset-0 min-h-0 bg-canvas'
  if (active)
    return `${base} visible z-[1]`
  return `${base} invisible pointer-events-none z-0`
}

function serviceOrigin(source: string): string {
  try {
    return new URL(source).origin
  }
  catch {
    return source
  }
}
