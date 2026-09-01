// @vitest-environment happy-dom

import type { PropsWithChildren } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useMir3Projects } from './use-mir3-projects'

const mocks = vi.hoisted(() => {
  function project(id: string, activeWorkspaceRoot = `/fixture/${id}`) {
    return {
      id,
      name: id,
      root: `/fixture/${id}`,
      clientRoot: `/fixture/${id}/客户端`,
      engineRoot: `/fixture/${id}/引擎`,
      activeWorkspaceRoot,
      engineVersion: '1.8',
      status: 'valid' as const,
      warnings: [],
      createdAt: 1,
      updatedAt: 1,
    }
  }
  return {
    activeProject: project('active'),
    importedProject: project('imported'),
    selectedProject: project('active', '/fixture/active/client'),
    restart: vi.fn(),
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((command: string) => {
    if (command === 'project_list')
      return Promise.resolve([mocks.activeProject])
    if (command === 'project_get_active')
      return Promise.resolve(mocks.activeProject)
    if (command === 'scan_status')
      return Promise.resolve(null)
    if (command === 'project_pick_directory')
      return Promise.resolve(mocks.importedProject.root)
    if (command === 'project_import') {
      mocks.activeProject = mocks.importedProject
      return Promise.resolve(mocks.importedProject)
    }
    if (command === 'workspace_pick_directory')
      return Promise.resolve(mocks.selectedProject.activeWorkspaceRoot)
    if (command === 'workspace_select')
      return Promise.resolve(mocks.selectedProject)
    return Promise.resolve(null)
  }),
}))

vi.mock('@/store', () => ({
  store: { harness: { restart: mocks.restart } },
}))

beforeEach(() => {
  mocks.activeProject = fixtureProject('active')
  mocks.importedProject = fixtureProject('imported')
  mocks.selectedProject = fixtureProject('active', '/fixture/active/client')
  mocks.restart.mockReset()
  mocks.restart.mockResolvedValue(undefined)
})

describe('useMir3Projects Workspace switching', () => {
  it('restarts Harness and activates its scope after importing a project', async () => {
    const { result } = renderHook(() => useMir3Projects(), { wrapper: queryWrapper() })
    await waitFor(() => expect(result.current.activeProject?.id).toBe('active'))

    await act(async () => {
      await result.current.importProject()
    })

    expect(mocks.restart).toHaveBeenCalledOnce()
  })

  it('restarts Harness after the active project Workspace changes', async () => {
    const { result } = renderHook(() => useMir3Projects(), { wrapper: queryWrapper() })
    await waitFor(() => expect(result.current.activeProject?.id).toBe('active'))

    await act(async () => {
      await result.current.selectWorkspace('active')
    })

    expect(mocks.restart).toHaveBeenCalledOnce()
  })

  it('does not restart Harness when an inactive project Workspace changes', async () => {
    mocks.selectedProject = fixtureProject('inactive', '/fixture/inactive/client')
    const { result } = renderHook(() => useMir3Projects(), { wrapper: queryWrapper() })
    await waitFor(() => expect(result.current.activeProject?.id).toBe('active'))

    await act(async () => {
      await result.current.selectWorkspace('inactive')
    })

    expect(mocks.restart).not.toHaveBeenCalled()
  })

  it('restarts Harness after removing an inactive project so its scope is pruned', async () => {
    const { result } = renderHook(() => useMir3Projects(), { wrapper: queryWrapper() })
    await waitFor(() => expect(result.current.activeProject?.id).toBe('active'))

    await act(async () => {
      await result.current.removeProject('inactive')
    })

    expect(mocks.restart).toHaveBeenCalledOnce()
  })
})

function queryWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return function QueryWrapper({ children }: PropsWithChildren) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

function fixtureProject(id: string, activeWorkspaceRoot = `/fixture/${id}`) {
  return {
    id,
    name: id,
    root: `/fixture/${id}`,
    clientRoot: `/fixture/${id}/客户端`,
    engineRoot: `/fixture/${id}/引擎`,
    activeWorkspaceRoot,
    engineVersion: '1.8',
    status: 'valid' as const,
    warnings: [],
    createdAt: 1,
    updatedAt: 1,
  }
}
