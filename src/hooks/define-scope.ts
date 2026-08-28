import { createContext, createElement } from 'react'

export interface Scope<T, P extends object = Record<string, never>> {
  Provider: React.FC<P & {
    children: React.ReactNode | ((value: T) => React.ReactNode)
  }>
  Context: React.Context<T | undefined>
}

export function defineScope<T, P extends object = Record<string, never>>(useSetup: (props: P) => T): Scope<T, P> {
  const Context = createContext<T | undefined>(undefined)

  function Provider({ children, ...props }: P & { children: React.ReactNode | ((value: T) => React.ReactNode) }) {
    const value = useSetup(props as P)
    const child = typeof children === 'function' ? children(value) : children
    return createElement(Context.Provider, { value }, child)
  }

  return { Provider, Context }
}
