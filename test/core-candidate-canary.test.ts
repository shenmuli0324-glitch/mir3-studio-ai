import { describe, expect, it, vi } from 'vitest'
import { runCoreCandidateCanary } from '../src/features/projects/core-candidate-canary'

describe('core candidate canary promotion', () => {
  it('commits only after every runtime canary passes', async () => {
    const markReady = vi.fn(async () => {})
    const rollback = vi.fn(async () => true)
    const outcome = await runCoreCandidateCanary({
      runCanary: async () => {},
      markReady,
      rollback,
      relaunch: async () => {},
      refresh() {},
    })
    expect(outcome).toEqual({ status: 'committed' })
    expect(markReady).toHaveBeenCalledOnce()
    expect(rollback).not.toHaveBeenCalled()
  })

  it('rolls back and reloads Harness when the real MCP or Session canary fails', async () => {
    const markReady = vi.fn(async () => {})
    const rollback = vi.fn(async () => true)
    const relaunch = vi.fn(async () => {})
    const refresh = vi.fn()
    const failure = new Error('CORE_MCP_CANARY_CAPABILITY_FAILED')
    const outcome = await runCoreCandidateCanary({
      runCanary: async () => Promise.reject(failure),
      markReady,
      rollback,
      relaunch,
      refresh,
    })
    expect(outcome).toEqual({ status: 'rejected', error: failure, rolledBack: true })
    expect(markReady).not.toHaveBeenCalled()
    expect(rollback).toHaveBeenCalledOnce()
    expect(relaunch).toHaveBeenCalledOnce()
    expect(refresh).toHaveBeenCalledOnce()
  })
})
