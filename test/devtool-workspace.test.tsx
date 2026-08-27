// @vitest-environment happy-dom

import type { DevToolIcon } from '../src/features/devtools/devtool-registry'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DevToolWorkspace } from '../src/features/devtools/shell/devtool-workspace'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t(key: string) {
      const translations: Record<string, string> = {
        'studio.devtools.panels.resources_expand': 'Expand resource pane',
        'studio.devtools.panels.resources_collapse': 'Collapse resource pane',
        'studio.devtools.panels.ai_expand': 'Expand AI pane',
        'studio.devtools.panels.ai_collapse': 'Collapse AI pane',
        'studio.devtools.mobile.resources': 'Resources',
        'studio.devtools.mobile.ai': 'AI',
      }
      return translations[key] ?? key
    },
  }),
}))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function FixtureIcon() {
  return <span />
}

const tool = {
  id: 'map',
  order: 1,
  category: 'resources',
  icon: FixtureIcon as DevToolIcon,
  status: 'ready',
} as const

describe('development tool three-pane workspace', () => {
  it('keeps desktop resource and AI panes reachable after independent collapse', () => {
    render(
      <DevToolWorkspace
        tool={tool}
        onBack={() => {}}
        sidebar={<span>resource-fixture</span>}
        toolbar={<span>toolbar-fixture</span>}
        rightPanel={<span>ai-fixture</span>}
      >
        <span>center-fixture</span>
      </DevToolWorkspace>,
    )
    const resourcePane = screen.getByText('resource-fixture').closest('aside')
    expect(resourcePane?.style.width).toBe('280px')
    fireEvent.click(screen.getByRole('button', { name: 'Collapse resource pane' }))
    expect(resourcePane?.className).toContain('hidden')
    expect(screen.getByRole('button', { name: 'Expand resource pane' })).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Collapse AI pane' }))
    expect(screen.getByRole('button', { name: 'Expand AI pane' })).toBeTruthy()
    expect(screen.getByText('center-fixture')).toBeTruthy()
  })

  it('uses one mutually exclusive narrow-screen drawer state', () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)
    render(
      <DevToolWorkspace
        tool={tool}
        onBack={() => {}}
        sidebar={<span>mobile-resource-fixture</span>}
        toolbar={<span>mobile-toolbar-fixture</span>}
        rightPanel={<span>mobile-ai-fixture</span>}
      >
        <span>mobile-center-fixture</span>
      </DevToolWorkspace>,
    )
    fireEvent.click(screen.getByRole('button', { name: 'Collapse resource pane' }))
    expect(screen.getByText('mobile-resource-fixture').closest('aside')?.className).toContain('absolute')
    fireEvent.click(screen.getByRole('button', { name: 'Collapse AI pane' }))
    expect(screen.getByText('mobile-resource-fixture').closest('aside')?.className).toContain('max-[900px]:hidden')
    expect(screen.getByText('mobile-ai-fixture').parentElement?.className).toContain('absolute')
  })
})
