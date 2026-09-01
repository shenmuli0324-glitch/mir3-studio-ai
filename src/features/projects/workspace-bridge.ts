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
type HarnessProjectScope = Pick<Mir3Project, 'id' | 'root' | 'activeWorkspaceRoot'>

let activeIframeRef: RefObject<HTMLIFrameElement | null> | null = null
let bridgePort: MessagePort | null = null
let bridgeReady = false
let activeProjectScopeKey: string | null = null
const pendingProjectActivations = new Map<string, Promise<void>>()
const bridgeListeners = new Set<BridgeListener>()
const outgoingSequences = new BridgeSequenceRegistry()
const incomingSequences = new BridgeSequenceRegistry()

export function connectHarnessBridge(iframeRef: RefObject<HTMLIFrameElement | null>) {
  activeIframeRef = iframeRef
  function handleMessage(event: MessageEvent) {
    const origin = getIframeOrigin(iframeRef)
    if (!origin || event.origin !== origin || event.source !== iframeRef.current?.contentWindow)
      return
    dispatchBridgeMessage(event.data, 'window')
  }
  window.addEventListener('message', handleMessage)
  return () => {
    window.removeEventListener('message', handleMessage)
    if (activeIframeRef === iframeRef) {
      activeIframeRef = null
      bridgePort?.close()
      bridgePort = null
      bridgeReady = false
      activeProjectScopeKey = null
      pendingProjectActivations.clear()
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
  bridgeReady = false
  activeProjectScopeKey = null
  const channel = new MessageChannel()
  bridgePort = channel.port1
  bridgePort.addEventListener('message', event => dispatchBridgeMessage(event.data, 'port'))
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

/** 在创建或恢复 AI Session 前确认 Harness 已接受当前项目作用域。 */
export function ensureHarnessProjectActive(project: HarnessProjectScope): Promise<void> {
  const scopeKey = projectScopeKey(project)
  if (activeProjectScopeKey === scopeKey)
    return Promise.resolve()
  const pending = pendingProjectActivations.get(scopeKey)
  if (pending)
    return pending
  const activation = activateHarnessProject(project, scopeKey)
  pendingProjectActivations.set(scopeKey, activation)
  void activation.then(
    () => pendingProjectActivations.delete(scopeKey),
    () => pendingProjectActivations.delete(scopeKey),
  )
  return activation
}

async function activateHarnessProject(project: HarnessProjectScope, scopeKey: string): Promise<void> {
  try {
    await ensureHarnessBridgeReady()
    await activateHarnessProjectOnce(project)
  }
  catch (error) {
    if (!isRetryableActivationError(error) || !activeIframeRef)
      throw error
    bridgeReady = false
    await ensureHarnessBridgeReady()
    await activateHarnessProjectOnce(project)
  }
  activeProjectScopeKey = scopeKey
}

/**
 * Windows WebView2 可能先触发 iframe load，随后才挂载客户端插件。传给尚未监听的
 * frame 的 MessagePort 不会被补收，因此这里重复建立短期端口，直到插件通过该端口
 * 返回 ready。项目激活必须发生在 ready 之后，不能再依赖首次启动弹窗或猜测延时。
 */
async function ensureHarnessBridgeReady(): Promise<void> {
  if (bridgeReady && bridgePort)
    return
  const iframeRef = activeIframeRef
  if (!iframeRef)
    throw new Error('HARNESS_BRIDGE_UNAVAILABLE: Harness frame is unavailable')
  let lastError: unknown = new Error('HARNESS_BRIDGE_TIMEOUT: MIR3 Core Plugin is not ready')
  for (let attempt = 0; attempt < 8; attempt++) {
    const ready = waitForHarnessBridge(
      message => message.type === 'mir3/plugin.ready',
      1_250,
    )
    if (!bootstrapHarnessBridge(iframeRef)) {
      void ready.catch(() => {})
      throw new Error('HARNESS_BRIDGE_UNAVAILABLE: Harness frame is unavailable')
    }
    try {
      await ready
      if (bridgeReady && bridgePort)
        return
    }
    catch (error) {
      lastError = error
    }
  }
  throw lastError
}

async function activateHarnessProjectOnce(project: HarnessProjectScope): Promise<void> {
  const requestId = bridgeRequestId()
  const response = waitForHarnessBridge(message => message.requestId === requestId
    && (message.type === 'mir3/project.activated' || message.type === 'mir3/bridge.error'))
  const posted = postHarnessBridge({
    type: 'mir3/project.activate',
    requestId,
    projectId: project.id,
    systemId: '__project__',
    taskId: 'project-activation',
    sessionId: '',
    payload: {
      projectRoot: project.root,
      workspaceRoot: project.activeWorkspaceRoot,
      startSession: false,
    },
  })
  if (!posted) {
    void response.catch(() => {})
    throw new Error('HARNESS_BRIDGE_UNAVAILABLE: project activation was not delivered')
  }
  const result = await response
  if (result.type === 'mir3/bridge.error')
    throw new Error(bridgeError(result.payload))
  if (result.projectId !== project.id)
    throw new Error('PROJECT_SCOPE_MISMATCH: Harness activated another project')
}

function isRetryableActivationError(error: unknown): boolean {
  const message = String(error)
  return message.includes('HARNESS_BRIDGE_TIMEOUT')
    || message.includes('HARNESS_BRIDGE_UNAVAILABLE')
}

function dispatchBridgeMessage(value: unknown, channel: 'port' | 'window') {
  if (!isBridgeEnvelope(value) || value.source !== 'mir3-core-plugin')
    return
  if (value.type === 'mir3/plugin.ready') {
    if (channel === 'port')
      bridgeReady = true
    incomingSequences.clear()
    activeProjectScopeKey = null
  }
  if (!incomingSequences.accept(value, value.sequence))
    return
  bridgeListeners.forEach(listener => listener(value))
}

function projectScopeKey(project: HarnessProjectScope): string {
  return `${project.id}\u241F${project.root}\u241F${project.activeWorkspaceRoot}`
}

function bridgeError(payload: unknown): string {
  if (!payload || typeof payload !== 'object')
    return String(payload)
  const error = payload as { code?: string, message?: string }
  return [error.code, error.message].filter(Boolean).join(': ')
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
