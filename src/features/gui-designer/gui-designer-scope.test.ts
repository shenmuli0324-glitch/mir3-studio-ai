import { describe, expect, it } from 'vitest'
import { diskConflictAfterFileSwitch, diskConflictAfterProbe, diskConflictForPath, mergeSaveNodes, shouldForceGuiProbe } from './gui-designer-scope'

describe('gui document probe cadence', () => {
  it('forces a full hash every thirty polling intervals', () => {
    expect(shouldForceGuiProbe(0)).toBe(false)
    expect(shouldForceGuiProbe(1)).toBe(false)
    expect(shouldForceGuiProbe(29)).toBe(false)
    expect(shouldForceGuiProbe(30)).toBe(true)
    expect(shouldForceGuiProbe(60)).toBe(true)
  })
})

describe('gui disk conflict isolation', () => {
  it('does not expose one cached file conflict on another file', () => {
    const conflict = { path: 'GUIExport/a.lua', reason: 'GUI_EXTERNAL_CHANGE_CONFLICT' }
    expect(diskConflictForPath(conflict, 'GUIExport/a.lua')).toBe('GUI_EXTERNAL_CHANGE_CONFLICT')
    expect(diskConflictForPath(conflict, 'GUIExport/b.lua')).toBeNull()
    expect(diskConflictAfterFileSwitch(conflict, 'GUIExport/b.lua')).toBeNull()
    expect(diskConflictAfterFileSwitch(conflict, 'GUIExport/a.lua')).toEqual(conflict)
  })

  it('clears a missing conflict after the same file probes unchanged', () => {
    const missing = { path: 'GUIExport/a.lua', reason: 'GUI_FILE_MISSING' }
    expect(diskConflictAfterProbe(missing, 'GUIExport/a.lua', 'unchanged')).toBeNull()
    expect(diskConflictAfterProbe(missing, 'GUIExport/b.lua', 'unchanged')).toEqual(missing)
  })

  it('keeps a real missing result bound to the probed file', () => {
    expect(diskConflictAfterProbe(null, 'GUIExport/a.lua', 'missing')).toEqual({
      path: 'GUIExport/a.lua',
      reason: 'GUI_FILE_MISSING',
    })
  })
})

describe('gui save node merging', () => {
  it('preserves an external node when an older concurrent list response arrives', () => {
    const external = saveNode('external-2', 2, 'external')
    const older = saveNode('studio-1', 1, 'studio')
    expect(mergeSaveNodes([external], [older])).toEqual([external, older])
  })

  it('deduplicates refreshed nodes by id and keeps the incoming record', () => {
    const stale = saveNode('studio-1', 1, 'studio')
    const refreshed = { ...stale, restoredFromNodeId: 'restore-source' }
    expect(mergeSaveNodes([stale], [refreshed])).toEqual([refreshed])
  })
})

function saveNode(id: string, createdAt: number, origin: 'studio' | 'external') {
  return { id, createdAt, origin, paths: [`GUIExport/${id}.lua`] }
}
