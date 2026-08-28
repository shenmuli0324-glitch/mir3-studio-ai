/* eslint-disable react/no-use-context */
import type { Context } from 'react'
import { useContext } from 'react'

export function useScope<T>(scope: { Context: Context<T | undefined> }) {
  const value = useContext(scope.Context)
  if (!value)
    throw new Error('Scope not found')
  return value
}
