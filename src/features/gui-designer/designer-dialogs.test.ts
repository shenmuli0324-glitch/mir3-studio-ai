import { describe, expect, it } from 'vitest'
import { isValidGuiRelativePath } from './gui-relative-path'

describe('isValidGuiRelativePath', () => {
  it('accepts a safe GUIExport-relative path', () => {
    expect(isValidGuiRelativePath('custom/inventory/main')).toBe(true)
    expect(isValidGuiRelativePath('活动/每日面板')).toBe(true)
  })

  it('rejects traversal, absolute paths, reserved characters, and controls', () => {
    expect(isValidGuiRelativePath('../outside')).toBe(false)
    expect(isValidGuiRelativePath('/absolute')).toBe(false)
    expect(isValidGuiRelativePath('\\absolute')).toBe(false)
    expect(isValidGuiRelativePath('custom/panel?.lua')).toBe(false)
    expect(isValidGuiRelativePath('custom/line\nbreak')).toBe(false)
  })
})
