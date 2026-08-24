import type { Mir3Project } from '@/features/projects/types'

export const STUDIO_VIEWS = [
  'project',
  'workbench',
  'builds',
  'runtime',
  'feedback',
  'knowledge',
  'settings',
  'logs',
] as const

export type StudioView = typeof STUDIO_VIEWS[number]

export interface StudioShellState {
  activeView: StudioView
  sidebarCollapsed: boolean
  project: Mir3Project | null
}

export interface HarnessWorkbenchState {
  status: import('@/store/modules/harness/types').SetupStatus
  serviceHealthy: boolean
  iframeSrc: string
  iframeKey: number
  iframeLoaded: boolean
  iframeError: boolean
}

export type HarnessSurface = 'workbench' | 'settings'

export const DEFAULT_STUDIO_VIEW: StudioView = 'project'

export function studioViewTitleKey(view: StudioView): string {
  return `studio.nav.${view}`
}

export function isHarnessView(view: StudioView): view is HarnessSurface {
  return view === 'workbench' || view === 'settings'
}

export function harnessSurfaceFor(view: StudioView): HarnessSurface {
  return view === 'settings' ? 'settings' : 'workbench'
}
