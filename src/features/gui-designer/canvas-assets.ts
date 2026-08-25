import type { Mir3UiNode } from './types'
import { useQueries } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { guiAssetQueryOptions } from './api'
import { componentDefinition, nodeAssetValue, renderAssetValue } from './component-catalog'

const MAX_CACHED_URLS = 256
const MAX_CACHED_BYTES = 128 * 1024 * 1024
const ASSET_CONCURRENCY = 6

interface CachedAssetUrl {
  url: string
  projectId: string
  logicalPath: string
  touchedAt: number
  byteLength: number
}

export interface CanvasAssetTable {
  hrefs: Record<string, string>
  dimensions: Record<string, { width: number, height: number }>
  omitted: number
}

const blobUrlCache = new Map<string, CachedAssetUrl>()
const projectReleaseTimers = new Map<string, number>()
let assetTouchSequence = 0

export function useCanvasAssets(projectId: string | undefined, nodes: Record<string, Mir3UiNode>, enabled: boolean, selectedNodeId?: string | null, extraPaths: readonly string[] = []): CanvasAssetTable {
  const paths = enabled ? prioritizedAssetPaths(nodes, selectedNodeId, extraPaths) : []
  const activePaths = paths
  const assetKey = `${projectId ?? ''}:${activePaths.join('|')}`
  const [requestWindow, setRequestWindow] = useState({ key: assetKey, count: ASSET_CONCURRENCY })
  const requestedCount = requestWindow.key === assetKey ? requestWindow.count : ASSET_CONCURRENCY
  const requestedPaths = activePaths.slice(0, requestedCount)
  const queries = useQueries({
    queries: projectId
      ? requestedPaths.map(path => guiAssetQueryOptions(projectId, path))
      : [],
  })
  useEffect(() => {
    if (requestedCount >= activePaths.length || queries.some(query => query.isPending))
      return
    const nextCount = Math.min(activePaths.length, requestedCount + ASSET_CONCURRENCY)
    queueMicrotask(() => setRequestWindow({ key: assetKey, count: nextCount }))
  }, [activePaths.length, assetKey, queries, requestedCount])
  useEffect(() => {
    if (!projectId)
      return
    const pendingRelease = projectReleaseTimers.get(projectId)
    if (pendingRelease != null) {
      window.clearTimeout(pendingRelease)
      projectReleaseTimers.delete(projectId)
    }
    return () => {
      scheduleProjectAssetRelease(projectId)
    }
  }, [projectId])
  const hrefs: Record<string, string> = {}
  const dimensions: Record<string, { width: number, height: number }> = {}
  const protectedPaths = new Set(requestedPaths)
  for (let index = 0; index < requestedPaths.length; index += 1) {
    const path = requestedPaths[index]
    const asset = queries[index]?.data
    if (!asset || !projectId)
      continue
    const href = cachedBlobUrl(projectId, path, asset.sha256, asset.blob, protectedPaths)
    if (href)
      hrefs[path] = href
    if (asset.width != null && asset.height != null)
      dimensions[path] = { width: asset.width, height: asset.height }
  }
  return { hrefs, dimensions, omitted: 0 }
}

export function clearCanvasAssetUrlCache(): void {
  for (const timer of projectReleaseTimers.values())
    window.clearTimeout(timer)
  projectReleaseTimers.clear()
  for (const cached of blobUrlCache.values())
    URL.revokeObjectURL(cached.url)
  blobUrlCache.clear()
}

function releaseProjectAssetUrls(projectId: string): void {
  projectReleaseTimers.delete(projectId)
  for (const [key, cached] of blobUrlCache) {
    if (cached.projectId !== projectId)
      continue
    URL.revokeObjectURL(cached.url)
    blobUrlCache.delete(key)
  }
}

function scheduleProjectAssetRelease(projectId: string): void {
  const timer = window.setTimeout(releaseProjectAssetUrls, 30000, projectId)
  projectReleaseTimers.set(projectId, timer)
}

function prioritizedAssetPaths(nodes: Record<string, Mir3UiNode>, selectedNodeId?: string | null, extraPaths: readonly string[] = []): string[] {
  const paths = new Set<string>()
  for (const path of extraPaths) {
    if (isRasterAsset(path))
      paths.add(path)
  }
  const selected = selectedNodeId ? nodes[selectedNodeId] : undefined
  if (selected)
    addNodeAssets(paths, selected, false)
  for (const node of Object.values(nodes))
    addRenderableAsset(paths, node)
  for (const node of Object.values(nodes)) {
    if (node.id !== selectedNodeId)
      addNodeAssets(paths, node, true)
  }
  return [...paths]
}

function addRenderableAsset(paths: Set<string>, node: Mir3UiNode): void {
  const path = renderAssetValue(node)?.value
  if (path && isRasterAsset(path))
    paths.add(path)
}

function addNodeAssets(paths: Set<string>, node: Mir3UiNode, secondaryOnly: boolean): void {
  if (node.kind === 'Unsupported')
    return
  for (const slot of componentDefinition(node.kind).assetSlots) {
    if (secondaryOnly && slot.render)
      continue
    const path = nodeAssetValue(node, slot.property)?.value
    if (path && isRasterAsset(path))
      paths.add(path)
  }
}

function isRasterAsset(path: string): boolean {
  return /\.(?:png|jpe?g)$/i.test(path)
}

function cachedBlobUrl(projectId: string, logicalPath: string, sha256: string, blob: Blob, protectedPaths: Set<string>): string | undefined {
  if (typeof URL.createObjectURL !== 'function')
    return undefined
  const key = `${projectId}:${logicalPath}:${sha256}`
  const existing = blobUrlCache.get(key)
  if (existing) {
    existing.touchedAt = ++assetTouchSequence
    return existing.url
  }
  const url = URL.createObjectURL(blob)
  blobUrlCache.set(key, { url, projectId, logicalPath, touchedAt: ++assetTouchSequence, byteLength: blob.size })
  pruneBlobUrls(projectId, protectedPaths)
  return url
}

function pruneBlobUrls(projectId: string, protectedPaths: Set<string>): void {
  let byteLength = [...blobUrlCache.values()].reduce((total, cached) => total + cached.byteLength, 0)
  if (blobUrlCache.size <= MAX_CACHED_URLS && byteLength <= MAX_CACHED_BYTES)
    return
  const ordered = [...blobUrlCache.entries()].sort((left, right) => left[1].touchedAt - right[1].touchedAt)
  for (const [key, cached] of ordered) {
    if (blobUrlCache.size <= MAX_CACHED_URLS && byteLength <= MAX_CACHED_BYTES)
      break
    if (cached.projectId === projectId && protectedPaths.has(cached.logicalPath))
      continue
    URL.revokeObjectURL(cached.url)
    blobUrlCache.delete(key)
    byteLength -= cached.byteLength
  }
}
