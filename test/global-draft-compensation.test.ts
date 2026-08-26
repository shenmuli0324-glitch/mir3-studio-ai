import { afterEach, describe, expect, it, vi } from 'vitest'
import { compensateGlobalDraftSetup } from '../src/features/system-ai/global-draft-compensation'

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))

afterEach(() => {
  mocks.invoke.mockReset()
})

describe('global Draft setup compensation', () => {
  it('discards newly created Drafts in reverse order and releases the existing Draft association', async () => {
    mocks.invoke.mockResolvedValue({})
    const errors = await compensateGlobalDraftSetup(
      'project-1',
      ['draft-created-1', 'draft-created-2'],
      {
        draftId: 'draft-existing',
        systemId: 'shop',
        pluginVersion: '1.3.0',
        compositeId: 'composite-failed',
      },
    )

    expect(errors).toEqual([])
    expect(mocks.invoke.mock.calls).toEqual([
      ['draft_discard', { projectId: 'project-1', draftId: 'draft-created-2' }],
      ['draft_discard', { projectId: 'project-1', draftId: 'draft-created-1' }],
      ['domain_draft_composite_disassociate', {
        projectId: 'project-1',
        draftId: 'draft-existing',
        systemId: 'shop',
        pluginVersion: '1.3.0',
        compositeId: 'composite-failed',
      }],
    ])
  })

  it('attempts every compensation and reports each failure', async () => {
    mocks.invoke.mockRejectedValue(new Error('fixture cleanup failed'))
    const errors = await compensateGlobalDraftSetup(
      'project-1',
      ['draft-created'],
      {
        draftId: 'draft-existing',
        systemId: 'shop',
        pluginVersion: '1.3.0',
        compositeId: 'composite-failed',
      },
    )

    expect(errors).toHaveLength(2)
    expect(errors[0]).toContain('GLOBAL_DRAFT_COMPENSATION_DISCARD_FAILED:draft-created')
    expect(errors[1]).toContain('GLOBAL_DRAFT_COMPENSATION_DISASSOCIATE_FAILED:draft-existing')
  })
})
