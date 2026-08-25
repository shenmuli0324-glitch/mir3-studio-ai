import type { GuiRuntimeSceneComposition, Mir3RuntimeSceneDocument, Mir3UiNode } from './types'
import { describe, expect, it } from 'vitest'
import {
  applyRuntimeScenePatch,
  applyWorldProfile,
  defaultSceneProfile,
  GUI_SCENE_PROFILES,
  resolveProfileCatalogEntry,
  runtimeNodeWindowKind,
  sceneRootNodeIds,
} from './scene-compositor'
import { boundNumber } from './types'

describe('gui scene compositor', () => {
  it('exposes the four canonical scene presets and device defaults', () => {
    expect(GUI_SCENE_PROFILES.map(profile => profile.id)).toEqual([
      'character-create',
      'character-select',
      'game-mobile',
      'game-pc',
    ])
    expect(defaultSceneProfile('mobile')).toBe('game-mobile')
    expect(defaultSceneProfile('pc')).toBe('game-pc')
  })

  it('resolves canonical and legacy layout catalog entries', () => {
    const profile = GUI_SCENE_PROFILES[2]
    expect(resolveProfileCatalogEntry(profile, [catalogEntry('game-mobile', '')])?.id).toBe('game-mobile')
    expect(resolveProfileCatalogEntry(profile, [catalogEntry('legacy', 'GUILayout/GUIInit.lua')])?.id).toBe('legacy')
  })

  it('orders scene graph roots by layer and window z-order without duplicates', () => {
    const document = runtimeDocument({ stage: node('stage'), hud: node('hud'), bag: node('bag') }, ['hud'])
    const composition: GuiRuntimeSceneComposition = {
      ...document.runtime,
      layers: [
        { id: 'hud', rootNodeIds: ['hud'], zOrder: 200 },
        { id: 'stage', rootNodeIds: ['stage'], zOrder: 0 },
      ],
      windows: [{ id: 'bag-window', kind: 'bag', rootNodeIds: ['bag', 'hud'], modal: false, zOrder: 300, source: 'runtime' }],
    }
    expect(sceneRootNodeIds(document, composition)).toEqual(['stage', 'hud', 'bag'])
  })

  it('applies incremental node and window patches while preserving scene identity', () => {
    const document = runtimeDocument({ old: node('old'), stable: node('stable') }, ['old', 'stable'])
    const patched = applyRuntimeScenePatch(document, {
      sequence: 2,
      addedNodes: { fresh: node('fresh') },
      updatedNodes: { stable: { ...node('stable'), compatibility: 'approximate' } },
      removedNodeIds: ['old'],
      roots: ['stable', 'fresh'],
      windows: [{ id: 'team-window', kind: 'team', rootNodeIds: ['fresh'], modal: false, zOrder: 310, source: 'runtime' }],
    })
    expect(Object.keys(patched.nodes).sort()).toEqual(['fresh', 'stable'])
    expect(patched.nodes.stable.compatibility).toBe('approximate')
    expect(patched.runtime.windows[0].kind).toBe('team')
    expect(patched.runtime.profileId).toBe('game-mobile')
  })

  it('adds a world snapshot only when runtime did not provide one', () => {
    const document = runtimeDocument({}, [])
    const enriched = applyWorldProfile(document.runtime, { id: 'map', mapId: '0', backgroundAsset: 'res/map/0.jpg' })
    expect(enriched.stage.mapId).toBe('0')
    expect(enriched.stage.backgroundAsset).toBe('res/map/0.jpg')
    const preserved = applyWorldProfile({ ...enriched, stage: { ...enriched.stage, backgroundAsset: 'res/map/custom.jpg' } }, { id: 'map', backgroundAsset: 'res/map/other.jpg' })
    expect(preserved.stage.backgroundAsset).toBe('res/map/custom.jpg')
  })

  it('maps familiar HUD controls to composited windows', () => {
    expect(runtimeNodeWindowKind({ ...node('bag'), name: stringValue('Button_bag') })).toBe('bag')
    expect(runtimeNodeWindowKind({ ...node('team'), luaVariable: 'Button_zudui' })).toBe('team')
    expect(runtimeNodeWindowKind({ ...node('store'), name: stringValue('商城') })).toBe('store')
  })
})

function catalogEntry(id: string, layoutPath: string) {
  return { id, name: id, category: 'main', layoutPath, platform: 'shared' as const, compatibility: 'supported' as const }
}

function runtimeDocument(nodes: Record<string, Mir3UiNode>, roots: string[]): Mir3RuntimeSceneDocument {
  return {
    schemaVersion: 'runtime-scene-1',
    projectId: 'project',
    devRelativePath: '',
    roots,
    nodes,
    diagnostics: [],
    runtime: {
      profileId: 'game-mobile',
      device: 'mobile',
      stage: { kind: 'world', compatibility: 'approximate' },
      layers: [{ id: 'hud', rootNodeIds: roots, zOrder: 200 }],
      windows: [],
    },
  }
}

function node(id: string): Mir3UiNode {
  return {
    id,
    kind: 'Node',
    children: [],
    position: { x: boundNumber(0), y: boundNumber(0) },
    size: { width: boundNumber(10), height: boundNumber(10) },
    compatibility: 'supported',
  }
}

function stringValue(value: string) {
  return { value, source: 'literal' as const, writable: false }
}
