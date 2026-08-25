import type { Mir3UiDocument, Mir3UiNode } from './types'
import { componentDefinition, isContainerKind } from './component-catalog'

export type CanvasRenderMode = 'full' | 'lightweight' | 'blocked'

export interface CanvasNodeSize {
  width: number
  height: number
}

export interface Matrix2D {
  a: number
  b: number
  c: number
  d: number
  e: number
  f: number
}

export interface CanvasPreviewGroup {
  rootNodeIds: string[]
  transform: Matrix2D
  normalized: boolean
}

interface CanvasBounds {
  minX: number
  minY: number
  maxX: number
  maxY: number
}

export const LIGHTWEIGHT_NODE_THRESHOLD = 2000
export const BLOCKED_NODE_THRESHOLD = 10000

export function canvasRenderMode(nodeCount: number): CanvasRenderMode {
  if (nodeCount >= BLOCKED_NODE_THRESHOLD)
    return 'blocked'
  if (nodeCount >= LIGHTWEIGHT_NODE_THRESHOLD)
    return 'lightweight'
  return 'full'
}

export function renderedNodeSize(node: Mir3UiNode, intrinsic?: CanvasNodeSize): CanvasNodeSize {
  const explicitWidth = positive(node.size.width.value)
  const explicitHeight = positive(node.size.height.value)
  if (node.kind === 'Node')
    return { width: explicitWidth, height: explicitHeight }
  if (node.kind === 'Unsupported')
    return { width: explicitWidth || 24, height: explicitHeight || 24 }
  if (isContainerKind(node.kind)) {
    const definition = componentDefinition(node.kind)
    return { width: explicitWidth || definition.defaultWidth, height: explicitHeight || definition.defaultHeight }
  }
  if (isTextKind(node.kind)) {
    const fontSize = positive(node.paint?.fontSize?.value) || 14
    const text = node.paint?.text?.value || node.name?.value || node.kind
    return {
      width: explicitWidth || Math.max(8, Array.from(text).length * fontSize * 0.62),
      height: explicitHeight || Math.max(8, fontSize * 1.35),
    }
  }
  const respectsExplicitSize = node.ignoreContentAdaptWithSize?.value !== false
  if (respectsExplicitSize) {
    return {
      width: explicitWidth || positive(intrinsic?.width) || 24,
      height: explicitHeight || positive(intrinsic?.height) || 24,
    }
  }
  return {
    width: positive(intrinsic?.width) || explicitWidth || 24,
    height: positive(intrinsic?.height) || explicitHeight || 24,
  }
}

function isTextKind(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Text' || kind === 'TextAtlas' || kind === 'RichText' || kind === 'ScrollText' || kind === 'TextInput' || kind === 'MenuItem'
}

export function nodeLocalMatrix(node: Mir3UiNode, size: CanvasNodeSize): Matrix2D {
  const anchorX = -(node.anchor?.x.value ?? 0) * size.width
  const anchorY = -(node.anchor?.y.value ?? 0) * size.height
  const rotation = radians(node.transform?.rotation.value ?? 0)
  const skewX = Math.tan(radians(node.transform?.skewX.value ?? 0))
  const skewY = Math.tan(radians(node.transform?.skewY.value ?? 0))
  const scaleX = node.transform?.scaleX.value ?? 1
  const scaleY = node.transform?.scaleY.value ?? 1
  const position = translationMatrix(node.position.x.value, node.position.y.value)
  const rotate = rotationMatrix(rotation)
  const skew = multiplyMatrices(
    { a: 1, b: 0, c: skewX, d: 1, e: 0, f: 0 },
    { a: 1, b: skewY, c: 0, d: 1, e: 0, f: 0 },
  )
  const scale = { a: scaleX, b: 0, c: 0, d: scaleY, e: 0, f: 0 }
  const anchor = translationMatrix(anchorX, anchorY)
  return multiplyMatrices(position, multiplyMatrices(rotate, multiplyMatrices(skew, multiplyMatrices(scale, anchor))))
}

export function matrixTransformValue(matrix: Matrix2D): string {
  return `matrix(${format(matrix.a)} ${format(matrix.b)} ${format(matrix.c)} ${format(matrix.d)} ${format(matrix.e)} ${format(matrix.f)})`
}

export function transformMatrixPoint(matrix: Matrix2D, point: { x: number, y: number }): { x: number, y: number } {
  return {
    x: matrix.a * point.x + matrix.c * point.y + matrix.e,
    y: matrix.b * point.x + matrix.d * point.y + matrix.f,
  }
}

export function translatedNodeMatrix(matrix: Matrix2D, deltaX: number, deltaY: number): Matrix2D {
  return { ...matrix, e: matrix.e + deltaX, f: matrix.f + deltaY }
}

export function canvasPreviewGroups(
  document: Mir3UiDocument,
  rootNodeIds: string[],
  viewport: { width: number, height: number },
  resolveSize: (node: Mir3UiNode) => CanvasNodeSize = node => renderedNodeSize(node),
): CanvasPreviewGroup[] {
  const viewportRoots: string[] = []
  const templateRoots: string[] = []
  for (const rootNodeId of rootNodeIds) {
    const root = document.nodes[rootNodeId]
    const size = root ? resolveSize(root) : { width: 0, height: 0 }
    if (root && isContainerKind(root.kind) && size.width >= viewport.width * 0.9 && size.height >= viewport.height * 0.9)
      viewportRoots.push(rootNodeId)
    else
      templateRoots.push(rootNodeId)
  }

  const groups = viewportRoots.map(rootNodeId => previewGroup(document, [rootNodeId], viewport, resolveSize))
  if (templateRoots.length > 0)
    groups.push(previewGroup(document, templateRoots, viewport, resolveSize))
  return groups
}

function translationMatrix(x: number, y: number): Matrix2D {
  return { a: 1, b: 0, c: 0, d: 1, e: x, f: y }
}

function rotationMatrix(angle: number): Matrix2D {
  const cosine = Math.cos(angle)
  const sine = Math.sin(angle)
  return { a: cosine, b: sine, c: -sine, d: cosine, e: 0, f: 0 }
}

export function multiplyMatrices(left: Matrix2D, right: Matrix2D): Matrix2D {
  return {
    a: left.a * right.a + left.c * right.b,
    b: left.b * right.a + left.d * right.b,
    c: left.a * right.c + left.c * right.d,
    d: left.b * right.c + left.d * right.d,
    e: left.a * right.e + left.c * right.f + left.e,
    f: left.b * right.e + left.d * right.f + left.f,
  }
}

function previewGroup(
  document: Mir3UiDocument,
  rootNodeIds: string[],
  viewport: { width: number, height: number },
  resolveSize: (node: Mir3UiNode) => CanvasNodeSize,
): CanvasPreviewGroup {
  const bounds = subtreeBounds(document, rootNodeIds, resolveSize)
  if (!bounds)
    return { rootNodeIds, transform: identityMatrix(), normalized: false }
  const width = Math.max(1, bounds.maxX - bounds.minX)
  const height = Math.max(1, bounds.maxY - bounds.minY)
  const fillsViewport = width >= viewport.width * 0.9 && height >= viewport.height * 0.9
  const fitsViewport = bounds.minX >= -0.5 && bounds.minY >= -0.5 && bounds.maxX <= viewport.width + 0.5 && bounds.maxY <= viewport.height + 0.5
  if (fillsViewport && fitsViewport)
    return { rootNodeIds, transform: identityMatrix(), normalized: false }

  const padding = Math.min(24, viewport.width * 0.04, viewport.height * 0.04)
  const scale = Math.min(1, (viewport.width - padding * 2) / width, (viewport.height - padding * 2) / height)
  const targetX = (viewport.width - width * scale) / 2
  const targetY = (viewport.height - height * scale) / 2
  const transform = {
    a: scale,
    b: 0,
    c: 0,
    d: scale,
    e: targetX - bounds.minX * scale,
    f: targetY - bounds.minY * scale,
  }
  return { rootNodeIds, transform, normalized: !matrixIsIdentity(transform) }
}

function subtreeBounds(
  document: Mir3UiDocument,
  rootNodeIds: string[],
  resolveSize: (node: Mir3UiNode) => CanvasNodeSize,
): CanvasBounds | null {
  let result: CanvasBounds | null = null
  for (const rootNodeId of rootNodeIds)
    result = visitNodeBounds(document, rootNodeId, identityMatrix(), resolveSize, new Set(), result)
  return result
}

function visitNodeBounds(
  document: Mir3UiDocument,
  nodeId: string,
  parentMatrix: Matrix2D,
  resolveSize: (node: Mir3UiNode) => CanvasNodeSize,
  ancestry: Set<string>,
  bounds: CanvasBounds | null,
): CanvasBounds | null {
  const node = document.nodes[nodeId]
  if (!node || node.visible?.value === false || ancestry.has(nodeId))
    return bounds
  const size = resolveSize(node)
  const matrix = multiplyMatrices(parentMatrix, nodeLocalMatrix(node, size))
  let nextBounds = size.width > 0 && size.height > 0 ? mergeBounds(bounds, transformedBounds(matrix, size)) : bounds
  const nextAncestry = new Set(ancestry)
  nextAncestry.add(nodeId)
  for (const childId of node.children)
    nextBounds = visitNodeBounds(document, childId, matrix, resolveSize, nextAncestry, nextBounds)
  return nextBounds
}

function transformedBounds(matrix: Matrix2D, size: CanvasNodeSize): CanvasBounds {
  const points = [
    transformMatrixPoint(matrix, { x: 0, y: 0 }),
    transformMatrixPoint(matrix, { x: size.width, y: 0 }),
    transformMatrixPoint(matrix, { x: 0, y: size.height }),
    transformMatrixPoint(matrix, { x: size.width, y: size.height }),
  ]
  return {
    minX: Math.min(...points.map(point => point.x)),
    minY: Math.min(...points.map(point => point.y)),
    maxX: Math.max(...points.map(point => point.x)),
    maxY: Math.max(...points.map(point => point.y)),
  }
}

function mergeBounds(left: CanvasBounds | null, right: CanvasBounds): CanvasBounds {
  if (!left)
    return right
  return {
    minX: Math.min(left.minX, right.minX),
    minY: Math.min(left.minY, right.minY),
    maxX: Math.max(left.maxX, right.maxX),
    maxY: Math.max(left.maxY, right.maxY),
  }
}

function identityMatrix(): Matrix2D {
  return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }
}

function matrixIsIdentity(matrix: Matrix2D): boolean {
  return Math.abs(matrix.a - 1) < 0.000001
    && Math.abs(matrix.b) < 0.000001
    && Math.abs(matrix.c) < 0.000001
    && Math.abs(matrix.d - 1) < 0.000001
    && Math.abs(matrix.e) < 0.5
    && Math.abs(matrix.f) < 0.5
}

function radians(degrees: number): number {
  return degrees * Math.PI / 180
}

function positive(value: number | undefined): number {
  return value != null && value > 0 ? value : 0
}

function format(value: number): string {
  const normalized = Math.abs(value) < 0.000001 ? 0 : value
  return String(Math.round(normalized * 1000000) / 1000000)
}
