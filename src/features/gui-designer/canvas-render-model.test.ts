import type { BoundValue, Mir3UiDocument, Mir3UiNode } from './types'
import { describe, expect, it } from 'vitest'
import { canvasPreviewGroups } from './canvas-render-model'

const VIEWPORT = { width: 1136, height: 640 }

describe('static GUI template preview placement', () => {
  it('centers the auction bidding cell without changing its source coordinates', () => {
    const root = panelNode('cell', 346, 85, 692, 96, 0.5, 1)
    const groups = canvasPreviewGroups(documentWith(root), ['cell'], VIEWPORT)

    expect(groups).toHaveLength(1)
    expect(groups[0].normalized).toBe(true)
    expect(groups[0].transform).toMatchObject({ a: 1, d: 1, e: 222, f: 283 })
    expect(root.position.x.value).toBe(346)
    expect(root.position.y.value).toBe(85)
  })

  it('keeps a viewport-sized root in authored screen coordinates', () => {
    const root = panelNode('screen', 0, 0, 1136, 640, 0, 0)
    const groups = canvasPreviewGroups(documentWith(root), ['screen'], VIEWPORT)

    expect(groups).toHaveLength(1)
    expect(groups[0].normalized).toBe(false)
    expect(groups[0].transform).toEqual({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 })
  })

  it('centers a fragment subtree separately from its full-screen sibling', () => {
    const screen = panelNode('screen', 0, 0, 1136, 640, 0, 0)
    const scene = nodeNode('scene', ['frame'])
    const frame = panelNode('frame', 0, 0, 762, 510, 0.5, 0.5, 'scene')
    const document = documentWith(screen, scene, frame)
    const groups = canvasPreviewGroups(document, ['screen', 'scene'], VIEWPORT)

    expect(groups).toHaveLength(2)
    expect(groups[0].normalized).toBe(false)
    expect(groups[1].normalized).toBe(true)
    expect(groups[1].transform).toMatchObject({ a: 1, d: 1, e: 568, f: 320 })
  })
})

function documentWith(...nodes: Mir3UiNode[]): Mir3UiDocument {
  return {
    schemaVersion: '2',
    projectId: 'fixture',
    devRelativePath: 'GUIExport/fixture.lua',
    roots: nodes.filter(node => node.parentId == null).map(node => node.id),
    nodes: Object.fromEntries(nodes.map(node => [node.id, node])),
    diagnostics: [],
  }
}

function panelNode(id: string, x: number, y: number, width: number, height: number, anchorX: number, anchorY: number, parentId?: string): Mir3UiNode {
  return {
    id,
    kind: 'Panel',
    parentId,
    children: [],
    position: { x: literal(x), y: literal(y) },
    size: { width: literal(width), height: literal(height) },
    anchor: { x: literal(anchorX), y: literal(anchorY) },
    transform: transform(),
    visible: literal(true),
    compatibility: 'supported',
  }
}

function nodeNode(id: string, children: string[]): Mir3UiNode {
  return {
    id,
    kind: 'Node',
    children,
    position: { x: literal(0), y: literal(0) },
    size: { width: literal(0), height: literal(0) },
    anchor: { x: literal(0), y: literal(0) },
    transform: transform(),
    visible: literal(true),
    compatibility: 'supported',
  }
}

function transform() {
  return {
    scaleX: literal(1),
    scaleY: literal(1),
    rotation: literal(0),
    skewX: literal(0),
    skewY: literal(0),
  }
}

function literal<T>(value: T): BoundValue<T> {
  return { value, source: 'literal', writable: true }
}
