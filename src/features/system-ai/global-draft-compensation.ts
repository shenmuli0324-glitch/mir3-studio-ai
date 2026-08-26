import { disassociateDomainDraftComposite, discardDomainDraft } from '@/features/devtools/domain/api'

export interface AssociatedCompositeDraft {
  draftId: string
  systemId: string
  pluginVersion: string
  compositeId: string
}

export async function compensateGlobalDraftSetup(
  projectId: string,
  createdDraftIds: string[],
  associatedDraft: AssociatedCompositeDraft | null,
): Promise<string[]> {
  const errors: string[] = []
  for (const draftId of [...createdDraftIds].reverse()) {
    try {
      await discardDomainDraft(projectId, draftId)
    }
    catch (reason) {
      errors.push(`GLOBAL_DRAFT_COMPENSATION_DISCARD_FAILED:${draftId}:${String(reason)}`)
    }
  }
  if (associatedDraft) {
    try {
      await disassociateDomainDraftComposite(
        projectId,
        associatedDraft.draftId,
        associatedDraft.systemId,
        associatedDraft.pluginVersion,
        associatedDraft.compositeId,
      )
    }
    catch (reason) {
      errors.push(`GLOBAL_DRAFT_COMPENSATION_DISASSOCIATE_FAILED:${associatedDraft.draftId}:${String(reason)}`)
    }
  }
  return errors
}
