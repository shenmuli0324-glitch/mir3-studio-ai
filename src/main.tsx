import { OverlaysProvider } from '@overlastic/react'
import { QueryClientProvider } from '@tanstack/react-query'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { ToastProvider } from './components/toast-provider'
import { queryClient } from './config/client'
import { DevToolsPreview } from './devtools-preview'
import { App } from './layout'
import '@/utils/logger'
import './style/main.css'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <OverlaysProvider>
          {rootContent()}
        </OverlaysProvider>
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
)

function rootContent() {
  const preview = new URLSearchParams(window.location.search).get('preview')
  if (import.meta.env.DEV && preview === 'devtools')
    return <DevToolsPreview />
  return <App />
}
