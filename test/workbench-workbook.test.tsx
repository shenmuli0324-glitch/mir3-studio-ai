// @vitest-environment happy-dom

import type { Mir3Project } from '../src/features/projects/types'
import type { Mir3BridgeEnvelope } from '../src/features/projects/workspace-bridge'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { WorkbenchWorkbookHost } from '../src/features/workbench/workbook-dialog'
import '../src/i18n'

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listener: null as null | ((message: Mir3BridgeEnvelope) => void) }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }))
vi.mock('../src/features/projects/workspace-bridge', () => ({
  subscribeHarnessBridge(listener: (message: Mir3BridgeEnvelope) => void) {
    mocks.listener = listener
    return () => {
      mocks.listener = null
    }
  },
}))
vi.mock('@overlastic/react', () => ({ useOverlay: () => [null, () => Promise.resolve()] }))

const project = { id: 'project', root: '/project' } as Mir3Project
const path = '引擎/Mir200/Envir/Data/cfg_item.xls'

function openWorkbook(projectId = project.id, target = path) {
  act(() => mocks.listener?.({ type: 'mir3/workbook.open', projectId, payload: { path: target } } as Mir3BridgeEnvelope))
}

function mountWorkbook(readonly = false) {
  let revision = 0
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'domain_save_node_list')
      return []
    if (command === 'workbench_xls_open') {
      return {
        workingCopy: readonly ? null : { id: 'copy', revision, systemId: 'item', pluginVersion: '1.3.2', dirty: revision > 0 },
        data: {
          revision,
          workbook: { relativePath: path, sha256: 'base', sheets: [{ name: 'Items', rowCount: 1, columnCount: 1 }] },
          sheets: [{ sheet: 'Items', rowCount: 1, columnCount: 1, rows: [[revision ? 'changed' : 'original']] }],
        },
      }
    }
    if (command === 'domain_working_xls_patch') {
      expect(args.operation.expectedRevision).toBe(0)
      revision = 1
      return { revision }
    }
    if (command === 'domain_working_save') {
      expect(args.expectedRevision).toBe(1)
      return {}
    }
    throw new Error(`Unexpected command: ${command}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><WorkbenchWorkbookHost project={project} /></QueryClientProvider>)
}

afterEach(() => {
  cleanup()
  mocks.invoke.mockReset()
})

it('opens XLS through the shared editor and only writes after an explicit save', async () => {
  mountWorkbook()
  openWorkbook('foreign')
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'workbench_xls_open')).toBe(false)
  openWorkbook()
  const cell = await screen.findByDisplayValue('original')
  fireEvent.change(cell, { target: { value: 'changed' } })
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'domain_working_xls_patch')).toBe(false)
  openWorkbook(project.id, '引擎/Mir200/Envir/Data/cfg_equip.xls')
  expect(screen.getByDisplayValue('changed')).toBe(cell)
  fireEvent.click(screen.getByRole('button', { name: /^(保存|Save)$/ }))
  await waitFor(() => expect(mocks.invoke.mock.calls.some(([command]) => command === 'domain_working_save')).toBe(true))
})

it('keeps a reference workbook read-only', async () => {
  mountWorkbook(true)
  openWorkbook()
  const cell = await screen.findByDisplayValue('original') as HTMLInputElement
  expect(cell.readOnly).toBe(true)
  expect((screen.getByRole('button', { name: /^(保存|Save)$/ }) as HTMLButtonElement).disabled).toBe(true)
})
