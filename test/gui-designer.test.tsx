// @vitest-environment happy-dom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { GuiDesignerScope } from '../src/features/gui-designer/gui-designer-scope'
import { GuiDesignerView } from '../src/views/gui-designer-view'
import '../src/i18n'

const mocks = vi.hoisted(() => ({
  hasActiveProject: true,
  activeProject: {
    id: 'project-1',
    name: 'Fixture',
    root: '/fixture',
    clientRoot: '/fixture/客户端',
    engineRoot: '/fixture/引擎',
    activeWorkspaceRoot: '/fixture/引擎',
    status: 'valid' as const,
    warnings: [],
    createdAt: 1,
    updatedAt: 1,
  },
  invoke: vi.fn(),
}))

vi.mock('../src/features/projects/use-mir3-projects', () => ({
  useMir3Projects: () => ({ activeProject: mocks.hasActiveProject ? mocks.activeProject : null }),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }))

vi.mock('@uiw/react-codemirror', () => ({
  default: (props: { 'aria-label'?: string, 'value'?: string, 'onChange'?: (value: string) => void }) => (
    <textarea
      aria-label={props['aria-label']}
      value={props.value}
      onChange={event => props.onChange?.(event.target.value)}
    />
  ),
}))

afterEach(() => {
  cleanup()
  mocks.hasActiveProject = true
  mocks.invoke.mockReset()
})

describe('gui designer interaction', () => {
  it('shows a project-first empty state without touching GUI files', () => {
    mocks.hasActiveProject = false
    renderDesigner()
    expect(screen.getByText('Open a 996 project first')).toBeTruthy()
    expect(mocks.invoke.mock.calls.some(([command]) => String(command).startsWith('gui_'))).toBe(false)
  })

  it('opens an editable file and keeps one working copy across device and mode switches', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await openDemoFile(user)
    expect(await screen.findByText('1136 × 640')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'PC' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_open', expect.objectContaining({ devRelativePath: 'GUIExport/demo/main_win32.lua' })))
    expect(screen.getByLabelText('PC design resolution')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Code' }))
    expect(screen.getByLabelText('Lua source editor')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Split' }))
    expect(screen.getByLabelText('Lua source editor')).toBeTruthy()
    expect(document.querySelector('[data-gui-canvas-container]')).toBeTruthy()
  })

  it('edits a literal property, adds a component, and requires Diff confirmation before apply', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await openDemoFile(user)
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /Button_close/i }))
    expect((screen.getByLabelText('Image path') as HTMLInputElement).value).toBe('icons/close.png')

    const xInput = screen.getByLabelText('X')
    fireEvent.change(xInput, { target: { value: '42' } })
    fireEvent.blur(xInput)
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_reparse', expect.anything()), { timeout: 1500 })

    const widthInput = screen.getByLabelText('W')
    fireEvent.change(widthInput, { target: { value: '120' } })
    fireEvent.blur(widthInput)
    await waitFor(() => {
      const reparseCalls = mocks.invoke.mock.calls.filter(([command]) => command === 'gui_document_reparse')
      const latestArgs = reparseCalls.at(-1)?.[1] as { request?: { workingSource?: string } } | undefined
      expect(latestArgs?.request?.workingSource).toContain('GUI:setContentSize(Button_close, 120, 40)')
    }, { timeout: 1500 })

    await user.click(screen.getByRole('button', { name: 'Components' }))
    await user.click(screen.getByRole('button', { name: 'Panel' }))
    await waitFor(() => expect((screen.getByRole('button', { name: 'Generate Diff' }) as HTMLButtonElement).disabled).toBe(false))
    await user.click(screen.getByRole('button', { name: 'Generate Diff' }))
    expect(await screen.findByText('Review source Diff')).toBeTruthy()
    expect(screen.getAllByText(/Button_close/).length).toBeGreaterThan(0)
    await user.click(screen.getByRole('button', { name: 'Confirm and apply' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_draft_apply', expect.objectContaining({ confirmationToken: 'confirm-once' })))
  })

  it('lazy-loads GUILayout and opens Lua in a read-only preview', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Files' }))
    await user.click(await screen.findByRole('button', { name: /GUILayout.*GUI runtime logic/i }))
    await user.click(await screen.findByRole('button', { name: /demo\.lua.*read-only/i }))
    expect(await screen.findByText('Read-only Lua source')).toBeTruthy()
    expect(screen.getByText('return {}')).toBeTruthy()
    expect(mocks.invoke).toHaveBeenCalledWith('gui_readonly_document_open', expect.objectContaining({ devRelativePath: 'GUILayout/demo.lua' }))
  })

  it('shows descriptions only for real DEV root directories without the official prefix', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Files' }))
    expect(await screen.findByRole('button', { name: /data_config.*Data configuration/i })).toBeTruthy()
    expect(screen.queryByText(/Official data configuration/i)).toBeNull()
    await user.click(screen.getByRole('button', { name: /scripts.*Scripts directory/i }))
    expect(await screen.findByRole('button', { name: /^game_config$/i })).toBeTruthy()
  })

  it('starts the character creation preset with its real scene background', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    expect(await screen.findByRole('button', { name: /Character creation/i })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Character selection/i })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Mobile game/i })).toBeTruthy()
    expect(screen.getByRole('button', { name: /PC game/i })).toBeTruthy()
    await user.click(screen.getByRole('button', { name: /Character creation/i }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_start', expect.objectContaining({
      projectId: 'project-1',
      request: expect.objectContaining({ sceneId: 'character-create', presetId: 'character-create' }),
    })))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_scene_asset_read', expect.objectContaining({ assetId: 'dev://res/private/login/create_bg.jpg' })))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_scene_atlas_manifest', expect.objectContaining({ assetId: 'cache://stab/anim/player/player_9999_0_0.plist' })))
    expect(document.querySelector('[data-gui-canvas-container]')).toBeTruthy()
  })

  it('selects the matching Mobile and PC maps and dispatches overlay events', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: /Mobile game/i }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_start', expect.objectContaining({
      request: expect.objectContaining({ presetId: 'game-mobile', mapId: '01', mockProfileId: 'mobile-hud' }),
    })))
    await user.click(screen.getByRole('button', { name: 'Bag' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_event', expect.objectContaining({ eventType: 'open-bag' })))
    await user.click(await screen.findByRole('button', { name: 'Close window' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_event', expect.objectContaining({ eventType: 'close-top' })))

    await user.click(screen.getByRole('button', { name: 'PC' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_start', expect.objectContaining({
      request: expect.objectContaining({ presetId: 'game-pc', mapId: '1', mockProfileId: 'pc-hud' }),
    })))
  })

  it('keeps Runtime source references and opens the matching GUIExport in Split mode', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Modules' }))
    await user.click(await screen.findByRole('button', { name: /Auction interface/i }))
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /RuntimeButton/i }))
    expect((screen.getByLabelText('Image path') as HTMLInputElement).value).toBe('icons/close.png')
    expect((screen.getByLabelText('Pressed image') as HTMLInputElement).value).toBe('icons/close_pressed.png')
    expect((screen.getByLabelText('Disabled image') as HTMLInputElement).value).toBe('icons/close_disabled.png')
    const sourceJump = await screen.findByRole('button', { name: /GUIExport\/demo\/main\.lua.*Open in Split/i })
    await user.click(sourceJump)
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_open', expect.objectContaining({ devRelativePath: 'GUIExport/demo/main.lua' })))
    expect(screen.getByLabelText('Lua source editor')).toBeTruthy()
    expect(document.querySelector('[data-gui-scene-layer="world"]')).toBeTruthy()
    fireEvent.change(screen.getByLabelText('X'), { target: { value: '36' } })
    fireEvent.blur(screen.getByLabelText('X'))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_runtime_scene_reload', expect.objectContaining({
      sessionId: 'runtime-1',
      workingSources: expect.objectContaining({ 'GUIExport/demo/main.lua': expect.stringContaining(', 36,') }),
    })), { timeout: 1500 })
  })

  it('keeps the dirty GUI working copy while another Studio menu is visible', async () => {
    installInvokeFixture()
    renderPersistentDesigner()
    const user = userEvent.setup()

    await openDemoFile(user)
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /Button_close/i }))
    fireEvent.change(screen.getByLabelText('X'), { target: { value: '77' } })
    fireEvent.blur(screen.getByLabelText('X'))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_reparse', expect.anything()), { timeout: 1500 })

    await user.click(screen.getByRole('button', { name: 'Show another menu' }))
    expect(screen.getByText('Another Studio menu')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Show GUI Designer' }))
    await user.click(screen.getByRole('button', { name: 'Code' }))
    expect((screen.getByLabelText('Lua source editor') as HTMLTextAreaElement).value).toContain(', 77,')
  })
})

function renderDesigner(): void {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  render(<QueryClientProvider client={queryClient}><GuiDesignerScope.Provider><GuiDesignerView /></GuiDesignerScope.Provider></QueryClientProvider>)
}

function renderPersistentDesigner(): void {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  render(<QueryClientProvider client={queryClient}><PersistentDesignerHost /></QueryClientProvider>)
}

function PersistentDesignerHost() {
  const [visible, setVisible] = useState(true)
  return (
    <GuiDesignerScope.Provider>
      <button type="button" onClick={() => setVisible(false)}>Show another menu</button>
      <button type="button" onClick={() => setVisible(true)}>Show GUI Designer</button>
      {visible ? <GuiDesignerView /> : <p>Another Studio menu</p>}
    </GuiDesignerScope.Provider>
  )
}

function installInvokeFixture(): void {
  mocks.invoke.mockImplementation((command: string, args: Record<string, unknown>) => {
    if (command === 'gui_designer_status')
      return Promise.resolve({ projectId: 'project-1', devRoot: '/fixture/客户端/dev', available: true, guiExportAvailable: true, resourceAvailable: true })
    if (command === 'gui_runtime_capabilities') {
      return Promise.resolve({
        available: true,
        backend: 'sidecar',
        dataSource: 'builtInMock',
        projectStaticAvailable: false,
        tables: [],
        limits: {},
        diagnostics: [],
      })
    }
    if (command === 'gui_runtime_catalog') {
      return Promise.resolve({
        presets: [
          runtimeCatalogEntry('character-create', 'Character creation', 'shared'),
          runtimeCatalogEntry('character-select', 'Character selection', 'shared'),
          runtimeCatalogEntry('game-mobile', 'Mobile game', 'mobile'),
          runtimeCatalogEntry('game-pc', 'PC game', 'pc'),
        ],
        modules: [runtimeCatalogEntry('auction', 'Auction interface', 'shared', 'GUILayout/auction.lua')],
        worldProfiles: [
          { id: 'mobile-01', name: 'Mobile', device: 'mobile', mapId: '01', mockProfileId: 'mobile-hud' },
          { id: 'pc-1', name: 'PC', device: 'pc', mapId: '1', mockProfileId: 'pc-hud' },
        ],
        scenes: [],
      })
    }
    if (command === 'gui_runtime_scene_start') {
      const request = (args.request ?? {}) as { presetId?: string, sceneId?: string, device?: string, mapId?: string }
      return Promise.resolve({
        sessionId: 'runtime-1',
        sequence: 1,
        scene: runtimeSceneFixture(request.presetId ?? request.sceneId ?? 'game-mobile', request.device, request.mapId),
        fallback: false,
        diagnostics: [],
      })
    }
    if (command === 'gui_runtime_scene_event') {
      return Promise.resolve({ sessionId: 'runtime-1', sequence: 2, scene: runtimeSceneFixture('game-mobile', 'mobile', '01'), fallback: false, diagnostics: [] })
    }
    if (command === 'gui_runtime_scene_reload') {
      return Promise.resolve({ sessionId: 'runtime-1', sequence: 2, scene: runtimeSceneFixture('game-mobile', 'mobile', '01'), fallback: false, diagnostics: [] })
    }
    if (command === 'gui_runtime_scene_stop')
      return Promise.resolve({ stopped: true })
    if (command === 'gui_runtime_data_source_set') {
      return Promise.resolve({ available: true, backend: 'sidecar', dataSource: 'builtInMock', projectStaticAvailable: false, tables: [], limits: {}, diagnostics: [] })
    }
    if (command === 'gui_scene_asset_catalog') {
      return Promise.resolve({ projectId: 'project-1', devAvailable: true, stabAvailable: false, modules: [], acceptedAssetIds: ['dev://res/'] })
    }
    if (command === 'gui_scene_login_presets') {
      return Promise.resolve([
        {
          sceneId: 'character-create',
          backgroundAssetId: 'dev://res/private/login/create_bg.jpg',
          backgroundAvailable: true,
          effects: [sceneEffect(3061), sceneEffect(3062)],
          diagnostics: [],
        },
        {
          sceneId: 'character-select',
          backgroundAssetId: 'dev://res/private/login/bg_cjzy_02.jpg',
          backgroundAvailable: true,
          effects: [],
          diagnostics: [],
        },
      ])
    }
    if (command === 'gui_scene_world_manifest') {
      return Promise.resolve({
        status: 'supported',
        chunk: { mapId: String(args.mapId), mapWidth: 256, mapHeight: 256, x: 0, y: 0, width: 32, height: 24 },
        frames: [],
        atlasAssetIds: [],
        diagnostics: [],
      })
    }
    if (command === 'gui_scene_atlas_manifest') {
      const assetId = String(args.assetId)
      return Promise.resolve({ assetId, textureAssetId: assetId.replace(/\.plist$/, '.png'), frames: [{ name: '0', x: 0, y: 0, width: 1, height: 1, offsetX: 0, offsetY: 0, sourceWidth: 1, sourceHeight: 1, rotated: false }], sourceSha256: 'atlas' })
    }
    if (command === 'gui_scene_asset_read')
      return Promise.resolve(Uint8Array.from([0xFF, 0xD8, 0xFF, 0xD9]).buffer)
    if (command === 'gui_document_list') {
      return Promise.resolve([
        { path: 'GUIExport/demo/main.lua', kind: 'editable', platform: 'mobile', peerPath: 'GUIExport/demo/main_win32.lua' },
        { path: 'GUIExport/demo/main_win32.lua', kind: 'editable', platform: 'pc', peerPath: 'GUIExport/demo/main.lua' },
        { path: 'GUILayout/demo.lua', kind: 'readonly', platform: 'shared' },
      ])
    }
    if (command === 'gui_dev_tree_list') {
      const parentPath = String(args.parentPath ?? '')
      if (parentPath === '') {
        return Promise.resolve({
          parentPath,
          entries: [treeDirectory('GUIExport', 'GUIExport'), treeDirectory('GUILayout', 'GUILayout'), treeDirectory('data_config', 'data_config'), treeDirectory('res', 'res'), treeDirectory('scripts', 'scripts')],
          nextCursor: null,
        })
      }
      if (parentPath === 'scripts')
        return Promise.resolve({ parentPath, entries: [treeDirectory('scripts/game_config', 'game_config')], nextCursor: null })
      if (parentPath === 'GUIExport') {
        return Promise.resolve({ parentPath, entries: [treeDirectory('GUIExport/demo', 'GUIExport')], nextCursor: null })
      }
      if (parentPath === 'GUIExport/demo') {
        return Promise.resolve({
          parentPath,
          entries: [
            treeFile('GUIExport/demo/main.lua', 'editable'),
            treeFile('GUIExport/demo/main_win32.lua', 'editable'),
          ],
          nextCursor: null,
        })
      }
      if (parentPath === 'GUILayout') {
        return Promise.resolve({
          parentPath,
          entries: [treeFile('GUILayout/demo.lua', 'readonly')],
          nextCursor: null,
        })
      }
      return Promise.resolve({ parentPath, entries: [], nextCursor: null })
    }
    if (command === 'gui_readonly_document_open') {
      return Promise.resolve({
        devRelativePath: 'GUILayout/demo.lua',
        source: 'return {}',
        sha256: 'readonly-sha',
        encoding: 'UTF-8',
        newline: '\n',
        readOnly: true,
      })
    }
    if (command === 'gui_document_open')
      return Promise.resolve(documentEnvelope(String(args.devRelativePath)))
    if (command === 'gui_document_reparse') {
      const request = args.request as { devRelativePath: string, workingSource: string }
      return Promise.resolve({ ...documentEnvelope(request.devRelativePath), source: request.workingSource })
    }
    if (command === 'gui_asset_read')
      return Promise.reject(new Error('GUI_ASSET_NOT_FOUND'))
    if (command === 'gui_asset_meta')
      return Promise.reject(new Error('GUI_ASSET_NOT_FOUND'))
    if (command === 'gui_draft_prepare')
      return Promise.resolve({ draftId: 'draft-1', revision: 1, preview: { changes: [] } })
    if (command === 'gui_draft_confirm') {
      return Promise.resolve({
        preview: {
          draft: { id: 'draft-1' },
          changes: [{ path: '客户端/dev/GUIExport/demo/main.lua', unifiedDiff: '@@ -1 +1 @@\n-Button_close\n+Button_close' }],
          diffHash: 'diff-hash',
        },
        confirmationToken: 'confirm-once',
      })
    }
    if (command === 'gui_draft_apply')
      return Promise.resolve({ id: 'snapshot-1', files: [] })
    return Promise.reject(new Error(`UNEXPECTED_COMMAND: ${command}`))
  })
}

function runtimeSceneFixture(profileId = 'game-mobile', device = 'mobile', mapId?: string) {
  return {
    id: 'scene-auction',
    profileId,
    viewport: { width: 1136, height: 640, scaleFactor: 1 },
    roots: ['runtime-root'],
    nodes: {
      'runtime-root': {
        id: 'runtime-root',
        nodeType: 'Scene',
        name: 'Scene',
        parentId: null,
        children: ['runtime-button'],
        transform: { x: 0, y: 0, anchorX: 0, anchorY: 0, scaleX: 1, scaleY: 1, rotation: 0 },
        size: { width: 1136, height: 640 },
        visible: true,
        sourceRef: { devRelativePath: 'GUILayout/auction.lua', line: 1, column: 0 },
        properties: {},
      },
      'runtime-button': {
        id: 'runtime-button',
        nodeType: 'Button',
        name: 'RuntimeButton',
        parentId: 'runtime-root',
        children: [],
        transform: { x: 10, y: 20, anchorX: 0, anchorY: 0, scaleX: 1, scaleY: 1, rotation: 0 },
        size: { width: 80, height: 40 },
        visible: true,
        asset: 'icons/close.png',
        assetSlots: {
          normal: 'icons/close.png',
          pressed: 'icons/close_pressed.png',
          disabled: 'icons/close_disabled.png',
        },
        sourceRef: { devRelativePath: 'GUIExport/demo/main.lua', line: 2, column: 0, templateNodeId: 'close' },
        properties: {},
      },
    },
    diagnostics: [],
    provenance: [{ kind: 'sceneMock', key: 'auction', description: 'Simulated scene data' }],
    runtime: {
      profileId,
      device,
      stage: { kind: profileId.startsWith('character-') ? 'login' : 'world', mapId, compatibility: 'approximate' },
      layers: [{ id: 'hud', rootNodeIds: ['runtime-root'], zOrder: 200 }],
      windows: [],
    },
  }
}

async function openDemoFile(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(await screen.findByRole('button', { name: 'Files' }))
  await user.click(await screen.findByRole('button', { name: /GUIExport.*Static GUI layout/i }))
  await user.click(await screen.findByRole('button', { name: /^demo$/i }))
  await user.click(await screen.findByRole('button', { name: /main\.lua.*visual editing/i }))
}

function runtimeCatalogEntry(id: string, name: string, platform: 'mobile' | 'pc' | 'shared', layoutPath = `GUILayout/${id}.lua`) {
  return { id, name, category: 'main', layoutPath, platform, compatibility: 'supported' }
}

function sceneEffect(effectId: number) {
  return { effectId, available: true, atlasAssetIds: [`cache://stab/sfx_${effectId}_0.plist`] }
}

function treeDirectory(path: string, descriptionId: string) {
  return {
    path,
    name: path.split('/').at(-1),
    entryType: 'directory',
    policy: path === 'GUILayout' ? 'readonly' : 'info',
    hidden: false,
    size: 0,
    hasChildren: true,
    descriptionId,
  }
}

function treeFile(path: string, policy: 'editable' | 'readonly' | 'asset' | 'info') {
  return {
    path,
    name: path.split('/').at(-1),
    entryType: 'file',
    policy,
    hidden: false,
    size: 64,
    hasChildren: false,
    descriptionId: 'GUIExport',
  }
}

function documentEnvelope(path: string) {
  const source = [
    'local Scene = GUI:Node_Create(parent, "Scene", 0, 0)',
    'local Button_close = GUI:Button_Create(Scene, "Button_close", 10, 20, "icons/close.png")',
    '',
  ].join('\n')
  const xStart = source.indexOf(', 10,') + 2
  const yStart = source.indexOf(', 20,') + 2
  const buttonStart = source.indexOf('GUI:Button_Create')
  return {
    devRelativePath: path,
    source,
    sha256: 'source-sha',
    encoding: 'UTF-8',
    newline: '\n',
    revision: 0,
    document: {
      schemaVersion: 1,
      source: { devRelativePath: path, sha256: 'source-sha', encoding: 'UTF-8', newline: '\n' },
      viewport: { width: 1136, height: 640 },
      roots: ['scene'],
      nodes: [
        wireNode({ id: 'scene', nodeType: 'Node', luaVariable: 'Scene', children: ['close'], insertByte: source.indexOf('\n') }),
        wireNode({
          id: 'close',
          nodeType: 'Button',
          luaVariable: 'Button_close',
          parentId: 'scene',
          name: literal('Button_close', buttonStart, buttonStart + 1),
          position: { x: literal(10, xStart, xStart + 2), y: literal(20, yStart, yStart + 2) },
          size: { width: defaultValue(0), height: defaultValue(0) },
          image: literal('icons/close.png', buttonStart, buttonStart + 1),
          insertByte: source.length - 1,
        }),
      ],
      assets: [],
      diagnostics: [],
    },
  }
}

function wireNode(input: Record<string, unknown>) {
  const insertByte = Number(input.insertByte ?? 1)
  return {
    id: input.id,
    nodeType: input.nodeType,
    parentId: input.parentId ?? null,
    children: input.children ?? [],
    luaVariable: input.luaVariable,
    name: input.name ?? literal(String(input.luaVariable), 0, 1),
    position: input.position ?? { x: literal(0, 0, 1), y: literal(0, 0, 1) },
    size: input.size ?? { width: defaultValue(0), height: defaultValue(0) },
    anchor: { x: defaultValue(0), y: defaultValue(0) },
    visible: defaultValue(true),
    text: defaultValue(''),
    image: input.image ?? defaultValue(''),
    fontSize: defaultValue(14),
    color: defaultValue('#ffffff'),
    opacity: defaultValue(255),
    tag: defaultValue(0),
    compatibility: { status: 'supported' },
    sourceBinding: {
      createCall: span(0, 1),
      statement: span(0, insertByte),
      propertySpans: {},
      insertByte,
    },
  }
}

function literal<T>(value: T, startByte: number, endByte: number) {
  return { value, source: 'literal', writable: true, originalToken: String(value), span: span(startByte, endByte) }
}

function defaultValue<T>(value: T) {
  return { value, source: 'default', writable: false, originalToken: null, span: null }
}

function span(startByte: number, endByte: number) {
  return { startByte, endByte, start: { row: 0, column: startByte }, end: { row: 0, column: endByte } }
}
