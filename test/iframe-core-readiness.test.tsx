// @vitest-environment happy-dom
import type { Mir3BridgeEnvelope } from '../src/features/projects/workspace-bridge'
import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { useIframeShim } from '../src/hooks/use-iframe-shim'

const mocks = vi.hoisted(() => ({ listener: null as null | ((message: Mir3BridgeEnvelope) => void), invoke: vi.fn(), canary: vi.fn(async () => ({ status: 'committed' })), ready: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ isMinimized: async () => false, isVisible: async () => true, onFocusChanged: async () => () => {}, onResized: async () => () => {} }) }))
vi.mock('../src/store', () => ({ store: { harness: { markCorePluginReady: mocks.ready } } }))
vi.mock('../src/features/projects/core-candidate-canary', () => ({ runCoreCandidateCanary: mocks.canary }))
vi.mock('../src/features/projects/workspace-bridge', async original => ({
  ...await original<object>(),
  subscribeHarnessBridge(listener: (message: Mir3BridgeEnvelope) => void) {
    mocks.listener = listener
    return () => {
      mocks.listener = null
    }
  },
}))

afterEach(cleanup)

it('consumes verified MessagePort bridge replies and defers empty-project startup without rollback', async () => {
  mocks.invoke.mockResolvedValue(null)
  renderHook(() => useIframeShim({ current: null }))
  const message = { type: 'mir3/bridge.description', payload: { protocolVersion: 2, capabilities: { sessions: true, workspaces: true, archive: true, snapshot: true, pendingInteraction: true, globalSession: true, ordinarySessionCanary: true, projectScope: true } } } as Mir3BridgeEnvelope
  await act(async () => mocks.listener?.(message))
  expect(mocks.canary).not.toHaveBeenCalled()
  mocks.invoke.mockResolvedValue({ id: 'project' })
  await act(async () => mocks.listener?.(message))
  await waitFor(() => expect(mocks.ready).toHaveBeenCalledOnce())
  expect(mocks.canary).toHaveBeenCalledOnce()
})
