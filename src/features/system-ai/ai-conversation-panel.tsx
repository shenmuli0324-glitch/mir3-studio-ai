import type { ReactNode } from 'react'
import { ArrowUp, CircleStop } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'

export interface AiConversationMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
}

export interface AiPendingInteraction {
  key: string
  kind: 'approval' | 'question' | string
  payload: Record<string, unknown>
}

export function AiConversationPanel({
  messages,
  running,
  pending,
  error,
  input,
  placeholder,
  sending = false,
  sendDisabled = false,
  scopeControl,
  emptyState,
  onInputChange,
  onSend,
  onCancel,
  onRespond,
}: {
  messages: AiConversationMessage[]
  running: boolean
  pending: AiPendingInteraction[]
  error?: string | null
  input: string
  placeholder: string
  sending?: boolean
  sendDisabled?: boolean
  scopeControl?: ReactNode
  emptyState?: ReactNode
  onInputChange: (value: string) => void
  onSend: () => void
  onCancel: () => void
  onRespond: (pendingKey: string, response: unknown) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden bg-panel">
      <div className="min-h-0 min-w-0 flex-1 space-y-3 overflow-x-hidden overflow-y-auto px-4 py-5">
        <If cond={messages.length === 0 && !running}>{emptyState}</If>
        <If cond={messages.length > 0}>
          {messages.map(message => <AiBubble key={message.id} message={message} />)}
        </If>
        <If cond={running}>
          <p className="text-[11px] text-accent">{t('studio.devtools.ai.running')}</p>
        </If>
        <If cond={pending.length > 0}>
          <div className="space-y-2">
            {pending.map(interaction => <PendingCard key={interaction.key} interaction={interaction} onRespond={response => onRespond(interaction.key, response)} />)}
          </div>
        </If>
        <If cond={error != null}><p className="max-w-full whitespace-pre-wrap break-words text-[11px] leading-5 text-danger [overflow-wrap:anywhere]">{error}</p></If>
      </div>
      <div className="shrink-0 p-3">
        <div className="rounded-2xl border border-line bg-panel2 p-2 shadow-[0_8px_32px_rgba(0,0,0,0.12)] focus-within:border-accent/70">
          <textarea
            rows={4}
            className="w-full resize-none overflow-x-hidden bg-transparent px-2 py-1.5 text-xs leading-5 text-ink outline-none placeholder:text-muted"
            value={input}
            placeholder={placeholder}
            aria-label={placeholder}
            onChange={event => onInputChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                onSend()
              }
            }}
          />
          <div className="flex items-end justify-between gap-2 px-1 pb-0.5">
            <If cond={scopeControl != null} else={<span />}>
              {scopeControl}
            </If>
            <If cond={running} else={<Button isIconOnly size="sm" className="size-8 shrink-0 rounded-full bg-accent text-white" isDisabled={input.trim().length === 0 || sending || sendDisabled} isPending={sending} aria-label={t('studio.devtools.ai.send')} onPress={onSend}><ArrowUp className="size-4" /></Button>}>
              <Button isIconOnly size="sm" variant="ghost" className="size-8 shrink-0 rounded-full" aria-label={t('studio.devtools.ai.cancel')} onPress={onCancel}><CircleStop className="size-4" /></Button>
            </If>
          </div>
        </div>
      </div>
    </div>
  )
}

function AiBubble({ message }: { message: AiConversationMessage }) {
  return <div className={aiBubbleClass(message.role)} dir="auto">{message.content}</div>
}

function aiBubbleClass(role: AiConversationMessage['role']) {
  if (role === 'user')
    return 'ml-8 max-w-full whitespace-pre-wrap break-words rounded-xl bg-accent px-3 py-2 text-xs leading-5 text-white [overflow-wrap:anywhere]'
  return 'mr-4 max-w-full whitespace-pre-wrap break-words rounded-xl border border-line bg-panel2 px-3 py-2 text-xs leading-5 text-ink [overflow-wrap:anywhere]'
}

function PendingCard({ interaction, onRespond }: { interaction: AiPendingInteraction, onRespond: (response: unknown) => void }) {
  const { t } = useTranslation()
  const [answer, setAnswer] = useState('')
  const message = String(interaction.payload.message ?? interaction.payload.question ?? interaction.payload.description ?? '')
  return (
    <div className="border-l-2 border-warning pl-3">
      <strong className="text-xs text-ink">{t('studio.devtools.ai.confirmation')}</strong>
      <p className="mt-1 text-[11px] leading-5 text-muted">{message}</p>
      <If
        cond={interaction.kind === 'approval'}
        else={(
          <div className="mt-2 flex gap-2">
            <input className="min-w-0 flex-1 rounded-lg border border-line bg-panel px-2 py-1.5 text-xs text-ink outline-none" value={answer} aria-label={t('studio.devtools.ai.answer')} onChange={event => setAnswer(event.target.value)} />
            <Button size="sm" className="bg-accent text-white" isDisabled={!answer.trim()} onPress={() => onRespond({ answer })}>{t('studio.devtools.ai.answer_send')}</Button>
          </div>
        )}
      >
        <div className="mt-2 flex gap-2">
          <Button size="sm" className="bg-accent text-white" onPress={() => onRespond({ outcome: 'allowed-once' })}>{t('studio.devtools.ai.allow_once')}</Button>
          <Button size="sm" variant="ghost" className="text-danger" onPress={() => onRespond({ outcome: 'rejected' })}>{t('studio.devtools.ai.reject')}</Button>
        </div>
      </If>
    </div>
  )
}
