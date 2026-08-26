import type { VerifiedDevtoolsTarget } from '@/features/system-ai/ai-handoff'
import type { HarnessSurface, StudioView } from '@/layout/studio-types'
import { BuildsView } from './builds-view'
import { DevToolsView } from './devtools-view'
import { FeedbackView } from './feedback-view'
import { GuiDesignerView } from './gui-designer-view'
import { LogsView } from './logs-view'
import { ProjectView } from './project-view'
import { RuntimeView } from './runtime-view'

export function StudioViewContent({ view, devtoolsTarget }: {
  view: Exclude<StudioView, HarnessSurface>
  devtoolsTarget?: VerifiedDevtoolsTarget | null
}) {
  switch (view) {
    case 'project':
      return <ProjectView />
    case 'gui-designer':
      return <GuiDesignerView />
    case 'builds':
      return <BuildsView />
    case 'devtools':
      return <DevToolsView key={devtoolsTarget?.nonce ?? 'devtools'} target={devtoolsTarget} />
    case 'runtime':
      return <RuntimeView />
    case 'feedback':
      return <FeedbackView />
    case 'logs':
      return <LogsView />
  }
}
