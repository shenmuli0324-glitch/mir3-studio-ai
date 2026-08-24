import type { Mir3UiNode } from './types'
import { useQueries } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { guiAssetQueryOptions } from './api'

const MAX_ACTIVE_ASSETS = 128
const MAX_CACHED_URLS = 256
const MAX_CACHED_BYTES = 128 * 1024 * 1024
const ASSET_CONCURRENCY = 6

interface CachedAssetUrl {
  url: string
  projectId: string
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

export function useCanvasAssets(projectId: string | undefined, nodes: Record<string, Mir3UiNode>, enabled: boolean): CanvasAssetTable {
  const paths = enabled ? uniqueAssetPaths(nodes) : []
  const activePaths = paths.slice(0, MAX_ACTIVE_ASSETS)
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
  for (let index = 0; index < requestedPaths.length; index += 1) {
    const path = requestedPaths[index]
    const asset = queries[index]?.data
    if (!asset || !projectId)
      continue
    const href = cachedBlobUrl(projectId, path, asset.sha256, asset.blob)
    if (href)
      hrefs[path] = href
    if (asset.width != null && asset.height != null)
      dimensions[path] = { width: asset.width, height: asset.height }
  }
  return { hrefs, dimensions, omitted: Math.max(0, paths.length - activePaths.length) }
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

function uniqueAssetPaths(nodes: Record<string, Mir3UiNode>): string[] {
  const paths = new Set<string>()
  for (const node of Object.values(nodes)) {
    const path = node.paint?.image?.value || node.paint?.normalImage?.value
    if (path)
      paths.add(path)
    for (const [property, bound] of Object.entries(node.properties ?? {})) {
      if (typeof bound.value !== 'string' || !/image|texture/i.test(property) || !/\.(?:png|jpe?g)$/i.test(bound.value))
        continue
      paths.add(bound.value)
    }
  }
  return [...paths]
}

function cachedBlobUrl(projectId: string, logicalPath: string, sha256: string, blob: Blob): string | undefined {
  if (typeof URL.createObjectURL !== 'function')
    return undefined
  const key = `${projectId}:${logicalPath}:${sha256}`
  const existing = blobUrlCache.get(key)
  if (existing) {
    existing.touchedAt = ++assetTouchSequence
    return existing.url
  }
  const url = URL.createObjectURL(blob)
  blobUrlCache.set(key, { url, projectId, touchedAt: ++assetTouchSequence, byteLength: blob.size })
  pruneBlobUrls()
  return url
}

function pruneBlobUrls(): void {
  let byteLength = [...blobUrlCache.values()].reduce((total, cached) => total + cached.byteLength, 0)
  if (blobUrlCache.size <= MAX_CACHED_URLS && byteLength <= MAX_CACHED_BYTES)
    return
  const ordered = [...blobUrlCache.entries()].sort((left, right) => left[1].touchedAt - right[1].touchedAt)
  for (const [key, cached] of ordered) {
    if (blobUrlCache.size <= MAX_CACHED_URLS && byteLength <= MAX_CACHED_BYTES)
      break
    URL.revokeObjectURL(cached.url)
    blobUrlCache.delete(key)
    byteLength -= cached.byteLength
  }
}
