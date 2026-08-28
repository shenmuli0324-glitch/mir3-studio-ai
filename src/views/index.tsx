import type { HarnessSurface, StudioView } from '@/layout/studio-types'
import { BuildsView } from './builds-view'
import { FeedbackView } from './feedback-view'
import { GuiDesignerView } from './gui-designer-view'
import { LogsView } from './logs-view'
import { ProjectView } from './project-view'
import { RuntimeView } from './runtime-view'

export function StudioViewContent({ view }: {
  view: Exclude<StudioView, HarnessSurface>
}) {
  switch (view) {
    case 'project':
      return <ProjectView />
    case 'gui-designer':
      return <GuiDesignerView />
    case 'builds':
      return <BuildsView />
    case 'runtime':
      return <RuntimeView />
    case 'feedback':
      return <FeedbackView />
    case 'logs':
      return <LogsView />
  }
}
