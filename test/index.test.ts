import type { Mir3UiNode } from '../src/features/gui-designer/types'
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import runtimeBaseline from '../runtime-baseline.lock.json'
import mir3Plugin from '../src-tauri/resources/mir3-core-plugin/package.json'
import { DEV_TOOL_CATEGORIES, DEV_TOOLS } from '../src/features/devtools/devtool-registry'
import { normalizeGuiAssetPayload } from '../src/features/gui-designer/api'
import { canvasRenderMode, nodeLocalMatrix, renderedNodeSize, transformMatrixPoint } from '../src/features/gui-designer/canvas-render-model'
import { MOBILE_VIEWPORT, PC_VIEWPORTS } from '../src/features/gui-designer/types'
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
    expect(STUDIO_VIEWS).toHaveLength(9)
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

  it('exposes GUI Designer after project and development tools below Harness', () => {
    expect(STUDIO_VIEWS.slice(0, 4)).toEqual(['project', 'gui-designer', 'workbench', 'devtools'])
    expect(zhCN['studio.nav.gui-designer']).toBe('GUI编辑')
    expect(enUS['studio.nav.gui-designer']).toBe('GUI Designer')
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

  it('keeps GUI Designer local with the locked device profiles', () => {
    const designerView = readFileSync(new URL('../src/views/gui-designer-view.tsx', import.meta.url), 'utf8')
    expect(isHarnessView('gui-designer')).toBe(false)
    expect(designerView).toContain('<DesignerToolbar')
    expect(designerView).toContain('<DesignerWorkspace')
    expect(MOBILE_VIEWPORT).toEqual({ width: 1136, height: 640 })
    expect(PC_VIEWPORTS).toEqual([
      { width: 1920, height: 1080 },
      { width: 1600, height: 1024 },
      { width: 1600, height: 900 },
      { width: 1440, height: 900 },
      { width: 1366, height: 768 },
      { width: 1280, height: 800 },
      { width: 1280, height: 768 },
      { width: 1152, height: 864 },
      { width: 1024, height: 768 },
      { width: 800, height: 600 },
    ])
  })

  it('applies the parent anchor to its child coordinate system', () => {
    const parent = testGuiNode('Panel', 200, 100)
    parent.position.x.value = 100
    parent.position.y.value = 100
    parent.anchor!.x.value = 0.5
    parent.anchor!.y.value = 0.5
    const origin = transformMatrixPoint(nodeLocalMatrix(parent, renderedNodeSize(parent)), { x: 0, y: 0 })
    expect(origin).toEqual({ x: 0, y: 50 })
  })

  it('applies position after rotation and scale in the local node matrix', () => {
    const node = testGuiNode('Panel', 10, 10)
    node.position.x.value = 10
    node.position.y.value = 20
    node.transform!.scaleX.value = 2
    node.transform!.scaleY.value = 2
    node.transform!.rotation.value = 90
    const point = transformMatrixPoint(nodeLocalMatrix(node, renderedNodeSize(node)), { x: 1, y: 0 })
    expect(point.x).toBeCloseTo(10)
    expect(point.y).toBeCloseTo(22)
  })

  it('chooses explicit or intrinsic image size from ignoreContentAdaptWithSize instead of taking the maximum', () => {
    const image = testGuiNode('Image', 80, 40)
    image.ignoreContentAdaptWithSize!.value = true
    expect(renderedNodeSize(image, { width: 200, height: 100 })).toEqual({ width: 80, height: 40 })
    image.ignoreContentAdaptWithSize!.value = false
    expect(renderedNodeSize(image, { width: 200, height: 100 })).toEqual({ width: 200, height: 100 })
  })

  it('uses lightweight and blocked modes for very large GUI documents', () => {
    expect(canvasRenderMode(1999)).toBe('full')
    expect(canvasRenderMode(2000)).toBe('lightweight')
    expect(canvasRenderMode(9999)).toBe('lightweight')
    expect(canvasRenderMode(10000)).toBe('blocked')
  })

  it('updates drag preview with requestAnimationFrame and only commits on pointerup', () => {
    const canvas = readFileSync(new URL('../src/features/gui-designer/designer-canvas.tsx', import.meta.url), 'utf8')
    const pointerMove = canvas.slice(canvas.indexOf('function handlePointerMove'), canvas.indexOf('function applyPendingDragFrame'))
    expect(pointerMove).toContain('requestAnimationFrame')
    expect(pointerMove).not.toContain('updateNodePosition')
    expect(pointerMove).not.toContain('setState')
    expect(canvas.match(/scope\.updateNodePosition/g)).toHaveLength(1)
  })

  it('normalizes legacy base64 and future binary GUI asset payloads', async () => {
    const legacy = await normalizeGuiAssetPayload({ logicalPath: 'res/a.png', mimeType: 'image/png', base64: 'AQID', sha256: 'legacy' }, 'res/a.png')
    const binary = await normalizeGuiAssetPayload(Uint8Array.from([1, 2, 3]).buffer, 'res/a.png')
    expect(legacy.blob.size).toBe(3)
    expect(legacy.sha256).toBe('legacy')
    expect(binary.blob.size).toBe(3)
    expect(binary.mimeType).toBe('image/png')
  })

  it('uses one persistent Harness iframe for the workbench and its settings surface', () => {
    expect(isHarnessView('workbench')).toBe(true)
    expect(isHarnessView('settings')).toBe(true)
    expect(isHarnessView('project')).toBe(false)
    expect(harnessSurfaceFor('settings')).toBe('settings')
    expect(harnessSurfaceFor('project')).toBe('workbench')
  })
})

function testGuiNode(kind: Mir3UiNode['kind'], width: number, height: number): Mir3UiNode {
  function bound<T>(value: T) {
    return { value, source: 'literal' as const, writable: true }
  }
  return {
    id: 'node',
    kind,
    children: [],
    position: { x: bound(0), y: bound(0) },
    size: { width: bound(width), height: bound(height) },
    anchor: { x: bound(0), y: bound(0) },
    transform: { scaleX: bound(1), scaleY: bound(1), rotation: bound(0), skewX: bound(0), skewY: bound(0) },
    visible: bound(true),
    ignoreContentAdaptWithSize: bound(true),
    compatibility: 'supported',
  }
}

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
