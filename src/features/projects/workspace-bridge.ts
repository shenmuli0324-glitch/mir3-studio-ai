import type { RefObject } from 'react'
import type { Mir3Project } from './types'
import { getIframeOrigin } from '@/utils/iframe-origin'
import { BridgeSequenceRegistry } from './bridge-sequence'

export const MIR3_BRIDGE_PROTOCOL_VERSION = 2

export interface Mir3BridgeEnvelope<T = unknown> {
  source: 'mir3-studio' | 'mir3-core-plugin'
  protocolVersion: 2
  type: string
  requestId: string
  projectId: string
  systemId: string
  taskId: string
  sessionId: string
  sequence: number
  payload: T
}

type BridgeListener = (message: Mir3BridgeEnvelope) => void

let activeIframeRef: RefObject<HTMLIFrameElement | null> | null = null
let bridgePort: MessagePort | null = null
const bridgeListeners = new Set<BridgeListener>()
const outgoingSequences = new BridgeSequenceRegistry()
const incomingSequences = new BridgeSequenceRegistry()

export function connectHarnessBridge(iframeRef: RefObject<HTMLIFrameElement | null>) {
  activeIframeRef = iframeRef
  function handleMessage(event: MessageEvent) {
    const origin = getIframeOrigin(iframeRef)
    if (!origin || event.origin !== origin || event.source !== iframeRef.current?.contentWindow)
      return
    dispatchBridgeMessage(event.data)
  }
  window.addEventListener('message', handleMessage)
  return () => {
    window.removeEventListener('message', handleMessage)
    if (activeIframeRef === iframeRef) {
      activeIframeRef = null
      bridgePort?.close()
      bridgePort = null
    }
  }
}

/** 用一次性 MessagePort 穿过 macOS 的不透明 Tauri origin，后续消息不再依赖宽泛 targetOrigin。 */
export function bootstrapHarnessBridge(iframeRef: RefObject<HTMLIFrameElement | null>) {
  const origin = getIframeOrigin(iframeRef)
  const target = iframeRef.current?.contentWindow
  if (!origin || !target)
    return false
  bridgePort?.close()
  const channel = new MessageChannel()
  bridgePort = channel.port1
  bridgePort.addEventListener('message', event => dispatchBridgeMessage(event.data))
  bridgePort.start()
  target.postMessage({
    source: 'mir3-studio',
    protocolVersion: MIR3_BRIDGE_PROTOCOL_VERSION,
    type: 'mir3/bridge.port',
  }, origin, [channel.port2])
  return true
}

export function subscribeHarnessBridge(listener: BridgeListener) {
  bridgeListeners.add(listener)
  return () => {
    bridgeListeners.delete(listener)
  }
}

export function waitForHarnessBridge(predicate: (message: Mir3BridgeEnvelope) => boolean, timeoutMs = 10_000) {
  return new Promise<Mir3BridgeEnvelope>((resolve, reject) => {
    let unsubscribe = () => {}
    const timeout = window.setTimeout(() => {
      unsubscribe()
      reject(new Error('HARNESS_BRIDGE_TIMEOUT: no matching bridge response'))
    }, timeoutMs)
    unsubscribe = subscribeHarnessBridge((message) => {
      if (!predicate(message))
        return
      window.clearTimeout(timeout)
      unsubscribe()
      resolve(message)
    })
  })
}

export function postHarnessBridge<T>(message: Omit<Mir3BridgeEnvelope<T>, 'source' | 'protocolVersion' | 'requestId' | 'sequence'> & {
  requestId?: string
}) {
  const iframeRef = activeIframeRef
  if (!iframeRef)
    return false
  if (!bridgePort)
    return false
  bridgePort.postMessage({
    ...message,
    source: 'mir3-studio',
    protocolVersion: MIR3_BRIDGE_PROTOCOL_VERSION,
    requestId: message.requestId ?? bridgeRequestId(),
    sequence: outgoingSequences.next(message),
  } satisfies Mir3BridgeEnvelope<T>)
  return true
}

export function postProjectActivation(
  iframeRef: RefObject<HTMLIFrameElement | null>,
  project: Mir3Project,
) {
  const origin = getIframeOrigin(iframeRef)
  if (!origin || !bridgePort)
    return false
  bridgePort.postMessage({
    source: 'mir3-studio',
    protocolVersion: MIR3_BRIDGE_PROTOCOL_VERSION,
    type: 'mir3/project.activate',
    requestId: bridgeRequestId(),
    projectId: project.id,
    systemId: '',
    taskId: '',
    sessionId: '',
    sequence: outgoingSequences.next({ projectId: project.id, taskId: '', sessionId: '' }),
    payload: {
      projectId: project.id,
      projectRoot: project.root,
      workspaceRoot: project.activeWorkspaceRoot,
    },
  })
  return true
}

function dispatchBridgeMessage(value: unknown) {
  if (!isBridgeEnvelope(value) || value.source !== 'mir3-core-plugin')
    return
  if (value.type === 'mir3/plugin.ready')
    incomingSequences.clear()
  if (!incomingSequences.accept(value, value.sequence))
    return
  bridgeListeners.forEach(listener => listener(value))
}

export function bridgeRequestId() {
  if (typeof crypto.randomUUID === 'function')
    return crypto.randomUUID()
  return `mir3-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function isBridgeEnvelope(value: unknown): value is Mir3BridgeEnvelope {
  if (!value || typeof value !== 'object')
    return false
  const message = value as Partial<Mir3BridgeEnvelope>
  return message.protocolVersion === MIR3_BRIDGE_PROTOCOL_VERSION
    && (message.source === 'mir3-studio' || message.source === 'mir3-core-plugin')
    && typeof message.type === 'string'
    && typeof message.requestId === 'string'
    && typeof message.projectId === 'string'
    && typeof message.systemId === 'string'
    && typeof message.taskId === 'string'
    && typeof message.sessionId === 'string'
    && Number.isSafeInteger(message.sequence)
    && message.sequence! > 0
    && 'payload' in message
}
