import type { TaskScopeLease } from '../src/features/devtools/domain/types'
import type { Mir3BridgeEnvelope } from '../src/features/projects/workspace-bridge'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { draftHandoffs, isGlobalDraftEvent, registeredGlobalTask, registerGlobalTask, returnTarget, unregisterGlobalTask, verifyDevtoolsTarget } from '../src/features/system-ai/ai-handoff'
import { currentScopeLease, includeScopeLeaseDraft, manageScopeLease, stopScopeLease } from '../src/features/system-ai/scope-lease-manager'

describe('aI Draft handoff contract', () => {
  const identity = {
    projectId: 'project-1',
    systemId: 'shop',
    taskId: 'global-task-1',
    sessionId: 'global-session-1',
    allowedSystems: ['shop', 'item'],
    allowedWriteSystems: ['shop'],
  }

  afterEach(() => {
    unregisterGlobalTask(identity)
    stopScopeLease(identity, false)
  })

  it('accepts structured Draft results only for the exact task identity and allowed domain', () => {
    const message = envelope({
      domainResults: [{ draftId: 'draft-1', revision: 3, systemId: 'shop', validation: { valid: true, diagnostics: [] }, changedResources: ['shop:item:1'] }],
      returnTo: { view: 'devtools', projectId: 'project-1', systemId: 'shop', resourceId: 'shop:item:1', draftId: 'draft-1' },
    })
    expect(draftHandoffs(message, identity)).toEqual([{
      draftId: 'draft-1',
      revision: 3,
      systemId: 'shop',
      validation: { valid: true, diagnostics: [] },
      changedResources: ['shop:item:1'],
      resourceId: null,
    }])
    expect(returnTarget(message, identity)).toMatchObject({ systemId: 'shop', resourceId: 'shop:item:1', draftId: 'draft-1' })

    expect(draftHandoffs({ ...message, sessionId: 'global-session-wrong' }, identity)).toEqual([])
    expect(draftHandoffs({ ...message, projectId: 'project-other' }, identity)).toEqual([])
    expect(draftHandoffs({ ...message, systemId: 'map' }, identity)).toEqual([])
    expect(draftHandoffs(envelope({ domainResults: [{ draftId: 'draft-readonly', revision: 1, systemId: 'item' }] }), identity)).toEqual([])
  })

  it('fails closed for traversal, foreign projects, and unregistered global tasks', () => {
    const malicious = envelope({
      domainResults: [],
      returnTo: { view: 'devtools', projectId: 'project-1', systemId: 'shop', resourceId: '../../secret' },
    })
    expect(returnTarget(malicious, identity)).toBeNull()
    expect(returnTarget(envelope({ returnTo: { view: 'devtools', projectId: 'foreign', systemId: 'shop' } }), identity)).toBeNull()

    registerGlobalTask({ ...identity, draftIds: ['draft-1'] })
    expect(registeredGlobalTask(envelope({}))).not.toBeNull()
    expect(registeredGlobalTask({ ...envelope({}), taskId: 'wrong-task' })).toBeNull()
  })

  it('verifies returned resources and Draft revisions before consuming snapshot and complete updates', async () => {
    const target = returnTarget(envelope({
      returnTo: { view: 'devtools', projectId: 'project-1', systemId: 'shop', resourceId: 'shop:item:1', draftId: 'draft-1' },
    }), identity)
    expect(target).not.toBeNull()
    const verification = {
      isKnownSystem: (systemId: string) => systemId === 'shop',
      getResource: vi.fn(async () => ({ id: 'shop:item:1', systemId: 'shop', files: [{ path: 'Data/shop.txt' }] })),
      previewDraft: vi.fn(async () => ({ preview: { draft: { revision: 5 } } })),
      validateDraft: vi.fn(async () => ({ systemId: 'shop' })),
      nonce: () => 'verified-nonce',
    }
    const verified = await verifyDevtoolsTarget(target!, [{
      draftId: 'draft-1',
      revision: 4,
      systemId: 'shop',
      changedResources: ['shop:item:1'],
    }], verification)
    expect(verified).toMatchObject({ relativePath: 'Data/shop.txt', revision: 5, nonce: 'verified-nonce' })
    expect(isGlobalDraftEvent('mir3/globalSession.snapshot')).toBe(true)
    expect(isGlobalDraftEvent('mir3/globalSession.completed')).toBe(true)

    verification.validateDraft.mockResolvedValueOnce({ systemId: 'item' })
    await expect(verifyDevtoolsTarget(target!, [], verification)).resolves.toBeNull()
    verification.getResource.mockResolvedValueOnce({ id: 'shop:item:foreign', systemId: 'shop', files: [{ path: 'Data/foreign.txt' }] })
    await expect(verifyDevtoolsTarget(target!, [], verification)).resolves.toBeNull()
  })

  it('renews only the live matching lease and stops renewal after completion', async () => {
    const scheduled: Array<() => void> = []
    const revoked: string[] = []
    const renew = vi.fn(async (previous: TaskScopeLease) => ({ ...lease('token-2', 900_000), draftIds: previous.draftIds }))
    manageScopeLease({
      identity,
      lease: lease('token-1', 301_000),
      renew,
      revoke: async value => void revoked.push(value.token),
      now: () => 0,
      schedule(callback) {
        scheduled.push(callback)
        return scheduled.length
      },
      cancelSchedule() {},
    })
    expect(currentScopeLease(identity)?.token).toBe('token-1')
    includeScopeLeaseDraft(identity, 'draft-from-ai')
    scheduled[0]()
    await vi.waitFor(() => expect(renew).toHaveBeenCalledTimes(1))
    expect(renew.mock.calls[0][0].draftIds).toContain('draft-from-ai')
    expect(currentScopeLease(identity)?.token).toBe('token-2')
    expect(revoked).toContain('token-1')

    stopScopeLease(identity)
    expect(currentScopeLease(identity)).toBeNull()
    scheduled.at(-1)?.()
    await Promise.resolve()
    expect(renew).toHaveBeenCalledTimes(1)
    expect(revoked).toContain('token-2')
  })

  it('revokes and rejects leases bound to another task instead of scheduling renewal', async () => {
    const revoked: string[] = []
    const schedule = vi.fn()
    expect(() => manageScopeLease({
      identity,
      lease: { ...lease('foreign-token', 301_000), taskId: 'foreign-task' },
      renew: async previous => previous,
      revoke: async value => void revoked.push(value.token),
      schedule,
    })).toThrow('SCOPE_LEASE_IDENTITY_MISMATCH')
    await vi.waitFor(() => expect(revoked).toEqual(['foreign-token']))
    expect(schedule).not.toHaveBeenCalled()
    expect(currentScopeLease(identity)).toBeNull()
  })
})

function envelope(payload: unknown): Mir3BridgeEnvelope {
  return {
    source: 'mir3-core-plugin',
    protocolVersion: 2,
    type: 'mir3/globalSession.completed',
    requestId: 'request-1',
    projectId: 'project-1',
    systemId: 'shop',
    taskId: 'global-task-1',
    sessionId: 'global-session-1',
    sequence: 4,
    payload,
  }
}

function lease(token: string, expiresAt: number) {
  return {
    token,
    taskId: 'global-task-1',
    readSystems: ['shop', 'item'],
    writeSystems: ['shop'],
    draftIds: ['draft-1'],
    pluginVersions: { shop: '1.0.0', item: '1.0.0' },
    expiresAt,
  }
}
