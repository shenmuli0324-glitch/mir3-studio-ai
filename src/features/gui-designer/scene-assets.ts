import { queryOptions, useQueries, useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

export interface GuiSceneAssetCatalog {
  projectId: string
  devAvailable: boolean
  stabAvailable: boolean
  modules: Array<{ id: string, assetRootId: string, hasRes: boolean, hasAnim: boolean, hasScene: boolean }>
  acceptedAssetIds: string[]
}

export interface GuiSceneEffectResolution {
  effectId: number
  available: boolean
  source?: string | null
  atlasAssetIds: string[]
  diagnostic?: string | null
}

export interface GuiSceneLoginPreset {
  sceneId: string
  backgroundAssetId: string
  backgroundAvailable: boolean
  effects: GuiSceneEffectResolution[]
  diagnostics: string[]
}

export interface GuiSceneAtlasFrame {
  name: string
  x: number
  y: number
  width: number
  height: number
  offsetX: number
  offsetY: number
  sourceWidth: number
  sourceHeight: number
  rotated: boolean
}

export interface GuiSceneAtlasManifest {
  assetId: string
  textureAssetId: string
  textureWidth?: number | null
  textureHeight?: number | null
  frames: GuiSceneAtlasFrame[]
  sourceSha256: string
}

export interface GuiSceneWorldFrame {
  layer: 'ground' | 'bottom' | 'top'
  x: number
  y: number
  fileIndex: number
  imageIndex: number
  frameIndex: number
  atlasIndex?: number | null
  atlasAssetId?: string | null
  available: boolean
  diagnostic?: string | null
}

export interface GuiSceneWorldManifest {
  status: 'supported' | 'partial' | 'unsupported'
  chunk: {
    mapId: string
    mapWidth: number
    mapHeight: number
    x: number
    y: number
    width: number
    height: number
  }
  frames: GuiSceneWorldFrame[]
  atlasAssetIds: string[]
  diagnostics: string[]
}

export interface GuiSceneAssetBinary {
  assetId: string
  blob: Blob
  browserRenderable: boolean
  diagnostic?: string
}

export interface GuiSceneAtlasResource {
  manifest: GuiSceneAtlasManifest
  texture?: GuiSceneAssetBinary
}

export interface GuiSceneStageAssets {
  background?: GuiSceneAssetBinary
  world?: GuiSceneWorldManifest
  atlases: GuiSceneAtlasResource[]
  diagnostics: string[]
  loading: boolean
}

const WORLD_CHUNK = { x: 0, y: 0, width: 32, height: 24 } as const

export function useGuiSceneAssetCatalog(projectId?: string) {
  return useQuery({
    queryKey: ['gui-scene-asset-catalog', projectId],
    queryFn: () => invoke<GuiSceneAssetCatalog>('gui_scene_asset_catalog', { projectId }),
    enabled: projectId != null,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function useGuiSceneLoginPresets(projectId?: string) {
  return useQuery({
    queryKey: ['gui-scene-login-presets', projectId],
    queryFn: () => invoke<GuiSceneLoginPreset[]>('gui_scene_login_presets', { projectId }),
    enabled: projectId != null,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function useGuiSceneWorldManifest(projectId: string | undefined, mapId: string | null | undefined, enabled = true) {
  return useQuery({
    queryKey: ['gui-scene-world-manifest', projectId, mapId, WORLD_CHUNK],
    queryFn: () => invoke<GuiSceneWorldManifest>('gui_scene_world_manifest', { projectId, mapId, request: WORLD_CHUNK }),
    enabled: enabled && projectId != null && mapId != null,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function guiSceneAssetQueryOptions(projectId: string, assetId: string) {
  return queryOptions({
    queryKey: ['gui-scene-asset', projectId, assetId],
    queryFn: () => invoke<unknown>('gui_scene_asset_read', { projectId, assetId }).then(payload => normalizeSceneAsset(payload, assetId)),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function guiSceneAtlasQueryOptions(projectId: string, assetId: string) {
  return queryOptions({
    queryKey: ['gui-scene-atlas', projectId, assetId],
    queryFn: () => invoke<GuiSceneAtlasManifest>('gui_scene_atlas_manifest', { projectId, assetId }),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })
}

export function useGuiSceneStageAssets(projectId: string | undefined, profileId: string, mapId: string | null | undefined): GuiSceneStageAssets {
  useGuiSceneAssetCatalog(projectId)
  const loginPresets = useGuiSceneLoginPresets(projectId)
  const loginPreset = loginPresets.data?.find(preset => preset.sceneId === profileId)
  const worldEnabled = profileId === 'game-mobile' || profileId === 'game-pc'
  const world = useGuiSceneWorldManifest(projectId, mapId, worldEnabled)
  const atlasAssetIds = sceneAtlasAssetIds(loginPreset, world.data, profileId)
  const atlasQueries = useQueries({
    queries: projectId == null ? [] : atlasAssetIds.map(assetId => guiSceneAtlasQueryOptions(projectId, assetId)),
  })
  const atlasManifests = atlasQueries.flatMap(query => query.data == null ? [] : [query.data])
  const textureAssetIds = [...new Set(atlasManifests.map(manifest => manifest.textureAssetId))]
  const backgroundAssetIds = loginPreset?.backgroundAvailable === true ? [loginPreset.backgroundAssetId] : []
  const binaryAssetIds = [...backgroundAssetIds, ...textureAssetIds]
  const binaryQueries = useQueries({
    queries: projectId == null ? [] : binaryAssetIds.map(assetId => guiSceneAssetQueryOptions(projectId, assetId)),
  })
  const binaryById = Object.fromEntries(binaryQueries.flatMap(query => query.data == null ? [] : [[query.data.assetId, query.data]]))
  const atlases = atlasManifests.map(manifest => ({ manifest, texture: binaryById[manifest.textureAssetId] }))
  return {
    background: loginPreset == null ? undefined : binaryById[loginPreset.backgroundAssetId],
    world: world.data,
    atlases,
    diagnostics: sceneAssetDiagnostics(loginPreset, world.data, atlasQueries, binaryQueries, [loginPresets.error, world.error]),
    loading: loginPresets.isLoading || world.isLoading || atlasQueries.some(query => query.isLoading) || binaryQueries.some(query => query.isLoading),
  }
}

function sceneAtlasAssetIds(loginPreset: GuiSceneLoginPreset | undefined, world: GuiSceneWorldManifest | undefined, profileId: string): string[] {
  const ids = new Set<string>()
  for (const effect of loginPreset?.effects ?? []) {
    for (const assetId of effect.atlasAssetIds)
      ids.add(assetId)
  }
  for (const assetId of world?.atlasAssetIds ?? [])
    ids.add(assetId)
  for (const assetId of sceneActorAtlasIds(profileId))
    ids.add(assetId)
  return [...ids]
}

function sceneActorAtlasIds(profileId: string): string[] {
  if (profileId === 'character-create') {
    return [
      'cache://stab/anim/player/player_9999_0_0.plist',
      'cache://stab/anim/player/player_9999_1_0.plist',
    ]
  }
  if (profileId === 'character-select')
    return ['cache://stab/anim/player/player_9999_0_0.plist']
  if (profileId === 'game-mobile' || profileId === 'game-pc') {
    return [
      'cache://stab/anim/player/player_9999_0_0.plist',
      'cache://stab/anim/npc/npc_9999_0_0.plist',
      'cache://stab/anim/monster/monster_9999_0_0.plist',
    ]
  }
  return []
}

function sceneAssetDiagnostics(loginPreset: GuiSceneLoginPreset | undefined, world: GuiSceneWorldManifest | undefined, atlasQueries: Array<{ error: Error | null }>, binaryQueries: Array<{ data?: GuiSceneAssetBinary, error: Error | null }>, requestErrors: Array<Error | null>): string[] {
  const diagnostics = new Set<string>([...(loginPreset?.diagnostics ?? []), ...(world?.diagnostics ?? [])])
  if (world?.status === 'unsupported')
    diagnostics.add('GUI_SCENE_WORLD_UNSUPPORTED')
  for (const effect of loginPreset?.effects ?? []) {
    if (effect.diagnostic)
      diagnostics.add(effect.diagnostic)
  }
  for (const query of atlasQueries) {
    if (query.error)
      diagnostics.add(query.error.message)
  }
  for (const query of binaryQueries) {
    if (query.error)
      diagnostics.add(query.error.message)
    if (query.data?.diagnostic)
      diagnostics.add(query.data.diagnostic)
  }
  for (const error of requestErrors) {
    if (error)
      diagnostics.add(error.message)
  }
  return [...diagnostics]
}

function normalizeSceneAsset(payload: unknown, assetId: string): GuiSceneAssetBinary {
  const bytes = assetBytes(payload)
  const pkm = bytes.length >= 6 && new TextDecoder().decode(bytes.slice(0, 6)) === 'PKM 20'
  if (pkm) {
    return {
      assetId,
      blob: new Blob([bytes], { type: 'application/octet-stream' }),
      browserRenderable: false,
      diagnostic: `GUI_SCENE_ASSET_PKM_UNSUPPORTED: ${assetId}`,
    }
  }
  const mimeType = bytes[0] === 0xFF && bytes[1] === 0xD8 ? 'image/jpeg' : 'image/png'
  return { assetId, blob: new Blob([bytes], { type: mimeType }), browserRenderable: true }
}

function assetBytes(payload: unknown): Uint8Array {
  if (payload instanceof ArrayBuffer)
    return new Uint8Array(payload)
  if (ArrayBuffer.isView(payload))
    return new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength)
  if (Array.isArray(payload))
    return Uint8Array.from(payload.map(Number))
  throw new Error('GUI_SCENE_ASSET_PAYLOAD_INVALID')
}
