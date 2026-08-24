import type { GuiNodeKind, Mir3UiNode } from './types'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { useScope } from '@/hooks/use-scope'
import { useGuiAsset } from './api'
import { GuiDesignerScope } from './gui-designer-scope'

interface DragState {
  nodeId: string
  startClientX: number
  startClientY: number
  originX: number
  originY: number
  deltaX: number
  deltaY: number
}

export function DesignerCanvas() {
  const { t } = useTranslation()
  const scope = useScope(GuiDesignerScope)
  const [drag, setDrag] = useState<DragState | null>(null)
  const file = scope.currentFile
  const viewport = scope.viewport

  function handlePointerMove(event: React.PointerEvent<SVGSVGElement>) {
    if (!drag)
      return
    setDrag({
      ...drag,
      deltaX: (event.clientX - drag.startClientX) / scope.zoom,
      deltaY: -(event.clientY - drag.startClientY) / scope.zoom,
    })
  }

  function handlePointerUp(event: React.PointerEvent<SVGSVGElement>) {
    if (!drag)
      return
    event.currentTarget.releasePointerCapture(event.pointerId)
    scope.updateNodePosition(drag.nodeId, drag.originX + drag.deltaX, drag.originY + drag.deltaY)
    setDrag(null)
  }

  function handleDrop(event: React.DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const kind = event.dataTransfer.getData('application/x-mir3-ui-kind') as GuiNodeKind
    if (kind !== 'Panel' && kind !== 'Image' && kind !== 'Text' && kind !== 'Button')
      return
    const surface = event.currentTarget.querySelector('[data-gui-surface]')?.getBoundingClientRect()
    if (!surface)
      return
    const x = (event.clientX - surface.left) / scope.zoom
    const y = viewport.height - (event.clientY - surface.top) / scope.zoom
    scope.addNode(kind, Math.round(x), Math.round(y), true)
  }

  return (
    <div
      className="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-auto bg-[radial-gradient(circle_at_1px_1px,var(--color-line)_1px,transparent_0)] bg-[size:16px_16px] p-14"
      data-gui-canvas-container
      onDragOver={event => event.preventDefault()}
      onDrop={handleDrop}
    >
      <If cond={file == null}>
        <div className="flex max-w-sm flex-col items-center text-center">
          <span className="mb-4 grid size-12 place-items-center rounded-2xl bg-panel ring-1 ring-line text-xl text-muted">◇</span>
          <strong className="text-sm font-medium text-ink">{t('studio.gui.canvas.empty')}</strong>
          <p className="mt-2 text-xs leading-5 text-muted">{t('studio.gui.canvas.empty_desc')}</p>
        </div>
      </If>
      <If cond={file != null}>
        <div
          className="relative shrink-0 overflow-hidden bg-[#111216] shadow-[0_28px_80px_rgba(0,0,0,0.22)] ring-1 ring-white/15"
          data-gui-surface
          style={{ width: viewport.width * scope.zoom, height: viewport.height * scope.zoom }}
        >
          <svg
            className="block size-full touch-none select-none"
            viewBox={`0 0 ${viewport.width} ${viewport.height}`}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={() => setDrag(null)}
          >
            <g transform={`translate(0 ${viewport.height}) scale(1 -1)`}>
              {file?.document.roots.map(nodeId => (
                <CanvasNode
                  key={nodeId}
                  nodeId={nodeId}
                  drag={drag}
                  onPointerDown={(event, node) => {
                    if (scope.parsePending || !node.position.x.writable || !node.position.y.writable)
                      return
                    event.stopPropagation()
                    event.currentTarget.ownerSVGElement?.setPointerCapture(event.pointerId)
                    scope.setSelectedNodeId(node.id)
                    setDrag({
                      nodeId: node.id,
                      startClientX: event.clientX,
                      startClientY: event.clientY,
                      originX: node.position.x.value,
                      originY: node.position.y.value,
                      deltaX: 0,
                      deltaY: 0,
                    })
                  }}
                />
              ))}
            </g>
          </svg>
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

function CanvasNode({ nodeId, drag, onPointerDown }: {
  nodeId: string
  drag: DragState | null
  onPointerDown: (event: React.PointerEvent<SVGGElement>, node: Mir3UiNode) => void
}) {
  const scope = useScope(GuiDesignerScope)
  const node = scope.currentFile?.document.nodes[nodeId]
  const assetPath = node?.paint?.image?.value || node?.paint?.normalImage?.value
  const asset = useGuiAsset(scope.activeProject?.id, assetPath)
  const href = asset.data ? `data:${asset.data.mimeType};base64,${asset.data.base64}` : undefined
  const [intrinsicAsset, setIntrinsicAsset] = useState<{ href: string, width: number, height: number } | null>(null)

  useEffect(() => {
    if (!href)
      return
    let active = true
    const image = new window.Image()
    image.onload = () => {
      if (active)
        setIntrinsicAsset({ href, width: image.naturalWidth, height: image.naturalHeight })
    }
    image.src = href
    return () => {
      active = false
    }
  }, [href])

  if (!node || node.visible?.value === false)
    return null
  const intrinsicSize = intrinsicAsset?.href === href ? intrinsicAsset : null
  const dragging = drag?.nodeId === nodeId
  const x = node.position.x.value + (dragging ? drag.deltaX : 0)
  const y = node.position.y.value + (dragging ? drag.deltaY : 0)
  const width = renderedWidth(node, intrinsicSize?.width)
  const height = renderedHeight(node, intrinsicSize?.height)
  const paintX = -node.anchor!.x.value * width
  const paintY = -node.anchor!.y.value * height
  const opacity = Math.min(1, Math.max(0, (node.paint?.opacity?.value ?? 255) / 255))
  return (
    <g
      transform={`translate(${x} ${y})`}
      opacity={opacity}
      onPointerDown={event => onPointerDown(event, node)}
      onClick={(event) => {
        event.stopPropagation()
        scope.setSelectedNodeId(node.id)
      }}
    >
      <NodePaint node={node} href={href} width={width} height={height} />
      <If cond={scope.selectedNodeId === node.id}>
        <rect
          x={-3}
          y={paintY - 3}
          width={width + 6}
          height={height + 6}
          transform={`translate(${paintX} 0)`}
          fill="none"
          stroke="var(--color-accent)"
          strokeWidth={1.5 / scope.zoom}
          vectorEffect="non-scaling-stroke"
        />
      </If>
      {node.children.map(childId => <CanvasNode nodeId={childId} drag={drag} onPointerDown={onPointerDown} key={childId} />)}
    </g>
  )
}

function NodePaint({ node, href, width, height }: { node: Mir3UiNode, href?: string, width: number, height: number }) {
  const x = -node.anchor!.x.value * width
  const y = -node.anchor!.y.value * height
  const color = node.paint?.color?.value || '#ffffff'
  return (
    <>
      <If cond={node.kind === 'Panel'}><rect x={x} y={y} width={width} height={height} rx={3} fill={color} fillOpacity={0.08} stroke={color} strokeOpacity={0.42} /></If>
      <If cond={node.kind === 'Image' || node.kind === 'Button'}><ImagePaint node={node} href={href} x={x} y={y} width={width} height={height} /></If>
      <If cond={node.kind === 'Text'}>
        <g transform={`translate(${x} ${y + height}) scale(1 -1)`}>
          <text x="0" y="0" dominantBaseline="hanging" fill={color} fontSize={node.paint?.fontSize?.value ?? 14}>{node.paint?.text?.value || node.name?.value}</text>
        </g>
      </If>
      <If cond={node.kind === 'Node'}><circle r={4} fill="var(--color-accent)" /></If>
      <If cond={node.kind === 'Unsupported'}><rect x={x} y={y} width={width} height={height} fill="rgba(242,90,90,0.06)" stroke="var(--color-danger)" strokeDasharray="5 4" /></If>
    </>
  )
}

function ImagePaint({ node, href, x, y, width, height }: { node: Mir3UiNode, href?: string, x: number, y: number, width: number, height: number }) {
  const path = node.paint?.image?.value || node.paint?.normalImage?.value
  return (
    <>
      <rect x={x} y={y} width={width} height={height} rx={node.kind === 'Button' ? 5 : 1} fill="rgba(255,255,255,0.045)" stroke={node.kind === 'Button' ? 'rgba(103,158,254,0.65)' : 'rgba(255,255,255,0.18)'} />
      <If cond={href != null}><image href={href} width={width} height={height} preserveAspectRatio="none" transform={`translate(${x} ${y + height}) scale(1 -1)`} /></If>
      <If cond={href == null}>
        <g transform={`translate(${x + width / 2} ${y + height / 2}) scale(1 -1)`}>
          <text textAnchor="middle" dominantBaseline="middle" fill="rgba(255,255,255,0.38)" fontSize="9">{path || node.kind}</text>
        </g>
      </If>
    </>
  )
}

function renderedWidth(node: Mir3UiNode, intrinsicWidth?: number): number {
  return Math.max(node.size.width.value, intrinsicWidth ?? 0, node.kind === 'Text' ? 80 : 24)
}

function renderedHeight(node: Mir3UiNode, intrinsicHeight?: number): number {
  return Math.max(node.size.height.value, intrinsicHeight ?? 0, 24)
}
