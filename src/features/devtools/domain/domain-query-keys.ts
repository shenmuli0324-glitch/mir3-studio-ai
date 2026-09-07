export function domainWorkspaceQueryKeys(projectId: string, systemId: string): unknown[][] {
  return [
    ['domain-files', projectId, systemId],
    ['domain-references', projectId, systemId],
    ['domain-bindings', projectId, systemId],
    ['domain-unclaimed', projectId],
    ['domain-source', projectId],
    ['domain-workbook', projectId],
  ]
}
