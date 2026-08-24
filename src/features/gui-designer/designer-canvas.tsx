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
import { canvasRenderMode, matrixTransformValue, nodeLocalMatrix, renderedNodeSize, translatedNodeMatrix } from './canvas-render-model'
import { isGuiComponentKind } from './component-catalog'
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
  const file = scope.currentFile
  const document = file?.document
  const nodes = document?.nodes ?? {}
  const nodeCount = Object.keys(nodes).length
  const renderMode = canvasRenderMode(nodeCount)
  const assets = useCanvasAssets(scope.activeProject?.id, nodes, renderMode === 'full')
  const viewport = scope.viewport

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
    if (scope.parsePending || !node.position.x.writable || !node.position.y.writable)
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
      <If cond={file == null}>
        <CanvasMessage title={t('studio.gui.canvas.empty')} description={t('studio.gui.canvas.empty_desc')} />
      </If>
      <If cond={file != null && renderMode === 'blocked'}>
        <CanvasMessage title={t('studio.gui.canvas.too_large')} description={t('studio.gui.canvas.too_large_desc', { count: nodeCount })} />
      </If>
      <If cond={file != null && renderMode !== 'blocked'}>
        <div
          className="relative shrink-0 overflow-hidden bg-[#111216] shadow-[0_28px_80px_rgba(0,0,0,0.22)] ring-1 ring-white/15"
          data-gui-surface
          style={{ width: viewport.width * scope.zoom, height: viewport.height * scope.zoom }}
        >
          <CanvasErrorBoundary key={`${file?.path}:${document?.sourceSha256 ?? 'working'}`} fallback={canvasFallback}>
            <CanvasDocument
              document={document!}
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

function CanvasDocument({ document, assets, lightweight, selectedNodeId, viewport, zoom, onPointerDown, onPointerMove, onPointerUp, onPointerCancel }: {
  document: Mir3UiDocument
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
        {document.roots.map(nodeId => (
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
  const path = node.paint?.image?.value || node.paint?.normalImage?.value
  const size = nodeSize(node, assets)
  const matrix = nodeLocalMatrix(node, size)
  const opacity = Math.min(1, Math.max(0, (node.paint?.opacity?.value ?? 255) / 255))
  const clipId = `gui-clip-${node.id.replace(/[^\w-]/g, '-')}`
  return (
    <g data-gui-node-id={node.id} transform={matrixTransformValue(matrix)} opacity={opacity}>
      <If cond={node.clippingEnabled?.value === true}>
        <defs><clipPath id={clipId}><rect width={size.width} height={size.height} /></clipPath></defs>
      </If>
      <NodePaint node={node} href={lightweight || !path ? undefined : assets.hrefs[path]} width={size.width} height={size.height} lightweight={lightweight} />
      <If cond={selectedNodeId === node.id}>
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

function NodePaint({ node, href, width, height, lightweight }: { node: Mir3UiNode, href?: string, width: number, height: number, lightweight: boolean }) {
  const color = node.paint?.color?.value || '#ffffff'
  if (lightweight)
    return <rect width={Math.max(width, 2)} height={Math.max(height, 2)} fill="none" stroke={color} strokeOpacity={0.28} vectorEffect="non-scaling-stroke" />
  return (
    <>
      <If cond={isContainerPaint(node.kind)}><rect width={width} height={height} rx={3} fill={color} fillOpacity={0.08} stroke={color} strokeOpacity={0.42} strokeDasharray={node.kind === 'Panel' ? undefined : '6 3'} /></If>
      <If cond={isImagePaint(node.kind)}><ImagePaint node={node} href={href} width={width} height={height} /></If>
      <If cond={isTextPaint(node.kind)}>
        <g transform={`translate(0 ${height}) scale(1 -1)`}>
          <text x="0" y="0" dominantBaseline="hanging" fill={color} fontSize={node.paint?.fontSize?.value ?? 14}>{node.paint?.text?.value || node.name?.value}</text>
        </g>
      </If>
      <If cond={node.kind === 'Node'}><circle r={4} fill="var(--color-accent)" /></If>
      <If cond={isApproximatePaint(node.kind)}><ApproximatePaint node={node} width={width} height={height} /></If>
      <If cond={node.kind === 'Unsupported'}><rect width={width} height={height} fill="rgba(242,90,90,0.06)" stroke="var(--color-danger)" strokeDasharray="5 4" /></If>
    </>
  )
}

function ApproximatePaint({ node, width, height }: { node: Mir3UiNode, width: number, height: number }) {
  const detail = genericPreviewDetail(node)
  return (
    <>
      <rect width={width} height={height} rx={4} fill="rgba(103,158,254,0.08)" stroke="rgba(103,158,254,0.55)" strokeDasharray="5 3" />
      <g transform={`translate(${width / 2} ${height / 2}) scale(1 -1)`}>
        <text textAnchor="middle" dominantBaseline="middle" fill="rgba(255,255,255,0.72)" fontSize="10">{detail}</text>
      </g>
    </>
  )
}

function genericPreviewDetail(node: Mir3UiNode): string {
  const value = Object.values(node.properties ?? {}).find(property => typeof property.value === 'string' && property.value.length > 0)?.value
  return typeof value === 'string' ? `${node.kind} · ${value}` : node.kind
}

function isContainerPaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Panel' || kind === 'PageView' || kind === 'ListView' || kind === 'ScrollView' || kind === 'QuickCell' || kind === 'TableView'
}

function isImagePaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Image' || kind === 'Button' || kind === 'CheckBox' || kind === 'Slider' || kind === 'ProgressTimer' || kind === 'LoadingBar'
}

function isTextPaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'Text' || kind === 'TextAtlas' || kind === 'RichText' || kind === 'ScrollText' || kind === 'TextInput' || kind === 'MenuItem'
}

function isApproximatePaint(kind: Mir3UiNode['kind']): boolean {
  return kind === 'ItemShow' || kind === 'Effect' || kind === 'UIModel' || kind === 'SpineAnim'
}

function ImagePaint({ node, href, width, height }: { node: Mir3UiNode, href?: string, width: number, height: number }) {
  const path = node.paint?.image?.value || node.paint?.normalImage?.value
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
  const path = node.paint?.image?.value || node.paint?.normalImage?.value
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
