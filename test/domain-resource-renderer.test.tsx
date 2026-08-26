// @vitest-environment happy-dom

import type { DomainMapProjection, DomainResourceRecord } from '../src/features/devtools/domain/types'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { getDomainResource, queryDomainResources } from '../src/features/devtools/domain/api'
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

  it('queries paged record resources instead of treating files as the resource list', async () => {
    mocks.invoke.mockResolvedValueOnce([])
    await queryDomainResources('project-1', 'shop', 'potion', 50, 100)
    expect(mocks.invoke).toHaveBeenCalledWith('domain_resource_query', {
      projectId: 'project-1',
      systemId: 'shop',
      query: { text: 'potion', limit: 50, offset: 100 },
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

  it('uses distinct semantic views for curves, calendars, rankings, timelines, and topology', () => {
    const rows = resourceWithProjection({
      kind: 'text',
      content: 'id\tname\tvalue\tnext\ttime\n1\tAlpha\t10\t2\t08:00\n2\tBeta\t25\t\t09:00',
      truncated: false,
    })
    const rendered = render(<ResourceRenderer renderer="chart-v1" resource={rows} loading={false} error={null} />)
    expect(screen.getByRole('img', { name: 'Domain progression curve' })).toBeTruthy()
    rendered.rerender(<ResourceRenderer renderer="calendar-v1" resource={rows} loading={false} error={null} />)
    expect(screen.getByText('Day 1')).toBeTruthy()
    rendered.rerender(<ResourceRenderer renderer="ranking-v1" resource={rows} loading={false} error={null} />)
    expect(screen.getByText('#1')).toBeTruthy()
    rendered.rerender(<ResourceRenderer renderer="timeline-v1" resource={rows} loading={false} error={null} />)
    expect(screen.getByText('08:00')).toBeTruthy()
    rendered.rerender(<ResourceRenderer renderer="topology-v1" resource={rows} loading={false} error={null} />)
    expect(rendered.container.querySelectorAll('svg path')).toHaveLength(1)
  })

  it('combines dedicated quest, talent, activity, Sabac, and cross-server summaries with semantic views', () => {
    const quest = resourceWithProjection({
      kind: 'text',
      content: 'questId\tname\tnextQuestId\nq1\tStart\tq2\nq2\tFinish\t',
      truncated: false,
    }, 'quest')
    const rendered = render(<ResourceRenderer renderer="flow-v1" resource={quest} loading={false} error={null} />)
    expect(screen.getByText('Quest flow overview')).toBeTruthy()
    expect(screen.getByText('Terminal nodes')).toBeTruthy()

    rendered.rerender(
      <ResourceRenderer
        renderer="graph-v1"
        resource={resourceWithProjection({
          kind: 'text',
          content: 'nodeId\tparentNodeId\tcostPoints\trequiredLevel\nn1\t\t2\t10\nn2\tn1\t3\t20',
          truncated: false,
        }, 'talent')}
        loading={false}
        error={null}
      />,
    )
    expect(screen.getByText('Talent tree budget')).toBeTruthy()
    expect(screen.getByText('Total cost')).toBeTruthy()

    rendered.rerender(
      <ResourceRenderer
        renderer="timeline-v1"
        resource={resourceWithProjection({
          kind: 'text',
          content: 'eventId\tstartEpochSeconds\tendEpochSeconds\ne1\t200\t100',
          truncated: false,
        }, 'limited_event')}
        loading={false}
        error={null}
      />,
    )
    expect(screen.getByText('Activity schedule integrity')).toBeTruthy()
    expect(screen.getByText('Warnings: 1')).toBeTruthy()

    rendered.rerender(
      <ResourceRenderer
        renderer="spatial-flow-v1"
        resource={resourceWithProjection({
          kind: 'text',
          content: 'phaseId\tbattleMapId\tstartMinute\tendMinute\np1\t3\t60\t120',
          truncated: false,
        }, 'sabac')}
        loading={false}
        error={null}
      />,
    )
    expect(screen.getByText('Sabac spatial state')).toBeTruthy()
    expect(screen.getByText('Battle maps')).toBeTruthy()

    rendered.rerender(
      <ResourceRenderer
        renderer="topology-v1"
        resource={resourceWithProjection({
          kind: 'text',
          content: 'routeId\tsourceShard\ttargetShard\tengineRange\nr1\ts1\ts2\t>=1.0.0',
          truncated: false,
        }, 'cross_server')}
        loading={false}
        error={null}
      />,
    )
    expect(screen.getByText('Cross-server compatibility matrix')).toBeTruthy()
    expect(screen.getByText('Source shard')).toBeTruthy()
    expect(screen.getByText('>=1.0.0')).toBeTruthy()
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

function resourceWithProjection(projection: DomainResourceRecord['projection'], systemId = 'fixture'): DomainResourceRecord {
  return {
    id: 'resource-1',
    systemId,
    resourceType: 'file',
    label: 'fixture',
    files: [],
    dependencySystems: [],
    writable: false,
    projection,
    diagnostics: [],
    fields: {},
    source: { path: 'fixture.txt', sheet: null, row: null, headers: [] },
    dependencies: [],
    mappingsApplied: [],
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
