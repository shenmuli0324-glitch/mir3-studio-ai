// @vitest-environment happy-dom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
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

    await user.click(await screen.findByRole('button', { name: /demo\/main\.lua/i }))
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

    await user.click(await screen.findByRole('button', { name: /demo\/main\.lua/i }))
    await user.click(screen.getByRole('button', { name: 'Layers' }))
    await user.click(await screen.findByRole('button', { name: /Button_close/i }))

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
})

function renderDesigner(): void {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  render(<QueryClientProvider client={queryClient}><GuiDesignerView /></QueryClientProvider>)
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
    if (command === 'gui_document_open')
      return Promise.resolve(documentEnvelope(String(args.devRelativePath)))
    if (command === 'gui_document_reparse') {
      const request = args.request as { devRelativePath: string, workingSource: string }
      return Promise.resolve({ ...documentEnvelope(request.devRelativePath), source: request.workingSource })
    }
    if (command === 'gui_asset_read')
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
