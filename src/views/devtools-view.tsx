import type { DevToolId } from '@/features/devtools/devtool-registry'
import type { VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import { useState } from 'react'
import { DevToolsCatalog } from '@/features/devtools/catalog/devtools-catalog'
import { DEV_TOOLS, getDevTool } from '@/features/devtools/devtool-registry'
import { DomainSystemView } from '@/features/devtools/domain/domain-system-view'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'

export function DevToolsView({ preview = false, target }: { preview?: boolean, target?: VerifiedDevtoolsTarget | null }) {
  const [activeToolId, setActiveToolId] = useState<DevToolId | null>(() => target?.systemId as DevToolId | undefined ?? null)

  function openTool(id: DevToolId) {
    setActiveToolId(id)
  }

  function closeTool() {
    setActiveToolId(null)
  }

  function openSystem(systemId: string) {
    if (DEV_TOOLS.some(candidate => candidate.id === systemId))
      setActiveToolId(systemId as DevToolId)
  }

  if (activeToolId == null)
    return <DevToolsCatalog onOpenTool={openTool} />

  const tool = getDevTool(activeToolId)
  if (preview)
    return <DomainSystemView tool={tool} project={null} onBack={closeTool} onOpenSystem={openSystem} target={target} />
  return <ConnectedDomainSystemView tool={tool} onBack={closeTool} onOpenSystem={openSystem} target={target} />
}

function ConnectedDomainSystemView({ tool, onBack, onOpenSystem, target }: { tool: ReturnType<typeof getDevTool>, onBack: () => void, onOpenSystem: (systemId: string) => void, target?: VerifiedDevtoolsTarget | null }) {
  const { activeProject } = useMir3Projects()
  return <DomainSystemView tool={tool} project={activeProject} onBack={onBack} onOpenSystem={onOpenSystem} target={target} />
}
