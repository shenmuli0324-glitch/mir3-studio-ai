import type { DevToolId } from '@/features/devtools/devtool-registry'
import { useState } from 'react'
import { DevToolsCatalog } from '@/features/devtools/catalog/devtools-catalog'
import { getDevTool } from '@/features/devtools/devtool-registry'
import { MapToolView } from '@/features/devtools/map/map-tool-view'
import { PlannedToolView } from '@/features/devtools/shell/planned-tool-view'
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
  if (activeToolId === 'map') {
    if (preview)
      return <MapToolView tool={tool} onBack={closeTool} hasProject />
    return <ConnectedMapToolView tool={tool} onBack={closeTool} />
  }

  return <PlannedToolView tool={tool} onBack={closeTool} />
}

function ConnectedMapToolView({ tool, onBack }: { tool: ReturnType<typeof getDevTool>, onBack: () => void }) {
  const { activeProject } = useMir3Projects()
  return <MapToolView tool={tool} onBack={onBack} hasProject={activeProject != null} />
}
