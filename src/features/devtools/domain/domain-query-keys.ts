export function domainWorkspaceQueryKeys(projectId: string, systemId: string): unknown[][] {
  return [
    ['domain-files', projectId, systemId],
    ['domain-source', projectId],
    ['domain-workbook', projectId],
  ]
}
