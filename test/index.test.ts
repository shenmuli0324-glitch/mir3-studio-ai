import { describe, expect, it } from 'vitest'
import enUS from '../src/i18n/locales/en-US.json'
import zhCN from '../src/i18n/locales/zh-CN.json'
import {
  DEFAULT_STUDIO_VIEW,
  harnessSurfaceFor,
  isHarnessView,
  STUDIO_VIEWS,
  studioViewTitleKey,
} from '../src/layout/studio-types'

describe('studio shell contract', () => {
  it('starts on the project view and exposes each planned destination once', () => {
    expect(DEFAULT_STUDIO_VIEW).toBe('project')
    expect(STUDIO_VIEWS).toHaveLength(8)
    expect(new Set(STUDIO_VIEWS).size).toBe(STUDIO_VIEWS.length)
  })

  it('has a title in both locales for every navigation destination', () => {
    for (const view of STUDIO_VIEWS) {
      const key = studioViewTitleKey(view) as keyof typeof zhCN
      expect(zhCN[key]).toBeTruthy()
      expect(enUS[key]).toBeTruthy()
    }
  })

  it('keeps all Studio shell strings synchronized across locales', () => {
    const studioKeysZh = Object.keys(zhCN).filter(key => key.startsWith('studio.')).sort()
    const studioKeysEn = Object.keys(enUS).filter(key => key.startsWith('studio.')).sort()
    expect(studioKeysEn).toEqual(studioKeysZh)
  })

  it('uses the persistent Harness iframe for both workbench and settings', () => {
    expect(isHarnessView('workbench')).toBe(true)
    expect(isHarnessView('settings')).toBe(true)
    expect(isHarnessView('project')).toBe(false)
    expect(harnessSurfaceFor('settings')).toBe('settings')
    expect(harnessSurfaceFor('project')).toBe('workbench')
  })
})
