// @vitest-environment happy-dom

import type { RefObject } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { connectHarnessBridge, ensureHarnessProjectActive } from './workspace-bridge'

const project = {
  id: 'project-windows',
  root: 'D:\\996\\项目',
  activeWorkspaceRoot: 'D:\\996\\项目\\客户端',
}

let disconnect: (() => void) | null = null

afterEach(() => {
  disconnect?.()
  disconnect = null
  vi.restoreAllMocks()
})

describe('harness project activation bridge', () => {
  it('retries the MessagePort handshake when the Windows client plugin mounts after iframe load', async () => {
    let bootstrapCount = 0
    const target = {
      postMessage(message: { type?: string }, _origin: string, ports?: MessagePort[]) {
        if (message.type !== 'mir3/bridge.port' || !ports?.[0])
          return
        bootstrapCount++
        if (bootstrapCount < 3)
          return
        attachReadyPlugin(ports[0])
      },
    }
    const iframeRef = {
      current: {
        src: 'http://127.0.0.1:3080/?test=windows-late-plugin',
        contentWindow: target,
      },
    } as unknown as RefObject<HTMLIFrameElement | null>
    disconnect = connectHarnessBridge(iframeRef)

    await expect(ensureHarnessProjectActive(project)).resolves.toBeUndefined()
    expect(bootstrapCount).toBe(3)
  })
})

function attachReadyPlugin(port: MessagePort) {
  let sequence = 0
  port.addEventListener('message', (event) => {
    const request = event.data as { type?: string, requestId: string, projectId: string, systemId: string, taskId: string, sessionId: string }
    if (request.type !== 'mir3/project.activate')
      return
    port.postMessage(envelope('mir3/project.activated', request, { canonicalPath: project.activeWorkspaceRoot }, ++sequence))
  })
  port.start()
  queueMicrotask(() => {
    port.postMessage(envelope('mir3/plugin.ready', {
      requestId: 'ready-windows',
      projectId: '',
      systemId: '',
      taskId: '',
      sessionId: '',
    }, { protocolVersion: 2 }, ++sequence))
  })
}

function envelope(
  type: string,
  request: { requestId: string, projectId: string, systemId: string, taskId: string, sessionId: string },
  payload: unknown,
  sequence: number,
) {
  return {
    source: 'mir3-core-plugin',
    protocolVersion: 2,
    type,
    requestId: request.requestId,
    projectId: request.projectId,
    systemId: request.systemId,
    taskId: request.taskId,
    sessionId: request.sessionId,
    sequence,
    payload,
  }
}
