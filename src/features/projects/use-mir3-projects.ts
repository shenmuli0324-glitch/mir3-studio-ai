import type { Mir3Project, ScanState } from './types'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { store } from '@/store'

export function useMir3Projects() {
  const queryClient = useQueryClient()
  const projects = useQuery({
    queryKey: ['mir3-projects'],
    queryFn: () => invoke<Mir3Project[]>('project_list'),
  })
  const active = useQuery({
    queryKey: ['mir3-active-project'],
    queryFn: () => invoke<Mir3Project | null>('project_get_active'),
  })
  const scan = useQuery({
    queryKey: ['mir3-scan'],
    queryFn: () => invoke<ScanState>('scan_status'),
    refetchInterval: query => query.state.data?.phase === 'running' ? 800 : false,
  })

  useEffect(() => {
    let unlisten: (() => void) | undefined
    void listen<ScanState>('mir3-scan-updated', () => {
      void invalidateProjectQueries(queryClient)
    }).then((dispose) => {
      unlisten = dispose
    })
    return () => unlisten?.()
  }, [queryClient])

  const importProject = useMutation({
    mutationFn: async () => {
      const path = await invoke<string | null>('project_pick_directory')
      if (!path)
        return null
      return invoke<Mir3Project>('project_import', { path })
    },
    onSuccess: () => invalidateProjectQueries(queryClient),
  })

  const activateProject = useMutation({
    mutationFn: (projectId: string) => invoke<Mir3Project>('project_activate', { projectId }),
    onSuccess: async (project) => {
      queryClient.setQueryData(['mir3-active-project'], project)
      await invalidateProjectQueries(queryClient)
      restartHarnessAfterProjectChange()
    },
  })

  const selectWorkspace = useMutation({
    mutationFn: async (projectId: string) => {
      const path = await invoke<string | null>('workspace_pick_directory', { projectId })
      if (!path)
        return null
      return invoke<Mir3Project>('workspace_select', { projectId, path })
    },
    onSuccess: () => invalidateProjectQueries(queryClient),
  })

  const startScan = useMutation({
    mutationFn: (projectId: string) => invoke<ScanState>('scan_start', { projectId }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })

  const removeProject = useMutation({
    mutationFn: async (projectId: string) => {
      const wasActive = active.data?.id === projectId
      await invoke<void>('project_remove', { projectId })
      return { projectId, wasActive }
    },
    onSuccess: async ({ projectId, wasActive }) => {
      queryClient.setQueryData<Mir3Project[]>(['mir3-projects'], current => current?.filter(project => project.id !== projectId))
      if (wasActive)
        queryClient.setQueryData(['mir3-active-project'], null)
      await invalidateProjectQueries(queryClient)
      if (wasActive)
        restartHarnessAfterProjectChange()
    },
  })

  const relinkProject = useMutation({
    mutationFn: async (projectId: string) => {
      const path = await invoke<string | null>('project_pick_directory')
      if (!path)
        return null
      const project = await invoke<Mir3Project>('project_relink', { projectId, path })
      if (active.data?.id === projectId)
        restartHarnessAfterProjectChange()
      return project
    },
    onSuccess: () => invalidateProjectQueries(queryClient),
  })

  return {
    projects: projects.data ?? [],
    activeProject: active.data ?? null,
    scan: scan.data ?? null,
    loading: projects.isLoading || active.isLoading,
    busy: importProject.isPending || activateProject.isPending || selectWorkspace.isPending || startScan.isPending || removeProject.isPending || relinkProject.isPending,
    pending: {
      import: importProject.isPending,
      activate: activateProject.isPending,
      workspace: selectWorkspace.isPending,
      scan: startScan.isPending,
      remove: removeProject.isPending,
      relink: relinkProject.isPending,
    },
    error: projects.error || active.error || importProject.error || activateProject.error || selectWorkspace.error || startScan.error || removeProject.error || relinkProject.error,
    importProject: importProject.mutateAsync,
    activateProject: activateProject.mutateAsync,
    selectWorkspace: selectWorkspace.mutateAsync,
    startScan: startScan.mutateAsync,
    removeProject: removeProject.mutateAsync,
    relinkProject: relinkProject.mutateAsync,
  }
}

function restartHarnessAfterProjectChange() {
  void store.harness.restart().catch((error) => {
    console.error('[MIR3 project] Harness restart failed after project change:', error)
  })
}

export async function invalidateProjectQueries(queryClient: ReturnType<typeof useQueryClient>) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['mir3-projects'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-active-project'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-scan'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-index-stats'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-drafts'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-snapshots'] }),
    queryClient.invalidateQueries({ queryKey: ['mir3-knowledge'] }),
  ])
}
