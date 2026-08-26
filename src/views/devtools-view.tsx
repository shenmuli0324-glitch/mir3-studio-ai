import type { DevToolId } from '@/features/devtools/devtool-registry'
import { useState } from 'react'
import { DevToolsCatalog } from '@/features/devtools/catalog/devtools-catalog'
import { getDevTool } from '@/features/devtools/devtool-registry'
import { DomainSystemView } from '@/features/devtools/domain/domain-system-view'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'

export function DevToolsView({ preview = false }: { preview?: boolean }) {
  const [activeToolId, setActiveToolId] = useState<DevToolId | null>(null)

  function openTool(id: DevToolId) {
    setActiveToolId(id)
  }

  function closeTool() {
    setActiveToolId(null)
  }

  if (activeToolId == null)
    return <DevToolsCatalog onOpenTool={openTool} />

  const tool = getDevTool(activeToolId)
  if (preview)
    return <DomainSystemView tool={tool} project={null} onBack={closeTool} />
  return <ConnectedDomainSystemView tool={tool} onBack={closeTool} />
}

function ConnectedDomainSystemView({ tool, onBack }: { tool: ReturnType<typeof getDevTool>, onBack: () => void }) {
  const { activeProject } = useMir3Projects()
  return <DomainSystemView tool={tool} project={activeProject} onBack={onBack} />
}
