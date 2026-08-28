// @vitest-environment happy-dom

import type { CompositeDraftApplyResult } from '../src/features/devtools/domain/types'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CompositeDraftReviewDialog } from '../src/features/devtools/domain/composite-draft-review'
import '../src/i18n'

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('@/utils', () => ({ toast: vi.fn() }))

afterEach(() => {
  cleanup()
  mocks.invoke.mockReset()
  vi.restoreAllMocks()
})

describe('composite Draft joint review', () => {
  it('previews every Draft, validates each one, and applies the complete set after one confirmation', async () => {
    const review = compositeReview(true)
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'draft_composite_preview')
        return Promise.resolve(review)
      if (command === 'draft_composite_apply') {
        return Promise.resolve({
          compositeId: 'composite-1',
          draftIds: ['draft-quest', 'draft-shop'],
          snapshot: { id: 'snapshot-1', createdAt: 3 },
        })
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`))
    })
    const onApplied = vi.fn()
    renderReview(onApplied)

    expect(await screen.findByText('Quest flow')).toBeTruthy()
    expect(screen.getByText('Shop price')).toBeTruthy()
    expect(document.querySelectorAll('[data-composite-draft]')).toHaveLength(2)
    expect(mocks.invoke).toHaveBeenCalledWith('draft_composite_preview', {
      projectId: 'project-1',
      compositeId: 'composite-1',
    })

    await userEvent.click(screen.getByRole('button', { name: 'Confirm once and apply atomically' }))
    await userEvent.click(await screen.findByRole('button', { name: 'Confirm' }))
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('draft_composite_apply', {
      projectId: 'project-1',
      compositeId: 'composite-1',
      drafts: [
        { draftId: 'draft-quest', confirmationToken: 'token-quest' },
        { draftId: 'draft-shop', confirmationToken: 'token-shop' },
      ],
    }))
    await waitFor(() => expect(onApplied).toHaveBeenCalledWith(expect.objectContaining({ compositeId: 'composite-1' })))
  })

  it('keeps the atomic Apply disabled when any Draft validation fails', async () => {
    mocks.invoke.mockResolvedValueOnce(compositeReview(false))
    renderReview(vi.fn())

    expect(await screen.findByText('Quest flow')).toBeTruthy()
    expect(screen.getByText('Missing dependency')).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Confirm once and apply atomically' }) as HTMLButtonElement).disabled).toBe(true)
    expect(mocks.invoke.mock.calls.some(([command]) => command === 'draft_composite_apply')).toBe(false)
  })
})

function renderReview(onApplied: (result: CompositeDraftApplyResult) => void) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  render(
    <QueryClientProvider client={queryClient}>
      <CompositeDraftReviewDialog
        request={{ projectId: 'project-1', compositeId: 'composite-1', taskId: 'task-1', sessionId: 'session-1' }}
        onClose={vi.fn()}
        onApplied={onApplied}
      />
    </QueryClientProvider>,
  )
}

function compositeReview(valid: boolean) {
  return {
    compositeId: 'composite-1',
    drafts: [
      draftReview('draft-quest', 'quest', 'Quest flow', 'token-quest', true),
      draftReview('draft-shop', 'shop', 'Shop price', 'token-shop', valid),
    ],
  }
}

function draftReview(draftId: string, systemId: string, intent: string, confirmationToken: string, valid: boolean) {
  return {
    draftId,
    systemId,
    pluginVersion: '1.3.1',
    confirmation: {
      confirmationToken,
      preview: {
        draft: { id: draftId, intent, revision: 1, status: 'open', createdAt: 1, updatedAt: 2 },
        changes: [{
          path: `引擎/${systemId}.txt`,
          deleted: false,
          baseSha256: 'before',
          newSha256: 'after',
          unifiedDiff: `-${systemId}=1\n+${systemId}=2`,
        }],
        diffHash: `hash-${systemId}`,
      },
    },
    validation: {
      systemId,
      valid,
      ownedFiles: 1,
      writableFiles: 1,
      readonlyFiles: 0,
      missingDependencies: valid ? [] : ['item'],
      diagnostics: valid ? [] : ['Missing dependency'],
    },
  }
}
