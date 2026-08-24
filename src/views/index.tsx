import type { Mir3Project } from '@/features/projects/types'
import type { HarnessSurface, StudioView } from '@/layout/studio-types'
import { BuildsView } from './builds-view'
import { FeedbackView } from './feedback-view'
import { KnowledgeView } from './knowledge-view'
import { LogsView } from './logs-view'
import { ProjectView } from './project-view'
import { RuntimeView } from './runtime-view'

export function StudioViewContent({ view, project }: {
  view: Exclude<StudioView, HarnessSurface>
  project: Mir3Project | null
}) {
  switch (view) {
    case 'project':
      return <ProjectView />
    case 'builds':
      return <BuildsView />
    case 'runtime':
      return <RuntimeView />
    case 'feedback':
      return <FeedbackView />
    case 'knowledge':
      return <KnowledgeView project={project} />
    case 'logs':
      return <LogsView />
  }
}
