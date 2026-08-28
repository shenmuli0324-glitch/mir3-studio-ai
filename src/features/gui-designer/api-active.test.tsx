// @vitest-environment happy-dom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useGuiDesignerStatus, useGuiDocumentList } from './api'

const invokeMock = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

function ActiveQueries({ active }: { active: boolean }) {
  useGuiDesignerStatus('project-1', active)
  useGuiDocumentList('project-1', active)
  return null
}

describe('gui designer active queries', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation((command: string) => Promise.resolve(command === 'gui_document_list' ? [] : { available: true }))
  })

  it('starts status and document list requests only after activation', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const view = render(
      <QueryClientProvider client={queryClient}>
        <ActiveQueries active={false} />
      </QueryClientProvider>,
    )
    expect(invokeMock).not.toHaveBeenCalled()

    view.rerender(
      <QueryClientProvider client={queryClient}>
        <ActiveQueries active />
      </QueryClientProvider>,
    )
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledTimes(2)
    })
    expect(invokeMock).toHaveBeenCalledWith('gui_designer_status', { projectId: 'project-1' })
    expect(invokeMock).toHaveBeenCalledWith('gui_document_list', { projectId: 'project-1' })
  })
})
