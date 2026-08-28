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
    expect(await screen.findByLabelText('Lua source editor')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Split' }))
    expect(await screen.findByLabelText('Lua source editor')).toBeTruthy()
    expect(document.querySelector('[data-gui-canvas-container]')).toBeTruthy()
  })

  it('edits the working copy and saves one node without a Diff confirmation', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await openDemoFile(user)
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /Button_close/i }))
    expect(screen.getAllByText('Button_close').length).toBeGreaterThan(0)

    await user.click(screen.getByRole('button', { name: 'Code' }))
    const editor = await screen.findByLabelText('Lua source editor') as HTMLTextAreaElement
    fireEvent.change(editor, { target: { value: editor.value.replace(', 10,', ', 42,') } })
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_reparse', expect.anything()), { timeout: 1500 })
    await waitFor(() => expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(false))
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_working_save', expect.objectContaining({
      changeSet: expect.objectContaining({ files: [expect.objectContaining({ source: expect.stringContaining(', 42,') })] }),
    })))
    expect(screen.queryByText('Review source Diff')).toBeNull()
  })

  it('lazy-loads GUILayout and opens Lua in a read-only preview', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Files' }))
    await user.click(await screen.findByRole('button', { name: /GUILayout.*GUI behavior source/i }))
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

  it('uses only the static GUIExport document and never calls the GUI Runtime', async () => {
    installInvokeFixture()
    renderDesigner()
    const user = userEvent.setup()

    expect(screen.queryByRole('button', { name: 'Scenes' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Interact' })).toBeNull()
    await openDemoFile(user)
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /Button_close/i }))
    expect(screen.getAllByText('Button_close').length).toBeGreaterThan(0)
    expect(document.querySelector('[data-gui-node-id="close"]')).toBeTruthy()
    expect(document.querySelector('[data-gui-scene-layer]')).toBeNull()
    expect(mocks.invoke.mock.calls.some(([command]) => String(command).startsWith('gui_runtime_'))).toBe(false)
  })

  it('keeps the dirty GUI working copy while another Studio menu is visible', async () => {
    installInvokeFixture()
    renderPersistentDesigner()
    const user = userEvent.setup()

    await openDemoFile(user)
    await user.click(screen.getByRole('button', { name: 'Code' }))
    const editor = await screen.findByLabelText('Lua source editor') as HTMLTextAreaElement
    fireEvent.change(editor, { target: { value: editor.value.replace(', 10,', ', 77,') } })
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('gui_document_reparse', expect.anything()), { timeout: 1500 })

    await user.click(screen.getByRole('button', { name: 'Show another menu' }))
    expect(screen.getByText('Another Studio menu')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Show GUI Designer' }))
    await user.click(screen.getByRole('button', { name: 'Code' }))
    expect((await screen.findByLabelText('Lua source editor') as HTMLTextAreaElement).value).toContain(', 77,')
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
    if (command === 'gui_save_node_list')
      return Promise.resolve([])
    if (command === 'gui_document_probe')
      return Promise.resolve({ path: String(args.devRelativePath), exists: true, changed: false, sha256: 'source-sha' })
    if (command === 'gui_game_process_status')
      return Promise.resolve({ supported: true, executablePath: String(args.executablePath ?? ''), running: false })
    if (command === 'gui_working_save') {
      const changeSet = args.changeSet as { files: Array<{ devRelativePath: string, source: string }> }
      return Promise.resolve({
        saveNode: { id: 'save-1', projectId: 'project-1', origin: 'studio', files: changeSet.files.map(file => ({ path: file.devRelativePath })), createdAt: 1 },
        files: changeSet.files.map(file => ({ ...documentEnvelope(file.devRelativePath), source: file.source, sha256: 'saved-sha' })),
      })
    }
    if (command === 'gui_asset_read')
      return Promise.reject(new Error('GUI_ASSET_NOT_FOUND'))
    if (command === 'gui_asset_meta')
      return Promise.reject(new Error('GUI_ASSET_NOT_FOUND'))
    return Promise.reject(new Error(`UNEXPECTED_COMMAND: ${command}`))
  })
}

async function openDemoFile(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(await screen.findByRole('button', { name: 'Files' }))
  await user.click(await screen.findByRole('button', { name: /GUIExport.*Static GUI layout/i }))
  await user.click(await screen.findByRole('button', { name: /^demo$/i }))
  await user.click(await screen.findByRole('button', { name: /main\.lua.*visual editing/i }))
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
