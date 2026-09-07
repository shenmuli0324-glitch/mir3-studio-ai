import type { IndexStats, KnowledgeRecord, KnowledgeStatus } from './types'
import type { DomainSaveNode, DomainWorkingRestoreResult } from '@/features/devtools/domain/types'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { invalidateProjectQueries } from './use-mir3-projects'

export function useProjectDetails(projectId?: string) {
  const queryClient = useQueryClient()
  const enabled = Boolean(projectId)
  const stats = useQuery({
    queryKey: ['mir3-index-stats', projectId],
    queryFn: () => invoke<IndexStats>('index_stats', { projectId }),
    enabled,
  })
  const saveNodes = useQuery({
    queryKey: ['domain-save-nodes', projectId],
    queryFn: () => invoke<DomainSaveNode[]>('domain_save_node_list', { projectId, systemId: null, limit: 100 }),
    enabled,
  })
  const knowledge = useQuery({
    queryKey: ['mir3-knowledge', projectId],
    queryFn: () => invoke<KnowledgeRecord[]>('knowledge_list', {
      projectId,
      filter: { text: null, statuses: [], limit: 200 },
    }),
    enabled,
  })
  const restore = useMutation({
    mutationFn: (nodeId: string) => invoke<DomainWorkingRestoreResult>('domain_save_node_restore', { projectId, nodeId }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  const setKnowledgeStatus = useMutation({
    mutationFn: ({ knowledgeId, status }: { knowledgeId: string, status: KnowledgeStatus }) =>
      invoke<KnowledgeRecord>('knowledge_set_status', { projectId, knowledgeId, status }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  return {
    stats: stats.data ?? null,
    saveNodes: saveNodes.data ?? [],
    knowledge: knowledge.data ?? [],
    loading: stats.isLoading || saveNodes.isLoading || knowledge.isLoading,
    restoreSaveNode: restore.mutateAsync,
    setKnowledgeStatus: setKnowledgeStatus.mutateAsync,
    busy: restore.isPending || setKnowledgeStatus.isPending,
    error: stats.error || saveNodes.error || knowledge.error || restore.error || setKnowledgeStatus.error,
  }
}
