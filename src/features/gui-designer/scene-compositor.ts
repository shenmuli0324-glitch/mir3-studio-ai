import type {
  GuiDevice,
  GuiRuntimeSceneCatalogEntry,
  GuiRuntimeSceneComposition,
  GuiRuntimeScenePatch,
  GuiRuntimeSceneProfileId,
  GuiRuntimeStage,
  GuiRuntimeWindow,
  GuiRuntimeWindowKind,
  Mir3RuntimeSceneDocument,
  Mir3UiDocument,
  Mir3UiNode,
} from './types'

export interface GuiSceneProfile {
  id: GuiRuntimeSceneProfileId
  device: GuiDevice
  titleKey: string
  descriptionKey: string
  stage: GuiRuntimeStage
  catalogIds: readonly string[]
  layoutSuffixes: readonly string[]
}

export const GUI_SCENE_PROFILES: readonly GuiSceneProfile[] = [
  {
    id: 'character-create',
    device: 'mobile',
    titleKey: 'studio.gui.scene.role_create',
    descriptionKey: 'studio.gui.scene.role_create.description',
    stage: { kind: 'login', compatibility: 'approximate' },
    catalogIds: ['character-create', 'role-create', 'login-role-create', 'login/LoginRolePanel'],
    layoutSuffixes: ['GUILayout/login/LoginRoleCreate.lua', 'GUILayout/login/LoginRolePanel.lua'],
  },
  {
    id: 'character-select',
    device: 'mobile',
    titleKey: 'studio.gui.scene.role_select',
    descriptionKey: 'studio.gui.scene.role_select.description',
    stage: { kind: 'login', compatibility: 'approximate' },
    catalogIds: ['character-select', 'role-select', 'login-role-select', 'login/LoginRolePanel'],
    layoutSuffixes: ['GUILayout/login/LoginRolePanel.lua'],
  },
  {
    id: 'game-mobile',
    device: 'mobile',
    titleKey: 'studio.gui.scene.hud_mobile',
    descriptionKey: 'studio.gui.scene.hud_mobile.description',
    stage: { kind: 'snapshot', compatibility: 'approximate' },
    catalogIds: ['game-mobile', 'hud-mobile', 'main-mobile', 'GUIInit'],
    layoutSuffixes: ['GUILayout/GUIInit.lua'],
  },
  {
    id: 'game-pc',
    device: 'pc',
    titleKey: 'studio.gui.scene.hud_pc',
    descriptionKey: 'studio.gui.scene.hud_pc.description',
    stage: { kind: 'snapshot', compatibility: 'approximate' },
    catalogIds: ['game-pc', 'hud-pc', 'main-pc', 'GUIInit_win32'],
    layoutSuffixes: ['GUILayout/GUIInit_win32.lua', 'GUILayout/GUIInit.lua'],
  },
]

export const GUI_SCENE_I18N_KEYS = [
  'studio.gui.scene.role_create',
  'studio.gui.scene.role_create.description',
  'studio.gui.scene.role_select',
  'studio.gui.scene.role_select.description',
  'studio.gui.scene.hud_mobile',
  'studio.gui.scene.hud_mobile.description',
  'studio.gui.scene.hud_pc',
  'studio.gui.scene.hud_pc.description',
  'studio.gui.scene.static_available',
  'studio.gui.scene.partial_available',
  'studio.gui.scene.load_failed',
  'studio.gui.interaction.design',
  'studio.gui.interaction.interact',
  'studio.gui.interaction.alt_hint',
  'studio.gui.scene.window.bag',
  'studio.gui.scene.window.team',
  'studio.gui.scene.window.store',
  'studio.gui.scene.window.close',
  'studio.gui.scene.window.fallback',
] as const

export function sceneProfile(profileId: GuiRuntimeSceneProfileId): GuiSceneProfile {
  return GUI_SCENE_PROFILES.find(profile => profile.id === profileId) ?? GUI_SCENE_PROFILES[2]
}

export function defaultSceneProfile(device: GuiDevice): GuiRuntimeSceneProfileId {
  return device === 'pc' ? 'game-pc' : 'game-mobile'
}

export function resolveProfileCatalogEntry(profile: GuiSceneProfile, entries: readonly GuiRuntimeSceneCatalogEntry[]): GuiRuntimeSceneCatalogEntry | undefined {
  return entries.find(entry => profile.catalogIds.includes(entry.id))
    ?? entries.find(entry => profile.layoutSuffixes.some(suffix => normalizedPath(entry.layoutPath).endsWith(normalizedPath(suffix))))
}

export function runtimeSceneDocument(document: Mir3UiDocument, composition: GuiRuntimeSceneComposition): Mir3RuntimeSceneDocument {
  return { ...document, runtime: composition }
}

export function sceneComposition(document: Mir3UiDocument | null | undefined, profile: GuiSceneProfile, device: GuiDevice, localWindows: GuiRuntimeWindow[]): GuiRuntimeSceneComposition {
  if (isRuntimeSceneDocument(document)) {
    return {
      ...document.runtime,
      profileId: profile.id,
      device,
      windows: mergeWindows(document.runtime.windows, localWindows),
    }
  }
  return {
    profileId: profile.id,
    device,
    stage: profile.stage,
    layers: defaultLayers(document),
    windows: localWindows,
  }
}

export function sceneRootNodeIds(document: Mir3UiDocument, composition: GuiRuntimeSceneComposition): string[] {
  const ordered = [...composition.layers]
    .sort((left, right) => left.zOrder - right.zOrder)
    .flatMap(layer => layer.rootNodeIds)
  const windowRoots = [...composition.windows]
    .sort((left, right) => left.zOrder - right.zOrder)
    .flatMap(window => window.rootNodeIds)
  let rootIds = document.roots
  if (ordered.length > 0 || windowRoots.length > 0)
    rootIds = [...ordered, ...windowRoots]
  return [...new Set(rootIds)].filter(nodeId => document.nodes[nodeId] != null)
}

export function applyRuntimeScenePatch(document: Mir3RuntimeSceneDocument, patch: GuiRuntimeScenePatch): Mir3RuntimeSceneDocument {
  const nodes = { ...document.nodes, ...patch.addedNodes, ...patch.updatedNodes }
  for (const nodeId of patch.removedNodeIds ?? [])
    delete nodes[nodeId]
  return {
    ...document,
    roots: patch.roots ?? document.roots,
    nodes,
    diagnostics: patch.diagnostics ?? document.diagnostics,
    provenance: patch.provenance ?? document.provenance,
    runtime: {
      ...document.runtime,
      stage: patch.stage ?? document.runtime.stage,
      layers: patch.layers ?? document.runtime.layers,
      windows: patch.windows ?? document.runtime.windows,
    },
  }
}

export function isRuntimeSceneDocument(document: Mir3UiDocument | null | undefined): document is Mir3RuntimeSceneDocument {
  return document != null && 'runtime' in document
}

export function fallbackWindow(kind: GuiRuntimeWindowKind, index: number): GuiRuntimeWindow {
  return {
    id: `local-${kind}`,
    kind,
    titleKey: `studio.gui.scene.window.${kind}`,
    rootNodeIds: [],
    modal: false,
    zOrder: 1_000 + index,
    source: 'localFallback',
  }
}

export function runtimeNodeWindowKind(node: Mir3UiNode): GuiRuntimeWindowKind | undefined {
  const identity = `${node.name?.value ?? ''} ${node.luaVariable ?? ''}`.toLowerCase()
  if (/bag|backpack|beibao|背包/.test(identity))
    return 'bag'
  if (/team|group|zudui|组队/.test(identity))
    return 'team'
  if (/store|shop|mall|shangcheng|商城/.test(identity))
    return 'store'
  return undefined
}

function defaultLayers(document: Mir3UiDocument | null | undefined) {
  return [
    { id: 'stage' as const, rootNodeIds: [], zOrder: 0 },
    { id: 'world' as const, rootNodeIds: [], zOrder: 100 },
    { id: 'hud' as const, rootNodeIds: document?.roots ?? [], zOrder: 200 },
    { id: 'windows' as const, rootNodeIds: [], zOrder: 300 },
  ]
}

function mergeWindows(runtimeWindows: GuiRuntimeWindow[], localWindows: GuiRuntimeWindow[]): GuiRuntimeWindow[] {
  const runtimeKinds = new Set(runtimeWindows.map(window => window.kind))
  return [...runtimeWindows, ...localWindows.filter(window => !runtimeKinds.has(window.kind))]
}

function normalizedPath(path: string): string {
  return path.replaceAll('\\', '/').toLowerCase()
}
