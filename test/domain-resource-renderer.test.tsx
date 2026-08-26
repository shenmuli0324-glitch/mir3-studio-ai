// @vitest-environment happy-dom

import type { DomainMapProjection, DomainResourceRecord } from '../src/features/devtools/domain/types'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { getDomainResource } from '../src/features/devtools/domain/api'
import { projectionTable } from '../src/features/devtools/domain/projection-model'
import { ResourceRenderer } from '../src/features/devtools/domain/renderers/resource-renderer'
import '../src/i18n'

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }))

afterEach(() => {
  cleanup()
  mocks.invoke.mockReset()
  vi.restoreAllMocks()
})

describe('domain resource projection', () => {
  it('requests the selected resource through the generic Tauri DTO', async () => {
    mocks.invoke.mockResolvedValueOnce({ id: 'resource-1' })
    await getDomainResource('project-1', 'quest', 'resource-1')
    expect(mocks.invoke).toHaveBeenCalledWith('domain_resource_get', {
      projectId: 'project-1',
      systemId: 'quest',
      resourceId: 'resource-1',
    })
  })

  it('turns real JSON and delimited text into rows without file metadata', () => {
    const jsonTable = projectionTable({
      kind: 'text',
      content: JSON.stringify({ nodes: [{ id: 7, name: 'Start', state: 'active' }] }),
      truncated: false,
    })
    const delimitedTable = projectionTable({
      kind: 'text',
      content: 'id\tname\n9\tDragon',
      truncated: false,
    })
    expect(jsonTable).toMatchObject({ columns: ['id', 'name', 'state'], rows: [['7', 'Start', 'active']] })
    expect(delimitedTable).toMatchObject({ columns: ['id', 'name'], rows: [['9', 'Dragon']] })
  })

  it('renders XLS cells and flow nodes from projected fields', () => {
    const xls = resourceWithProjection({
      kind: 'xls',
      sha256: 'sha',
      truncated: false,
      sheets: [{ name: 'Items', rowCount: 2, columnCount: 2, rows: [['ID', 'Name'], ['100', 'Potion']] }],
    })
    const rendered = render(<ResourceRenderer renderer="table-v1" resource={xls} loading={false} error={null} />)
    expect(screen.getByText('Potion')).toBeTruthy()
    rendered.rerender(<ResourceRenderer renderer="flow-v1" resource={resourceWithProjection({ kind: 'text', content: '[{"id":"q1","name":"Quest Alpha","state":"open"}]', truncated: false })} loading={false} error={null} />)
    expect(screen.getByText('Quest Alpha')).toBeTruthy()
    expect(screen.getByText('open')).toBeTruthy()
  })

  it('draws map cells and exposes dimensions, coordinates, layers, and walkability', () => {
    const fillRect = vi.fn()
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      clearRect: vi.fn(),
      fillRect,
      strokeRect: vi.fn(),
      fillStyle: '',
      strokeStyle: '',
      globalAlpha: 1,
    } as unknown as CanvasRenderingContext2D)
    render(<ResourceRenderer renderer="map-canvas-v1" resource={resourceWithProjection(mapProjection())} loading={false} error={null} />)
    expect(fillRect).toHaveBeenCalled()
    expect(screen.getByText('64 × 48')).toBeTruthy()
    expect(screen.getAllByText('(0, 0)')).toHaveLength(2)
    expect(screen.getByText(/Back 1:20.*middle 2:30.*front 3:40/)).toBeTruthy()
    expect(screen.getAllByText('Walkable').length).toBeGreaterThan(0)
  })
})

function resourceWithProjection(projection: DomainResourceRecord['projection']): DomainResourceRecord {
  return {
    id: 'resource-1',
    systemId: 'fixture',
    resourceType: 'file',
    label: 'fixture',
    files: [],
    dependencySystems: [],
    writable: false,
    projection,
    diagnostics: [],
  }
}

function mapProjection(): DomainMapProjection {
  return {
    kind: 'map',
    header: {
      format: 'aragom31',
      width: 64,
      height: 48,
      sourceSha256: 'sha',
      capabilities: { background: true, middle: true, front: true, collision: true },
      diagnostics: [],
    },
    initialChunk: {
      chunkX: 0,
      chunkY: 0,
      startX: 0,
      startY: 0,
      width: 1,
      height: 1,
      cells: [{
        x: 0,
        y: 0,
        background: { library: 1, image: 20 },
        middle: { library: 2, image: 30 },
        front: { library: 3, image: 40 },
        walkable: true,
        frontBlocked: false,
        middleAnimationFrames: 0,
        frontAnimationFrames: 0,
        doorIndex: 0,
        doorOffset: 0,
        light: 0,
      }],
    },
  }
}
