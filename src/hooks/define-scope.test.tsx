// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'
import { defineScope } from './define-scope'
import { useScope } from './use-scope'

const TestScope = defineScope(({ active }: { active: boolean }) => {
  const [dirty, setDirty] = useState(false)
  return { active, dirty, setDirty }
})

function ScopeConsumer() {
  const scope = useScope(TestScope)
  return (
    <button type="button" onClick={() => scope.setDirty(true)}>
      {`${scope.active}:${scope.dirty}`}
    </button>
  )
}

describe('defineScope provider props', () => {
  it('updates the active gate without remounting provider state', () => {
    const view = render(
      <TestScope.Provider active>
        <ScopeConsumer />
      </TestScope.Provider>,
    )
    fireEvent.click(screen.getByRole('button'))
    expect(screen.getByRole('button').textContent).toBe('true:true')

    view.rerender(
      <TestScope.Provider active={false}>
        <ScopeConsumer />
      </TestScope.Provider>,
    )
    expect(screen.getByRole('button').textContent).toBe('false:true')
  })
})
