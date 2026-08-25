import type { ReactNode } from 'react'
import type { CanvasAssetTable } from './canvas-assets'
import type { CanvasNodeSize, Matrix2D } from './canvas-render-model'
import type { Mir3UiDocument, Mir3UiNode } from './types'
import { Component, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { setGuiAssetDecodingPaused } from './api'
import { useCanvasAssets } from './canvas-assets'
import { canvasPreviewGroups, canvasRenderMode, matrixTransformValue, nodeLocalMatrix, renderedNodeSize, translatedNodeMatrix } from './canvas-render-model'
import { isGuiComponentKind, renderAssetValue } from './component-catalog'
import { GuiDesignerScope } from './gui-designer-scope'

interface DragRuntime {
  nodeId: string
  pointerId: number
  group: SVGGElement
  baseMatrix: Matrix2D
  originX: number
  originY: number
  startPoint: { x: number, y: number }
  parentInverse?: DOMMatrix
  deltaX: number
  deltaY: number
}

interface CanvasErrorBoundaryProps {
  children: ReactNode
  fallback: ReactNode
}

interface CanvasErrorBoundaryState {
  failed: boolean
}

class CanvasErrorBoundary extends Component<CanvasErrorBoundaryProps, CanvasErrorBoundaryState> {
  state = { failed: false }

  static getDerivedStateFromError(): CanvasErrorBoundaryState {
    return { failed: true }
  }

  render() {
    if (this.state.failed)
      return this.props.fallback
    return this.props.children
  }
}

export function DesignerCanvas() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const dragRef = useRef<DragRuntime | null>(null)
  const animationFrameRef = useRef<number | null>(null)
  const document = scope.previewDocument
  const nodes = document?.nodes ?? {}
  const nodeCount = Object.keys(nodes).length
  const renderMode = canvasRenderMode(nodeCount)
  const assets = useCanvasAssets(scope.activeProject?.id, nodes, renderMode === 'full', scope.selectedNodeId)
  const rootNodeIds = document?.roots ?? []
  const viewport = scope.viewport
  const previewGroups = document == null ? [] : canvasPreviewGroups(document, rootNodeIds, viewport, node => nodeSize(node, assets))
  const normalizedPreview = previewGroups.some(group => group.normalized)

  useEffect(() => {
    return () => {
      if (animationFrameRef.current != null)
        cancelAnimationFrame(animationFrameRef.current)
      setGuiAssetDecodingPaused(false)
    }
  }, [])

  function handlePointerDown(event: React.PointerEvent<SVGSVGElement>) {
    if (!document)
      return
    const group = nodeGroupFromTarget(event.target)
    if (!group)
      return
    const nodeId = group.dataset.guiNodeId
    const node = nodeId ? document.nodes[nodeId] : undefined
    if (!node)
      return
    scope.setSelectedNodeId(node.id)
    if (scope.parsePending || !scope.nodePropertyWritable(node, 'x') || !scope.nodePropertyWritable(node, 'y'))
      return
    const parentMatrix = (group.parentElement as unknown as SVGGraphicsElement | null)?.getScreenCTM()
    const parentInverse = parentMatrix?.inverse()
    const startPoint = localPointerPoint(event.clientX, event.clientY, parentInverse, scope.zoom)
    const size = nodeSize(node, assets)
    dragRef.current = {
      nodeId: node.id,
      pointerId: event.pointerId,
      group,
      baseMatrix: nodeLocalMatrix(node, size),
      originX: node.position.x.value,
      originY: node.position.y.value,
      startPoint,
      parentInverse,
      deltaX: 0,
      deltaY: 0,
    }
    setGuiAssetDecodingPaused(true)
    event.currentTarget.setPointerCapture(event.pointerId)
    event.preventDefault()
  }

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>) {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId)
      return
    const point = localPointerPoint(event.clientX, event.clientY, drag.parentInverse, scope.zoom)
    drag.deltaX = point.x - drag.startPoint.x
    drag.deltaY = point.y - drag.startPoint.y
    if (animationFrameRef.current != null)
      return
    animationFrameRef.current = requestAnimationFrame(applyPendingDragFrame)
  }

  function applyPendingDragFrame() {
    animationFrameRef.current = null
    const drag = dragRef.current
    if (!drag)
      return
    drag.group.setAttribute('transform', matrixTransformValue(translatedNodeMatrix(drag.baseMatrix, drag.deltaX, drag.deltaY)))
  }

  function handlePointerUp(event: React.PointerEvent<SVGSVGElement>) {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId)
      return
    const point = localPointerPoint(event.clientX, event.clientY, drag.parentInverse, scope.zoom)
    drag.deltaX = point.x - drag.startPoint.x
    drag.deltaY = point.y - drag.startPoint.y
    if (animationFrameRef.current != null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }
    applyPendingDragFrame()
    event.currentTarget.releasePointerCapture(event.pointerId)
    dragRef.current = null
    setGuiAssetDecodingPaused(false)
    scope.updateNodePosition(drag.nodeId, drag.originX + drag.deltaX, drag.originY + drag.deltaY)
  }

  function handlePointerCancel(event: React.PointerEvent<SVGSVGElement>) {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId)
      return
    if (animationFrameRef.current != null) {
      cancelAnimationFrame(animationFrameRef.current)
      animationFrameRef.current = null
    }
    drag.group.setAttribute('transform', matrixTransformValue(drag.baseMatrix))
    dragRef.current = null
    setGuiAssetDecodingPaused(false)
  }

  function handleDrop(event: React.DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const kind = event.dataTransfer.getData('application/x-mir3-ui-kind')
    if (!isGuiComponentKind(kind))
      return
    const selectedGroup = selectedNodeGroup(event.currentTarget, scope.selectedNodeId)
    const selectedInverse = selectedGroup?.getScreenCTM()?.inverse()
    if (selectedInverse) {
      const point = localPointerPoint(event.clientX, event.clientY, selectedInverse, scope.zoom)
      scope.addNode(kind, Math.round(point.x), Math.round(point.y))
      return
    }
    const surface = event.currentTarget.querySelector('[data-gui-surface]')?.getBoundingClientRect()
    if (!surface)
      return
    const x = (event.clientX - surface.left) / scope.zoom
    const y = viewport.height - (event.clientY - surface.top) / scope.zoom
    scope.addNode(kind, Math.round(x), Math.round(y), true)
  }

  const canvasFallback = (
    <CanvasMessage title={t('studio.gui.canvas.render_error')} description={t('studio.gui.canvas.render_error_desc')} />
  )
  return (
    <div
      className="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-auto bg-[radial-gradient(circle_at_1px_1px,var(--color-line)_1px,transparent_0)] bg-[size:16px_16px] p-14"
      data-gui-canvas-container
      onDragOver={event => event.preventDefault()}
      onDrop={handleDrop}
    >
      <If cond={document == null}>
        <CanvasMessage title={t('studio.gui.canvas.empty')} description={t('studio.gui.canvas.empty_desc')} />
      </If>
      <If cond={document != null && renderMode === 'blocked'}>
        <CanvasMessage title={t('studio.gui.canvas.too_large')} description={t('studio.gui.canvas.too_large_desc', { count: nodeCount })} />
      </If>
      <If cond={renderMode !== 'blocked'}>
        <div
          className="relative shrink-0 overflow-hidden bg-[#111216] shadow-[0_28px_80px_rgba(0,0,0,0.22)] ring-1 ring-white/15"
          data-gui-surface
          style={{ width: viewport.width * scope.zoom, height: viewport.height * scope.zoom }}
        >
          <If cond={document != null}>
            <div className="absolute inset-0">
              <CanvasErrorBoundary key={`${scope.currentFile?.path ?? 'preview'}:${document?.sourceSha256 ?? 'working'}`} fallback={canvasFallback}>
                <CanvasDocument
                  document={document!}
                  previewGroups={previewGroups}
                  assets={assets}
                  lightweight={renderMode === 'lightweight'}
                  selectedNodeId={scope.selectedNodeId}
                  viewport={viewport}
                  zoom={scope.zoom}
                  onPointerDown={handlePointerDown}
                  onPointerMove={handlePointerMove}
                  onPointerUp={handlePointerUp}
                  onPointerCancel={handlePointerCancel}
                />
              </CanvasErrorBoundary>
            </div>
          </If>
          <If cond={normalizedPreview}>
            <div className="pointer-events-none absolute right-2 top-2 rounded-md bg-black/60 px-2 py-1 text-[9px] text-white/70">{t('studio.gui.canvas.template_normalized')}</div>
          </If>
          <If cond={renderMode === 'lightweight'}>
            <div className="pointer-events-none absolute left-2 top-2 rounded-md bg-black/55 px-2 py-1 text-[9px] text-white/70">{t('studio.gui.canvas.lightweight', { count: nodeCount })}</div>
          </If>
          <div className="pointer-events-none absolute bottom-2 left-2 rounded-md bg-black/55 px-2 py-1 text-[9px] text-white/65">
            {viewport.width}
            {' '}
            ×
            {' '}
            {viewport.height}
          </div>
        </div>
      </If>
    </div>
  )
}

function CanvasDocument({ document, previewGroups, assets, lightweight, selectedNodeId, viewport, zoom, onPointerDown, onPointerMove, onPointerUp, onPointerCancel }: {
  document: Mir3UiDocument
  previewGroups: ReturnType<typeof canvasPreviewGroups>
  assets: CanvasAssetTable
  lightweight: boolean
  selectedNodeId: string | null
  viewport: { width: number, height: number }
  zoom: number
  onPointerDown: (event: React.PointerEvent<SVGSVGElement>) => void
  onPointerMove: (event: React.PointerEvent<SVGSVGElement>) => void
  onPointerUp: (event: React.PointerEvent<SVGSVGElement>) => void
  onPointerCancel: (event: React.PointerEvent<SVGSVGElement>) => void
}) {
  return (
    <svg
      className="block size-full touch-none select-none"
      viewBox={`0 0 ${viewport.width} ${viewport.height}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
    >
      <g transform={`translate(0 ${viewport.height}) scale(1 -1)`}>
        {previewGroups.map((group, groupIndex) => (
          <g transform={matrixTransformValue(group.transform)} key={`${groupIndex}:${group.rootNodeIds.join(':')}`}>
            {group.rootNodeIds.map(nodeId => (
              <CanvasNode
                key={nodeId}
                nodeId={nodeId}
                document={document}
                assets={assets}
                lightweight={lightweight}
                selectedNodeId={selectedNodeId}
                zoom={zoom}
                ancestry={new Set()}
              />
            ))}
          </g>
        ))}
      </g>
    </svg>
  )
}

function CanvasNode({ nodeId, document, assets, lightweight, selectedNodeId, zoom, ancestry }: {
  nodeId: string
  document: Mir3UiDocument
  assets: CanvasAssetTable
  lightweight: boolean
  selectedNodeId: string | null
  zoom: number
  ancestry: Set<string>
}) {
  const node = document.nodes[nodeId]
  if (!node || node.visible?.value === false || ancestry.has(nodeId))
    return null
  const nextAncestry = new Set(ancestry)
  nextAncestry.add(nodeId)
  const path = renderAssetValue(node)?.value
  const size = nodeSize(node, assets)
  const matrix = nodeLocalMatrix(node, size)
  const opacity = Math.min(1, Math.max(0, (node.paint?.opacity?.value ?? 255) / 255))
  const clipId = `gui-clip-${node.id.replace(/[^\w-]/g, '-')}`
  const visual = !isDynamicRuntimePaint(node.kind)
  const selectedVisual = selectedNodeId === node.id && visual
  return (
    <g data-gui-node-id={node.id} transform={matrixTransformValue(matrix)} opacity={opacity}>
      <If cond={node.clippingEnabled?.value === true}>
        <defs><clipPath id={clipId}><rect width={size.width} height={size.height} /></clipPath></defs>
      </If>
      <If cond={visual}><NodePaint node={node} path={path} href={lightweight || !path ? undefined : assets.hrefs[path]} width={size.width} height={size.height} lightweight={lightweight} /></If>
      <If cond={selectedVisual}>
        <rect x={-3} y={-3} width={size.width + 6} height={size.height + 6} fill="none" stroke="var(--color-accent)" strokeWidth={1.5 / zoom} vectorEffect="non-scaling-stroke" />
      </If>
      <g clipPath={node.clippingEnabled?.value === true ? `url(#${clipId})` : undefined}>
        {node.children.map(childId => (
          <CanvasNode
            key={childId}
            nodeId={childId}
            document={document}
            assets={assets}
            lightweight={lightweight}
            selectedNodeId={selectedNodeId}
            zoom={zoom}
            ancestry={nextAncestry}
          />
        ))}
      </g>
    </g>
  )
}

function NodePaint({ node, path, href, width, height, lightweight }: { node: Mir3UiNode, path?: string, href?: string, width: number, height: number, lightweight: boolean }) {
  const color = node.paint?.color?.value || '#ffffff'
  if (lightweight)
    return <rect width={Math.max(width, 2)} height={Math.max(height, 2)} fill="none" stroke={color} strokeOpacity={0.28} vectorEffect="non-scaling-stroke" />
  return (
    <>
      <If cond={path != null}><ImagePaint node={node} path={path ?? ''} href={href} width={width} height={height} /></If>
      <If cond={isContainerPaint(node.kind)}><rect width={width} height={height} rx={3} fill={color} fillOpacity={path ? 0 : 0.08} stroke={color} strokeOpacity={0.42} strokeDasharray={node.kind === 'Panel' ? undefined : '6 3'} /></If>
      <If cond={isTextPaint(node.kind)}>
        <g transform={`translate(0 ${height}) scale(1 -1)`}>
          <text x="0" y="0" dominantBaseline="hanging" fill={color} fontSize={node.paint?.fontSize?.value ?? 14}>{node.paint?.text?.value || node.name?.value}</text>
        </g>
      </If>
      <If cond={node.kind === 'Node'}><circle r={4} fill="var(--color-accent)" /></If>
      <If cond={node.kind === 'Unsupported'}><rect width={width} height={height} fill="rgba(242,90,90,0.06)" stroke="var(--color-danger)" strokeDasharray="5 4" /></If>
    </>
  )
}

function isContainerPaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Panel' || kind === 'PageView' || kind === 'ListView' || kind === 'ScrollView' || kind === 'QuickCell' || kind === 'TableView'
}

function isTextPaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Text' || kind === 'TextAtlas' || kind === 'RichText' || kind === 'ScrollText' || kind === 'TextInput' || kind === 'MenuItem'
}

function isDynamicRuntimePaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'ItemShow' || kind === 'Effect' || kind === 'UIModel' || kind === 'SpineAnim'
}

function ImagePaint({ node, path, href, width, height }: { node: Mir3UiNode, path: string, href?: string, width: number, height: number }) {
  return (
    <>
      <rect width={width} height={height} rx={node.kind === 'Button' ? 5 : 1} fill="rgba(255,255,255,0.045)" stroke={node.kind === 'Button' ? 'rgba(103,158,254,0.65)' : 'rgba(255,255,255,0.18)'} />
      <If cond={href != null}><image href={href} width={width} height={height} preserveAspectRatio="none" transform={`translate(0 ${height}) scale(1 -1)`} /></If>
      <If cond={href == null}>
        <g transform={`translate(${width / 2} ${height / 2}) scale(1 -1)`}>
          <text textAnchor="middle" dominantBaseline="middle" fill="rgba(255,255,255,0.38)" fontSize="9">{path || node.kind}</text>
        </g>
      </If>
    </>
  )
}

function CanvasMessage({ title, description }: { title: string, description: string }) {
  return (
    <div className="flex max-w-sm flex-col items-center text-center">
      <span className="mb-4 grid size-12 place-items-center rounded-2xl bg-panel ring-1 ring-line text-xl text-muted">◇</span>
      <strong className="text-sm font-medium text-ink">{title}</strong>
      <p className="mt-2 text-xs leading-5 text-muted">{description}</p>
    </div>
  )
}

function nodeSize(node: Mir3UiNode, assets: CanvasAssetTable): CanvasNodeSize {
  const path = renderAssetValue(node)?.value
  return renderedNodeSize(node, path ? assets.dimensions[path] : undefined)
}

function nodeGroupFromTarget(target: EventTarget): SVGGElement | null {
  if (!(target instanceof Element))
    return null
  return target.closest<SVGGElement>('[data-gui-node-id]')
}

function selectedNodeGroup(container: HTMLElement, nodeId: string | null): SVGGElement | null {
  if (!nodeId)
    return null
  const groups = container.querySelectorAll<SVGGElement>('[data-gui-node-id]')
  return [...groups].find(group => group.dataset.guiNodeId === nodeId) ?? null
}

function localPointerPoint(clientX: number, clientY: number, inverse: DOMMatrix | undefined, zoom: number): { x: number, y: number } {
  if (inverse) {
    const point = new DOMPoint(clientX, clientY).matrixTransform(inverse)
    return { x: point.x, y: point.y }
  }
  return { x: clientX / zoom, y: -clientY / zoom }
}
