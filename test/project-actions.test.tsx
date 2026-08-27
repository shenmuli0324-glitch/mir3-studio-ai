// @vitest-environment happy-dom

import { OverlaysProvider } from '@overlastic/react'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ProjectView } from '../src/views/project-view'

const mocks = vi.hoisted(() => {
  function fixtureProject(id: string, name: string) {
    const root = `/fixture/${name}`
    return {
      id,
      name,
      root,
      clientRoot: `${root}/客户端`,
      engineRoot: `${root}/引擎`,
      activeWorkspaceRoot: root,
      engineVersion: '1.8',
      status: 'valid' as const,
      warnings: [],
      createdAt: 1,
      updatedAt: 1,
    }
  }
  return {
    activeProject: fixtureProject('active', '测试'),
    otherProject: fixtureProject('other', '木立'),
    activateProject: vi.fn(),
    removeProject: vi.fn(),
    toast: vi.fn(),
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t(key: string) {
      return key
    },
  }),
}))

vi.mock('../src/utils', () => ({ toast: mocks.toast }))
vi.mock('../src/features/gui-designer/gui-designer-scope', () => ({ isGuiDesignerDirty: () => false }))
vi.mock('../src/features/projects/use-mir3-projects', () => ({
  useMir3Projects: () => ({
    projects: [mocks.activeProject, mocks.otherProject],
    activeProject: mocks.activeProject,
    scan: null,
    busy: false,
    pending: { import: false, activate: false, workspace: false, scan: false, remove: false, relink: false },
    error: null,
    importProject: vi.fn(),
    activateProject: mocks.activateProject,
    selectWorkspace: vi.fn(),
    startScan: vi.fn(),
    removeProject: mocks.removeProject,
    relinkProject: vi.fn(),
  }),
}))
vi.mock('../src/features/projects/use-project-details', () => ({
  useProjectDetails: () => ({
    stats: null,
    drafts: [],
    snapshots: [],
    knowledge: [],
    loading: false,
    previewDraft: vi.fn(),
    applyDraft: vi.fn(),
    discardDraft: vi.fn(),
    restoreSnapshot: vi.fn(),
    setKnowledgeStatus: vi.fn(),
    busy: false,
    error: null,
  }),
}))

afterEach(() => {
  cleanup()
  mocks.activateProject.mockReset()
  mocks.removeProject.mockReset()
  mocks.toast.mockReset()
})

describe('project list actions', () => {
  it('uses an in-app confirmation before switching projects', async () => {
    mocks.activateProject.mockResolvedValue(mocks.otherProject)
    renderProjectView()
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'studio.project.activate' }))
    expect(await screen.findByText('studio.project.switch_confirm')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'buttons.confirm' }))

    await waitFor(() => expect(mocks.activateProject).toHaveBeenCalledWith(mocks.otherProject.id))
  })

  it('cancels removal without invoking the backend action', async () => {
    renderProjectView()
    const user = userEvent.setup()

    await user.click(screen.getAllByRole('button', { name: 'studio.project.remove' })[1])
    expect(await screen.findByText('studio.project.remove_confirm')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'buttons.cancel' }))

    await waitFor(() => expect(mocks.removeProject).not.toHaveBeenCalled())
  })

  it('removes the selected registration after in-app confirmation', async () => {
    mocks.removeProject.mockResolvedValue({ projectId: mocks.otherProject.id, wasActive: false })
    renderProjectView()
    const user = userEvent.setup()

    await user.click(screen.getAllByRole('button', { name: 'studio.project.remove' })[1])
    await user.click(await screen.findByRole('button', { name: 'buttons.confirm' }))

    await waitFor(() => expect(mocks.removeProject).toHaveBeenCalledWith(mocks.otherProject.id))
  })
})

function renderProjectView() {
  return render(<OverlaysProvider><ProjectView /></OverlaysProvider>)
}
