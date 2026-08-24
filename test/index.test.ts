import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import runtimeBaseline from '../runtime-baseline.lock.json'
import mir3Plugin from '../src-tauri/resources/mir3-core-plugin/package.json'
import { DEV_TOOL_CATEGORIES, DEV_TOOLS } from '../src/features/devtools/devtool-registry'
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

  it('exposes the development tools directly below the Harness workbench', () => {
    expect(STUDIO_VIEWS.slice(0, 3)).toEqual(['project', 'workbench', 'devtools'])
    expect(zhCN['studio.nav.devtools']).toBe('开发工具')
    expect(enUS['studio.nav.devtools']).toBe('Development tools')
    expect('studio.nav.knowledge' in zhCN).toBe(false)
    expect('studio.nav.knowledge' in enUS).toBe(false)
  })

  it('registers 33 unique development systems and keeps cross-server last', () => {
    expect(DEV_TOOLS).toHaveLength(33)
    expect(new Set(DEV_TOOLS.map(tool => tool.id)).size).toBe(DEV_TOOLS.length)
    expect(new Set(DEV_TOOLS.map(tool => tool.order)).size).toBe(DEV_TOOLS.length)
    expect(DEV_TOOLS.at(-1)).toMatchObject({ id: 'cross_server', order: 33, category: 'extension' })
    expect(DEV_TOOLS[0]).toMatchObject({ id: 'map', order: 1, status: 'developing' })
    for (const tool of DEV_TOOLS) {
      expect(DEV_TOOL_CATEGORIES).toContain(tool.category)
      expect(zhCN[`studio.devtools.tool.${tool.id}.title` as keyof typeof zhCN]).toBeTruthy()
      expect(enUS[`studio.devtools.tool.${tool.id}.title` as keyof typeof enUS]).toBeTruthy()
      expect(zhCN[`studio.devtools.tool.${tool.id}.description` as keyof typeof zhCN]).toBeTruthy()
      expect(enUS[`studio.devtools.tool.${tool.id}.description` as keyof typeof enUS]).toBeTruthy()
    }
  })

  it('keeps the development tool pages inside Studio instead of Harness', () => {
    const devToolsView = readFileSync(new URL('../src/views/devtools-view.tsx', import.meta.url), 'utf8')
    expect(isHarnessView('devtools')).toBe(false)
    expect(devToolsView).toContain('<DevToolsCatalog')
    expect(devToolsView).toContain('<MapToolView')
    expect(devToolsView).toContain('<PlannedToolView')
  })

  it('uses one persistent Harness iframe for the workbench and its settings surface', () => {
    expect(isHarnessView('workbench')).toBe(true)
    expect(isHarnessView('settings')).toBe(true)
    expect(isHarnessView('project')).toBe(false)
    expect(harnessSurfaceFor('settings')).toBe('settings')
    expect(harnessSurfaceFor('project')).toBe('workbench')
  })
})

describe('runtime baseline promotion contract', () => {
  it('locks one checksummed runtime set per supported target', () => {
    const targets = Object.values(runtimeBaseline.platforms).map(platform => platform.target)
    expect(new Set(targets).size).toBe(targets.length)
    expect(targets).toEqual(expect.arrayContaining([
      'x86_64-pc-windows-msvc',
      'aarch64-apple-darwin',
      'x86_64-apple-darwin',
      'x86_64-unknown-linux-gnu',
    ]))
    expect(runtimeBaseline.policy.requireApprovedPlatformForRelease).toBe(true)
    for (const platform of Object.values(runtimeBaseline.platforms)) {
      expect(['testing', 'approved']).toContain(platform.validation)
      expect(platform.node.sha256).toMatch(/^[a-f0-9]{64}$/)
      expect(platform.core.sha256).toMatch(/^[a-f0-9]{64}$/)
      expect(platform.core.url).toContain(runtimeBaseline.core.tag)
    }
    expect(runtimeBaseline.pnpm.sha256).toMatch(/^[a-f0-9]{64}$/)
  })
})

describe('bundled Harness plugin contract', () => {
  it('ships versioned local documentation and synchronized UI strings', () => {
    const changelog = readFileSync(new URL('../src-tauri/resources/mir3-core-plugin/CHANGELOG.md', import.meta.url), 'utf8')
    expect(mir3Plugin.version).toMatch(/^\d+\.\d+\.\d+$/)
    expect(changelog).toContain(`## ${mir3Plugin.version}`)
    expect(mir3Plugin.files).toEqual(expect.arrayContaining(['lib', 'README.md', 'CHANGELOG.md']))

    const pluginKeysZh = Object.keys(zhCN).filter(key => key.startsWith('plugins.')).sort()
    const pluginKeysEn = Object.keys(enUS).filter(key => key.startsWith('plugins.')).sort()
    expect(pluginKeysEn).toEqual(pluginKeysZh)
  })
})
