import type { RefObject } from 'react'
import type { Mir3Project } from './types'
import { getIframeOrigin } from '@/utils/iframe-origin'

export function postProjectActivation(
  iframeRef: RefObject<HTMLIFrameElement | null>,
  project: Mir3Project,
) {
  const origin = getIframeOrigin(iframeRef)
  if (!origin)
    return false
  iframeRef.current?.contentWindow?.postMessage(
    {
      source: 'mir3-studio',
      type: 'mir3/project.activate',
      version: 1,
      requestId: projectRequestId(),
      payload: {
        projectId: project.id,
        projectRoot: project.root,
        workspaceRoot: project.activeWorkspaceRoot,
      },
    },
    origin,
  )
  return true
}

function projectRequestId() {
  if (typeof crypto.randomUUID === 'function')
    return crypto.randomUUID()
  return `mir3-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
