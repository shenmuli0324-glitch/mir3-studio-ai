import type { DomainDraftPreview } from './types'
import { describe, expect, it } from 'vitest'
import { requiresSaveConfirmation } from './domain-save-policy'

describe('domain working copy save policy', () => {
  it('keeps ordinary text saves free of confirmation dialogs', () => {
    expect(requiresSaveConfirmation('item', preview({ unifiedDiff: '+name=Potion' }))).toBe(false)
  })

  it('requires confirmation for critical economic systems', () => {
    expect(requiresSaveConfirmation('shop', preview({ unifiedDiff: '+price=1' }))).toBe(true)
  })

  it('requires confirmation for deletions and binary changes', () => {
    expect(requiresSaveConfirmation('item', preview({ deleted: true, unifiedDiff: '-item' }))).toBe(true)
    expect(requiresSaveConfirmation('item', preview({ unifiedDiff: null }))).toBe(true)
  })
})

function preview(change: { deleted?: boolean, unifiedDiff?: string | null }): DomainDraftPreview {
  return {
    draft: {
      id: 'working-copy',
      intent: 'test',
      revision: 1,
      status: 'open',
      createdAt: 1,
      updatedAt: 1,
    },
    changes: [{
      path: 'client/test.lua',
      deleted: change.deleted ?? false,
      baseSha256: 'base',
      newSha256: 'next',
      unifiedDiff: change.unifiedDiff,
    }],
    diffHash: 'diff',
  }
}
