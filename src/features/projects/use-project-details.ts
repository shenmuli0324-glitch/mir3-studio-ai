import type { Draft, DraftConfirmation, IndexStats, KnowledgeRecord, KnowledgeStatus, Snapshot } from './types'
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
  const drafts = useQuery({
    queryKey: ['mir3-drafts', projectId],
    queryFn: () => invoke<Draft[]>('draft_list', { projectId }),
    enabled,
  })
  const snapshots = useQuery({
    queryKey: ['mir3-snapshots', projectId],
    queryFn: () => invoke<Snapshot[]>('snapshot_list', { projectId }),
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
  const preview = useMutation({
    mutationFn: (draftId: string) => invoke<DraftConfirmation>('draft_preview', { projectId, draftId }),
  })
  const apply = useMutation({
    mutationFn: ({ draftId, confirmationToken }: { draftId: string, confirmationToken: string }) =>
      invoke<Snapshot>('draft_apply', { projectId, draftId, confirmationToken }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  const discard = useMutation({
    mutationFn: (draftId: string) => invoke<Draft>('draft_discard', { projectId, draftId }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  const restore = useMutation({
    mutationFn: (snapshotId: string) => invoke<Snapshot>('snapshot_restore', { projectId, snapshotId }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  const setKnowledgeStatus = useMutation({
    mutationFn: ({ knowledgeId, status }: { knowledgeId: string, status: KnowledgeStatus }) =>
      invoke<KnowledgeRecord>('knowledge_set_status', { projectId, knowledgeId, status }),
    onSuccess: () => invalidateProjectQueries(queryClient),
  })
  return {
    stats: stats.data ?? null,
    drafts: drafts.data ?? [],
    snapshots: snapshots.data ?? [],
    knowledge: knowledge.data ?? [],
    loading: stats.isLoading || drafts.isLoading || snapshots.isLoading || knowledge.isLoading,
    previewDraft: preview.mutateAsync,
    preview: preview.data ?? null,
    applyDraft: apply.mutateAsync,
    discardDraft: discard.mutateAsync,
    restoreSnapshot: restore.mutateAsync,
    setKnowledgeStatus: setKnowledgeStatus.mutateAsync,
    busy: preview.isPending || apply.isPending || discard.isPending || restore.isPending || setKnowledgeStatus.isPending,
    error: stats.error || drafts.error || snapshots.error || knowledge.error || preview.error || apply.error || discard.error || restore.error || setKnowledgeStatus.error,
  }
}
