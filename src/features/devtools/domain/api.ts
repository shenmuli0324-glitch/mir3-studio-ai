import type {
  DomainDraft,
  DomainDraftConfirmation,
  DomainFileRecord,
  DomainManifest,
  DomainMemory,
  DomainPackState,
  DomainPackUpdateCheck,
  DomainResourceRecord,
  DomainSnapshot,
  DomainSystemDescription,
  DomainValidationReport,
  LegacyDraftCloneRequest,
  SafeTextOpen,
  SafeTextPatchResult,
  SystemSessionBinding,
  TaskReceipt,
  TaskScopeLease,
  UserCapability,
} from './types'
import { invoke } from '@tauri-apps/api/core'

export function listDomainSystems() {
  return invoke<DomainManifest[]>('domain_system_list')
}

export function describeDomainSystem(projectId: string, systemId: string) {
  return invoke<DomainSystemDescription>('domain_system_describe', { projectId, systemId })
}

export async function queryDomainFiles(projectId: string, systemId: string, text = '') {
  const files: DomainFileRecord[] = []
  const pageSize = 10_000
  while (true) {
    const page = await invoke<DomainFileRecord[]>('domain_file_query', {
      projectId,
      systemId,
      query: { text, limit: pageSize, offset: files.length },
    })
    files.push(...page)
    if (page.length < pageSize)
      return files
  }
}

export async function queryUnclaimedDomainFiles(projectId: string, text = '') {
  const files: DomainFileRecord[] = []
  const pageSize = 10_000
  while (true) {
    const page = await invoke<DomainFileRecord[]>('domain_unclaimed_file_query', {
      projectId,
      query: { text, limit: pageSize, offset: files.length },
    })
    files.push(...page)
    if (page.length < pageSize)
      return files
  }
}

export function getDomainResource(projectId: string, systemId: string, resourceId: string) {
  return invoke<DomainResourceRecord>('domain_resource_get', { projectId, systemId, resourceId })
}

export function queryDomainResources(projectId: string, systemId: string, text = '', limit = 250, offset = 0) {
  return invoke<DomainResourceRecord[]>('domain_resource_query', {
    projectId,
    systemId,
    query: { text, limit, offset },
  })
}

export function validateDomainSystem(projectId: string, systemId: string) {
  return invoke<DomainValidationReport>('domain_validate', { projectId, systemId })
}

export function validateDomainDraft(projectId: string, draftId: string) {
  return invoke<DomainValidationReport>('domain_draft_validate', { projectId, draftId })
}

export function openDomainText(projectId: string, relativePath: string, draftId?: string | null) {
  return invoke<SafeTextOpen>('safe_file_open', { projectId, relativePath, draftId })
}

export function patchDomainText(projectId: string, opened: SafeTextOpen, newContent: string) {
  return invoke<SafeTextPatchResult>('safe_text_patch', {
    projectId,
    operation: {
      relativePath: opened.relativePath,
      draftId: opened.draftId,
      expectedRevision: opened.revision,
      expectedSha256: opened.sha256,
      originalContent: opened.content,
      newContent,
      newline: opened.newline,
    },
  })
}

export function listDomainDrafts(projectId: string) {
  return invoke<DomainDraft[]>('draft_list', { projectId })
}

export function openDomainDraft(projectId: string, systemId: string, pluginVersion: string, intent: string, compositeId?: string | null) {
  return invoke<DomainDraft>('domain_draft_open', { projectId, systemId, pluginVersion, intent, compositeId })
}

export function associateDomainDraftComposite(projectId: string, draftId: string, systemId: string, pluginVersion: string, compositeId: string) {
  return invoke<void>('domain_draft_composite_associate', { projectId, draftId, systemId, pluginVersion, compositeId })
}

export function previewDomainDraft(projectId: string, draftId: string) {
  return invoke<DomainDraftConfirmation>('draft_preview', { projectId, draftId })
}

export function cloneLegacyDomainDraft(projectId: string, request: LegacyDraftCloneRequest) {
  return invoke<DomainDraftConfirmation>('draft_legacy_clone', { projectId, request })
}

export function applyDomainDraft(projectId: string, draftId: string, confirmationToken: string) {
  return invoke<DomainSnapshot>('draft_apply', { projectId, draftId, confirmationToken })
}

export function getSystemSession(projectId: string, taskId: string) {
  return invoke<SystemSessionBinding | null>('system_session_get', { projectId, taskId })
}

export function bindSystemSession(projectId: string, binding: SystemSessionBinding) {
  return invoke<SystemSessionBinding>('system_session_bind', { projectId, binding })
}

export function saveTaskReceipt(projectId: string, receipt: TaskReceipt) {
  return invoke<TaskReceipt>('task_receipt_save', { projectId, receipt })
}

export function listTaskReceipts(projectId: string, systemId?: string | null) {
  return invoke<TaskReceipt[]>('task_receipt_list', { projectId, systemId })
}

export function issueTaskScope(projectId: string, taskId: string, readSystems: string[], writeSystems: string[], draftIds: string[], pluginVersions: Record<string, string>) {
  return invoke<TaskScopeLease>('task_scope_issue', {
    projectId,
    taskId,
    readSystems,
    writeSystems,
    draftIds,
    pluginVersions,
    expiresAt: Date.now() + 60 * 60 * 1000,
  })
}

export function revokeTaskScope(projectId: string, token: string) {
  return invoke<void>('task_scope_revoke', { projectId, token })
}

export function compileUserCapability(projectId: string, request: { receiptId: string, id: string, name: string, description: string }) {
  return invoke<UserCapability>('user_capability_compile', { projectId, request })
}

export function setUserCapabilityStatus(projectId: string, capabilityId: string, version: string, status: UserCapability['status'], confirmed: boolean) {
  return invoke<UserCapability>('user_capability_set_status', { projectId, capabilityId, version, status, confirmed })
}

export function listDomainMemories(projectId: string, systemId: string, activeOnly = false) {
  return invoke<DomainMemory[]>('domain_memory_list', { projectId, systemId, activeOnly })
}

export function listMemoryCandidates(projectId: string, systemId: string) {
  return invoke<DomainMemory[]>('memory_candidate_list', { projectId, systemId })
}

export function activateMemoryCandidate(projectId: string, memoryId: string) {
  return invoke<DomainMemory>('memory_candidate_activate', { projectId, memoryId, confirmed: true })
}

export function revokeMemoryCandidate(projectId: string, memoryId: string) {
  return invoke<DomainMemory>('memory_candidate_revoke', { projectId, memoryId })
}

export function getDomainPackState(systemId: string) {
  return invoke<DomainPackState>('domain_pack_state', { systemId })
}

export function checkDomainPackUpdates(systemId: string) {
  return invoke<DomainPackUpdateCheck>('domain_pack_update_check', { systemId })
}

export function stageDomainPackUpdate(systemId: string, version: string) {
  return invoke<DomainPackState>('domain_pack_update_stage', { systemId, version })
}

export function activateDomainPack(systemId: string) {
  return invoke<DomainPackState>('domain_pack_activate', { systemId, confirmed: true })
}

export function rollbackDomainPack(systemId: string) {
  return invoke<DomainPackState>('domain_pack_rollback', { systemId, confirmed: true })
}

export function setDomainPackEnabled(systemId: string, enabled: boolean) {
  return invoke<DomainPackState>('domain_pack_set_enabled', { systemId, enabled, confirmed: true })
}
