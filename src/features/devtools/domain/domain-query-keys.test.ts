import { describe, expect, it } from 'vitest'
import { domainWorkspaceQueryKeys } from './domain-query-keys'

describe('domain workspace cache refresh', () => {
  it('refreshes the complete workbook after save and AI handoff', () => {
    expect(domainWorkspaceQueryKeys('project', 'item')).toContainEqual(['domain-workbook', 'project'])
  })
})
