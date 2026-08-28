// @vitest-environment happy-dom

import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { PersistentDevTools } from './persistent-devtools'

const mocks = vi.hoisted(() => ({ loads: 0 }))

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

vi.mock('@/views/devtools-view', async () => {
  const { useState } = await import('react')
  mocks.loads += 1

  function DevToolsView() {
    const [count, setCount] = useState(0)
    return <button type="button" onClick={() => setCount(value => value + 1)}>{`count ${count}`}</button>
  }

  return { DevToolsView }
})

afterEach(() => cleanup())

describe('persistentDevTools', () => {
  it('loads on first mount and preserves its state while hidden', async () => {
    const view = render(<PersistentDevTools mounted={false} active={false} />)
    expect(mocks.loads).toBe(0)
    expect(screen.queryByText('count 0')).toBeNull()

    view.rerender(<PersistentDevTools mounted active />)
    await userEvent.click(await screen.findByRole('button', { name: 'count 0' }))
    expect(mocks.loads).toBe(1)

    view.rerender(<PersistentDevTools mounted active={false} />)
    expect(screen.getByText('count 1')).toBeTruthy()
    expect(screen.getByText('count 1').closest('[aria-hidden="true"]')).toBeTruthy()

    view.rerender(<PersistentDevTools mounted active />)
    expect(screen.getByRole('button', { name: 'count 1' })).toBeTruthy()
    expect(mocks.loads).toBe(1)
  })
})
