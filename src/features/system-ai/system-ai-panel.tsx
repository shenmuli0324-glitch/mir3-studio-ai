import type { DomainDraftHandoff } from './ai-handoff'
import type { CapabilityResolution, DomainManifest, DomainMemory, TaskReceipt, TaskScopeLease } from '@/features/devtools/domain/types'
import type { Mir3Project } from '@/features/projects/types'
import type { Mir3BridgeEnvelope } from '@/features/projects/workspace-bridge'
import { ArrowUp, ChevronDown, CircleStop } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { DEV_TOOLS } from '@/features/devtools/devtool-registry'
import { associateDomainDraftComposite, bindSystemSession, getSystemSession, issueTaskScope, listDomainMemories, listDomainSystems, listTaskReceipts, openDomainDraft, resolveUserCapabilities, revokeTaskScope, revokeTaskScopes } from '@/features/devtools/domain/api'
import { bridgeRequestId, ensureHarnessProjectActive, postHarnessBridge, subscribeHarnessBridge } from '@/features/projects/workspace-bridge'
import { draftHandoffs, includeGlobalTaskDraft, markGlobalTaskMcpDisabled, matchesTaskIdentity, registeredGlobalTask, registerGlobalTask, requestGlobalWorkbench, unregisterGlobalTask } from './ai-handoff'
import { compensateGlobalDraftSetup } from './global-draft-compensation'
import { appendScopedUserRequest, buildGlobalTaskHandoff, projectTaskMessages, taskGoalFromMessages } from './global-task-handoff'
import { retireSourceTaskScope } from './global-task-recovery'
import { currentScopeLease, includeScopeLeaseDraft, manageScopeLease, stopScopeLease } from './scope-lease-manager'
import { assertSystemTaskScopeLease, buildSystemTaskRenewalContract, buildSystemTaskScopeContract, systemTaskSafetyInstructions } from './system-task-scope'

interface AiMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
}

interface SessionSnapshot {
  nodes?: unknown[]
  partial?: unknown
  runningCalls?: unknown[]
  pending?: PendingInteraction[]
  running?: boolean
  openError?: string | null
  promptError?: string | null
}

export function SystemAiPanel({ project, manifest, selectedPath, selectedResourceId, draftId, onDraftHandoff }: {
  project: Mir3Project
  manifest: DomainManifest
  selectedPath?: string | null
  selectedResourceId?: string | null
  draftId?: string | null
  onDraftHandoff?: (handoff: DomainDraftHandoff) => Promise<void>
}) {
  const { t } = useTranslation()
  const projectRoot = project.root
  const projectWorkspaceRoot = project.activeWorkspaceRoot
  const [taskId, setTaskId] = useState(() => activeSystemTaskId(project.id, manifest.systemId))
  const [sessionId, setSessionId] = useState('')
  const [connected, setConnected] = useState(false)
  const [sessionReady, setSessionReady] = useState(false)
  const [running, setRunning] = useState(false)
  const [pending, setPending] = useState<PendingInteraction[]>([])
  const [runningCalls, setRunningCalls] = useState<unknown[]>([])
  const [taskToolCalls, setTaskToolCalls] = useState<unknown[]>([])
  const [messages, setMessages] = useState<AiMessage[]>([])
  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [scopeDraftId, setScopeDraftId] = useState<string | null>(null)
  const [activeMemories, setActiveMemories] = useState<DomainMemory[]>([])
  const [reusableReceipts, setReusableReceipts] = useState<TaskReceipt[]>([])
  const [resolvedCapabilities, setResolvedCapabilities] = useState<CapabilityResolution[]>([])
  const [globalWriteSystems, setGlobalWriteSystems] = useState<string[]>([])
  const [globalPending, setGlobalPending] = useState(false)
  const resumedSessionRef = useRef('')
  const lastSequenceRef = useRef(new Map<string, number>())
  const awaitingSnapshotRef = useRef(new Set<string>())
  const establishedSessionsRef = useRef(new Set<string>())
  const expectedSessionRef = useRef('')

  const usedCapabilities = extractUsedCapabilities(taskToolCalls, manifest)
    .filter(capability => capability.writeSystems.length > 0)

  useEffect(() => {
    let cancelled = false
    void getSystemSession(project.id, taskId).then(async (binding) => {
      if (!cancelled && binding) {
        if (binding.systemId !== manifest.systemId || binding.pluginVersion !== manifest.version) {
          await revokeTaskScopes(project.id, binding.taskId)
          if (cancelled)
            return
          const nextTaskId = createSystemTaskId(project.id, manifest.systemId)
          rememberSystemTaskId(project.id, manifest.systemId, nextTaskId)
          expectedSessionRef.current = ''
          setSessionId('')
          setTaskId(nextTaskId)
          return
        }
        expectedSessionRef.current = binding.sessionId
        setSessionId(binding.sessionId)
      }
    }).catch(reason => setError(String(reason)))
    return () => {
      cancelled = true
    }
  }, [manifest.systemId, manifest.version, project.id, taskId])

  useEffect(() => {
    void Promise.all([
      listDomainMemories(project.id, manifest.systemId, true),
      listTaskReceipts(project.id, manifest.systemId),
      resolveUserCapabilities(project.id, manifest.systemId),
    ]).then(([active, receipts, capabilities]) => {
      setActiveMemories(active)
      setReusableReceipts(receipts.filter(receipt => isSuccessfulReceipt(receipt.status)).slice(0, 6))
      setResolvedCapabilities(capabilities.slice(0, 12))
    }).catch(reason => setError(String(reason)))
  }, [manifest.systemId, project.id, taskId])

  useEffect(() => {
    const unsubscribe = subscribeHarnessBridge((message) => {
      if (message.type === 'mir3/plugin.ready' || message.type === 'mir3/bridge.description')
        setConnected(true)
      const globalTask = registeredGlobalTask(message)
      if (globalTask) {
        for (const handoff of draftHandoffs(message, globalTask)) {
          includeGlobalTaskDraft(globalTask, handoff.draftId)
          includeScopeLeaseDraft(globalTask, handoff.draftId)
        }
      }
      if (globalTask && message.type === 'mir3/globalSession.cancelled') {
        void stopScopeLease(globalTask)
        unregisterGlobalTask(globalTask)
      }
      else if (globalTask && message.type === 'mir3/bridge.error') {
        void stopScopeLease(globalTask)
        markGlobalTaskMcpDisabled(globalTask, bridgeError(message))
      }
      const identity = {
        projectId: project.id,
        systemId: manifest.systemId,
        taskId,
        sessionId: expectedSessionRef.current,
        allowedSystems: [manifest.systemId],
      }
      if (!expectedSessionRef.current || !matchesTaskIdentity(message, identity))
        return
      if (message.sessionId) {
        const lastSequence = lastSequenceRef.current.get(message.sessionId) ?? -1
        const waitingForFullSnapshot = awaitingSnapshotRef.current.has(message.sessionId) && message.type === 'mir3/systemSession.snapshot'
        if (message.sequence <= lastSequence && !waitingForFullSnapshot)
          return
        lastSequenceRef.current.set(message.sessionId, message.sequence)
      }
      if (message.type === 'mir3/bridge.error') {
        const reason = bridgeError(message)
        setError(reason)
        setRunning(false)
        void stopScopeLease(identity)
        if (!establishedSessionsRef.current.has(message.sessionId) && isRecoverableSessionStartupError(reason)) {
          const nextTaskId = createSystemTaskId(project.id, manifest.systemId)
          rememberSystemTaskId(project.id, manifest.systemId, nextTaskId)
          expectedSessionRef.current = ''
          resumedSessionRef.current = ''
          setSessionId('')
          setSessionReady(false)
          setTaskId(nextTaskId)
        }
        return
      }
      if (message.type === 'mir3/systemSession.created') {
        establishedSessionsRef.current.add(message.sessionId)
        setConnected(true)
        setSessionReady(false)
        if (message.sessionId) {
          awaitingSnapshotRef.current.add(message.sessionId)
          postSessionMessage('mir3/systemSession.snapshot', project.id, manifest.systemId, taskId, message.sessionId, {})
        }
      }
      if (message.type === 'mir3/systemSession.resumed') {
        establishedSessionsRef.current.add(message.sessionId)
        setConnected(true)
        setSessionReady(true)
        const snapshot = message.payload as SessionSnapshot
        setTaskToolCalls(value => mergeToolCalls(value, [...(snapshot.runningCalls ?? []), ...snapshotToolCalls(snapshot.nodes ?? [])]))
        applySnapshot(snapshot, setMessages, setRunning, setPending, setRunningCalls, setError)
      }
      if (message.sessionId && awaitingSnapshotRef.current.has(message.sessionId) && message.type !== 'mir3/systemSession.snapshot')
        return
      if (message.type === 'mir3/systemSession.cancelled' || message.type === 'mir3/systemSession.completed')
        setRunning(false)
      if (message.type === 'mir3/systemSession.cancelled' || message.type === 'mir3/systemSession.completed')
        void stopScopeLease(identity)
      for (const handoff of draftHandoffs(message, identity)) {
        includeScopeLeaseDraft(identity, handoff.draftId)
        setScopeDraftId(handoff.draftId)
        if (onDraftHandoff)
          void onDraftHandoff(handoff).catch(reason => setError(String(reason)))
      }
      if (message.type === 'mir3/systemSession.snapshot') {
        if (message.sessionId)
          awaitingSnapshotRef.current.delete(message.sessionId)
        setSessionReady(true)
        const snapshot = message.payload as SessionSnapshot
        setTaskToolCalls(value => mergeToolCalls(value, [...(snapshot.runningCalls ?? []), ...snapshotToolCalls(snapshot.nodes ?? [])]))
        applySnapshot(message.payload as SessionSnapshot, setMessages, setRunning, setPending, setRunningCalls, setError)
      }
    })
    postHarnessBridge({
      type: 'mir3/bridge.describe',
      projectId: project.id,
      systemId: manifest.systemId,
      taskId,
      sessionId: '',
      payload: {},
    })
    return () => {
      unsubscribe()
      const activeSessionId = expectedSessionRef.current
      if (activeSessionId)
        void stopScopeLease({ projectId: project.id, taskId, sessionId: activeSessionId })
    }
  }, [manifest.systemId, onDraftHandoff, project.id, taskId])

  useEffect(() => {
    if (!connected || !sessionId || resumedSessionRef.current === sessionId)
      return
    let cancelled = false
    resumedSessionRef.current = sessionId
    lastSequenceRef.current.delete(sessionId)
    void ensureHarnessProjectActive({ id: project.id, root: projectRoot, activeWorkspaceRoot: projectWorkspaceRoot })
      .then(() => {
        if (cancelled)
          return
        if (!postSessionMessage('mir3/systemSession.resume', project.id, manifest.systemId, taskId, sessionId, {}))
          throw new Error('HARNESS_BRIDGE_UNAVAILABLE: system Session resume was not delivered')
      })
      .catch((reason) => {
        if (!cancelled) {
          resumedSessionRef.current = ''
          setError(String(reason))
        }
      })
    return () => {
      cancelled = true
    }
  }, [connected, manifest.systemId, project.id, projectRoot, projectWorkspaceRoot, sessionId, taskId])

  async function sendPrompt() {
    const content = input.trim()
    if (!content)
      return
    if (!connected) {
      setError(t('studio.devtools.ai.unavailable'))
      return
    }
    try {
      await ensureHarnessProjectActive(project)
    }
    catch (reason) {
      setError(String(reason))
      return
    }
    if (globalWriteSystems.length > 0) {
      setInput('')
      await openGlobalTask(content)
      return
    }
    if (sessionId && !sessionReady) {
      setError(t('studio.devtools.ai.resuming'))
      return
    }
    setInput('')
    setError(null)
    setMessages(value => [...value, { id: bridgeRequestId(), role: 'user', content }])
    setRunning(true)
    let activeSessionId = sessionId
    if (!activeSessionId) {
      activeSessionId = `mir3-system-${bridgeRequestId()}`
      expectedSessionRef.current = activeSessionId
      resumedSessionRef.current = activeSessionId
      setSessionId(activeSessionId)
    }
    const leaseIdentity = { projectId: project.id, taskId, sessionId: activeSessionId }
    let activeLease = currentScopeLease(leaseIdentity)
    if (!activeLease || scopeDraftId !== (draftId ?? null)) {
      try {
        if (activeLease)
          await stopScopeLease(leaseIdentity)
        const manifests = await listDomainSystems()
        const contract = buildSystemTaskScopeContract(manifest, taskId, draftId, manifests)
        const issued = await issueTaskScope(
          project.id,
          contract.taskId,
          contract.readSystems,
          [contract.systemId],
          contract.draftIds,
          contract.pluginVersions,
        )
        try {
          activeLease = assertSystemTaskScopeLease(issued, contract)
        }
        catch (reason) {
          await revokeTaskScope(project.id, issued.token).catch(() => {})
          throw reason
        }
        manageSystemLease(activeLease, leaseIdentity, project, manifest, draftId, reason => setError(String(reason)))
        setScopeDraftId(draftId ?? null)
      }
      catch (reason) {
        setError(String(reason))
        setRunning(false)
        return
      }
    }
    const activeScopeToken = activeLease.token
    if (!sessionId) {
      const now = Date.now()
      await bindSystemSession(project.id, {
        taskId,
        systemId: manifest.systemId,
        sessionId: activeSessionId,
        pluginVersion: manifest.version,
        draftId,
        status: 'active',
        updatedAt: now,
      })
      const posted = postSessionMessage('mir3/systemSession.create', project.id, manifest.systemId, taskId, activeSessionId, {
        cwd: project.activeWorkspaceRoot,
        prompt: scopedPrompt(content, manifest, project, selectedPath, selectedResourceId, draftId, activeScopeToken, activeMemories, reusableReceipts, resolvedCapabilities),
      })
      if (!posted) {
        void stopScopeLease(leaseIdentity)
        handleUnavailable()
      }
      return
    }
    const posted = postSessionMessage('mir3/systemSession.prompt', project.id, manifest.systemId, taskId, activeSessionId, {
      content: scopedPrompt(content, manifest, project, selectedPath, selectedResourceId, draftId, activeScopeToken, activeMemories, reusableReceipts, resolvedCapabilities),
      mode: 'queue',
    })
    if (!posted) {
      void stopScopeLease(leaseIdentity)
      handleUnavailable()
    }
  }

  function handleUnavailable() {
    setError(t('studio.devtools.ai.unavailable'))
    setRunning(false)
  }

  function cancel() {
    if (sessionId) {
      postSessionMessage('mir3/systemSession.cancel', project.id, manifest.systemId, taskId, sessionId, {})
      void stopScopeLease({ projectId: project.id, taskId, sessionId })
    }
  }

  async function openGlobalTask(content: string) {
    setGlobalPending(true)
    setError(null)
    const createdDraftIds: string[] = []
    let associatedDraft: { draftId: string, systemId: string, pluginVersion: string, compositeId: string } | null = null
    let globalIdentity: { projectId: string, taskId: string, sessionId: string } | null = null
    try {
      await ensureHarnessProjectActive(project)
      const [manifests, taskReceipts] = await Promise.all([
        listDomainSystems(),
        listTaskReceipts(project.id, manifest.systemId),
      ])
      const writeSystems = uniqueStrings([manifest.systemId, ...globalWriteSystems])
      const readSystems = domainReadScope(manifests, writeSystems)
      const pluginVersions = domainPluginVersions(manifests, readSystems)
      const globalTaskId = `global-${taskId}-${bridgeRequestId()}`
      const globalSessionId = `global-${bridgeRequestId()}`
      const sourceSessionId = sessionId || globalSessionId
      const compositeId = `composite-${globalTaskId}`
      const draftIds: string[] = []
      for (const systemId of writeSystems) {
        const version = pluginVersions[systemId]
        if (systemId === manifest.systemId && draftId) {
          await associateDomainDraftComposite(project.id, draftId, systemId, version, compositeId)
          associatedDraft = { draftId, systemId, pluginVersion: version, compositeId }
          draftIds.push(draftId)
          continue
        }
        const draft = await openDomainDraft(
          project.id,
          systemId,
          version,
          t('studio.devtools.ai.global_draft_intent', { system: t(`studio.devtools.tool.${systemId}.title`) }),
          compositeId,
        )
        createdDraftIds.push(draft.id)
        draftIds.push(draft.id)
      }
      const lease = await issueTaskScope(
        project.id,
        globalTaskId,
        readSystems,
        writeSystems,
        draftIds,
        pluginVersions,
      )
      globalIdentity = { projectId: project.id, taskId: globalTaskId, sessionId: globalSessionId }
      manageGlobalLease(lease, globalIdentity, project, manifest.systemId, reason => setError(String(reason)))
      const sourceReceipts = taskReceipts.filter(receipt => receipt.taskId === taskId)
      const handoff = buildGlobalTaskHandoff({
        source: {
          projectId: project.id,
          systemId: manifest.systemId,
          taskId,
          sessionId: sourceSessionId,
        },
        explicitSummary: {
          goal: currentTaskGoal(messages),
          decisions: usedCapabilities.map(capability => `capability:${capability.id}@${capability.version}`),
        },
        taskState: {
          completedOperations: taskToolCalls.map(toolCallLabel),
          constraints: [
            `writeSystems:${writeSystems.join(',')}`,
            `readSystems:${readSystems.join(',')}`,
          ],
          openQuestions: pending.map(interaction => `${interaction.kind}:${interaction.key}`),
          unfinishedSteps: runningCalls.map(toolCallLabel),
        },
        receipts: sourceReceipts,
        references: {
          receiptIds: sourceReceipts.map(receipt => receipt.id),
          resourceIds: optionalValue(selectedResourceId),
          relativePaths: optionalValue(selectedPath),
          draftIds: lease.draftIds,
        },
        pluginVersions: lease.pluginVersions,
        allowedReadSystems: lease.readSystems,
        allowedWriteSystems: lease.writeSystems,
      })
      registerGlobalTask({
        ...globalIdentity,
        systemId: manifest.systemId,
        compositeId,
        allowedSystems: readSystems,
        allowedWriteSystems: writeSystems,
        draftIds: lease.draftIds,
        handoff,
      })
      if (sessionId) {
        await retireSourceTaskScope(
          { projectId: project.id, taskId, sessionId: sourceSessionId },
          { revokeTask: revokeTaskScopes },
        )
      }
      const structuredContext = {
        ...handoff,
        scopeToken: lease.token,
        compositeId,
        returnTo: {
          view: 'devtools',
          projectId: project.id,
          systemId: manifest.systemId,
          resourceId: selectedResourceId,
          draftId,
        },
      }
      const posted = postHarnessBridge({
        type: 'mir3/globalSession.create',
        projectId: project.id,
        systemId: manifest.systemId,
        taskId: globalTaskId,
        sessionId: globalSessionId,
        payload: {
          cwd: project.activeWorkspaceRoot,
          prompt: `${t('studio.devtools.ai.global_context')}\n${JSON.stringify(structuredContext)}\n\n${content}`,
          structuredContext,
        },
      })
      if (!posted) {
        await stopScopeLease(globalIdentity)
        unregisterGlobalTask(globalIdentity)
        throw new Error(t('studio.devtools.ai.unavailable'))
      }
      else {
        requestGlobalWorkbench(globalIdentity)
      }
    }
    catch (reason) {
      if (globalIdentity) {
        await stopScopeLease(globalIdentity).catch(() => {})
        unregisterGlobalTask(globalIdentity)
      }
      const compensationErrors = await compensateGlobalDraftSetup(project.id, createdDraftIds, associatedDraft)
      setError([String(reason), ...compensationErrors].join(' | '))
    }
    finally {
      setGlobalPending(false)
    }
  }

  function toggleGlobalWriteSystem(systemId: string) {
    setGlobalWriteSystems((value) => {
      if (value.includes(systemId))
        return value.filter(item => item !== systemId)
      return [...value, systemId]
    })
  }

  function respond(pendingKey: string, response: unknown) {
    if (sessionId)
      postSessionMessage('mir3/systemSession.respond', project.id, manifest.systemId, taskId, sessionId, { pendingKey, response })
  }

  return (
    <aside className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden border-l border-line bg-panel">
      <div className="min-h-0 min-w-0 flex-1 space-y-3 overflow-x-hidden overflow-y-auto px-4 py-5">
        <If cond={messages.length > 0}>
          {messages.map(message => <AiBubble key={message.id} message={message} />)}
        </If>
        <If cond={running}>
          <p className="text-[11px] text-accent">{t('studio.devtools.ai.running')}</p>
        </If>
        <If cond={pending.length > 0}>
          <div className="space-y-2">
            {pending.map(interaction => <PendingCard key={interaction.key} interaction={interaction} onRespond={response => respond(interaction.key, response)} />)}
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
            placeholder={t('studio.devtools.ai.placeholder')}
            aria-label={t('studio.devtools.ai.placeholder')}
            onChange={event => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                void sendPrompt()
              }
            }}
          />
          <div className="flex items-end justify-between gap-2 px-1 pb-0.5">
            <GlobalScopePicker manifest={manifest} selected={globalWriteSystems} onToggle={toggleGlobalWriteSystem} />
            <If cond={running && globalWriteSystems.length === 0} else={<Button isIconOnly size="sm" className="size-8 shrink-0 rounded-full bg-accent text-white" isDisabled={input.trim().length === 0 || globalPending} isPending={globalPending} aria-label={t('studio.devtools.ai.send')} onPress={() => void sendPrompt()}><ArrowUp className="size-4" /></Button>}>
              <Button isIconOnly size="sm" variant="ghost" className="size-8 shrink-0 rounded-full" aria-label={t('studio.devtools.ai.cancel')} onPress={cancel}><CircleStop className="size-4" /></Button>
            </If>
          </div>
        </div>
      </div>
    </aside>
  )
}

function GlobalScopePicker({ manifest, selected, onToggle }: { manifest: DomainManifest, selected: string[], onToggle: (systemId: string) => void }) {
  const { t } = useTranslation()
  return (
    <details className="group relative min-w-0">
      <summary className="flex h-7 max-w-52 cursor-pointer list-none items-center gap-1 rounded-md px-2 text-[10px] text-muted hover:bg-panel-hover hover:text-ink">
        <span className="truncate">{t('studio.devtools.ai.global_scope_summary', { count: selected.length + 1 })}</span>
        <ChevronDown className="size-3 shrink-0 transition-transform group-open:rotate-180" />
      </summary>
      <div className="absolute bottom-9 left-0 z-30 max-h-64 w-64 overflow-auto rounded-xl border border-line bg-panel p-2 shadow-2xl">
        <p className="px-2 pb-2 text-[10px] text-muted">{t('studio.devtools.ai.global_scope')}</p>
        <label className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[10px] text-ink">
          <input type="checkbox" checked disabled />
          {t(`studio.devtools.tool.${manifest.systemId}.title`)}
        </label>
        {globalSelectableSystems(manifest.systemId).map(systemId => (
          <label key={systemId} className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[10px] text-ink hover:bg-panel-hover">
            <input type="checkbox" checked={selected.includes(systemId)} onChange={() => onToggle(systemId)} />
            <span className="truncate">{t(`studio.devtools.tool.${systemId}.title`)}</span>
          </label>
        ))}
      </div>
    </details>
  )
}

function AiBubble({ message }: { message: AiMessage }) {
  return <div className={aiBubbleClass(message.role)} dir="auto">{message.content}</div>
}

function isSuccessfulReceipt(status: string): boolean {
  return status === 'applied'
}

function aiBubbleClass(role: AiMessage['role']) {
  if (role === 'user')
    return 'ml-8 max-w-full whitespace-pre-wrap break-words rounded-xl bg-accent px-3 py-2 text-xs leading-5 text-white [overflow-wrap:anywhere]'
  return 'mr-4 max-w-full whitespace-pre-wrap break-words rounded-xl border border-line bg-panel2 px-3 py-2 text-xs leading-5 text-ink [overflow-wrap:anywhere]'
}

function optionalValue(value?: string | null) {
  if (value)
    return [value]
  return []
}

function currentTaskGoal(messages: AiMessage[]): string | null {
  return taskGoalFromMessages(messages)
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)]
}

function globalSelectableSystems(currentSystemId: string): string[] {
  return DEV_TOOLS.map(tool => tool.id).filter(systemId => systemId !== currentSystemId)
}

function domainPluginVersions(manifests: DomainManifest[], systemIds: string[]): Record<string, string> {
  const versions: Record<string, string> = {}
  for (const systemId of systemIds) {
    const manifest = manifests.find(item => item.systemId === systemId)
    if (!manifest)
      throw new Error(`GLOBAL_SCOPE_DOMAIN_MISSING: ${systemId}`)
    versions[systemId] = manifest.version
  }
  return versions
}

function domainReadScope(manifests: DomainManifest[], writeSystems: string[]): string[] {
  const systems = new Set(writeSystems)
  const pending = [...writeSystems]
  while (pending.length > 0) {
    const systemId = pending.shift()!
    const manifest = manifests.find(item => item.systemId === systemId)
    if (!manifest)
      throw new Error(`GLOBAL_SCOPE_DOMAIN_MISSING: ${systemId}`)
    for (const dependency of manifest.dependencies) {
      if (systems.has(dependency))
        continue
      systems.add(dependency)
      pending.push(dependency)
    }
  }
  return [...systems]
}

function extractUsedCapabilities(calls: unknown[], manifest: DomainManifest) {
  return manifest.capabilities.filter((capability) => {
    return calls.some(call => serializeToolCall(call).includes(capability.id))
  })
}

function mergeToolCalls(current: unknown[], incoming: unknown[]) {
  const merged = new Map<string, unknown>()
  current.forEach(call => merged.set(toolCallKey(call), call))
  incoming.forEach(call => merged.set(toolCallKey(call), call))
  return [...merged.values()]
}

function snapshotToolCalls(nodes: unknown[]) {
  const calls: unknown[] = []
  nodes.forEach(node => collectToolCalls(node, calls, new Set()))
  return calls
}

function collectToolCalls(value: unknown, calls: unknown[], visited: Set<object>) {
  if (!value || typeof value !== 'object' || visited.has(value))
    return
  visited.add(value)
  if (Array.isArray(value)) {
    value.forEach(item => collectToolCalls(item, calls, visited))
    return
  }
  const record = value as Record<string, unknown>
  if (record.type === 'tool' || record.kind === 'tool' || record.tool != null || record.toolName != null) {
    calls.push(record)
    return
  }
  Object.values(record).forEach(item => collectToolCalls(item, calls, visited))
}

function serializeToolCall(call: unknown) {
  if (typeof call === 'string')
    return call
  try {
    return JSON.stringify(call)
  }
  catch {
    return String(call)
  }
}

function postSessionMessage(type: string, projectId: string, systemId: string, taskId: string, sessionId: string, payload: unknown) {
  return postHarnessBridge({ type, projectId, systemId, taskId, sessionId, payload })
}

function manageSystemLease(
  lease: TaskScopeLease,
  identity: { projectId: string, taskId: string, sessionId: string },
  project: Mir3Project,
  manifest: DomainManifest,
  draftId?: string | null,
  onError?: (reason: unknown) => void,
): void {
  manageScopeLease({
    identity,
    lease,
    renew: async (previous) => {
      const contract = buildSystemTaskRenewalContract(manifest, identity.taskId, previous)
      const issued = await issueTaskScope(
        project.id,
        contract.taskId,
        contract.readSystems,
        [contract.systemId],
        contract.draftIds,
        contract.pluginVersions,
      )
      let renewed: TaskScopeLease
      try {
        renewed = assertSystemTaskScopeLease(issued, contract)
      }
      catch (reason) {
        await revokeTaskScope(project.id, issued.token).catch(() => {})
        throw reason
      }
      const posted = postSessionMessage(
        'mir3/systemSession.prompt',
        project.id,
        manifest.systemId,
        identity.taskId,
        identity.sessionId,
        {
          content: `[MIR3 Scope Renewal] scopeToken=${renewed.token}; draft=${draftId ?? 'none'}; expiresAt=${renewed.expiresAt}.`,
          mode: 'steer',
        },
      )
      if (!posted) {
        await revokeTaskScope(project.id, renewed.token)
        throw new Error('HARNESS_BRIDGE_UNAVAILABLE: renewed scope was not delivered')
      }
      return renewed
    },
    revoke: value => revokeTaskScope(project.id, value.token),
    onError,
  })
}

function manageGlobalLease(
  lease: TaskScopeLease,
  identity: { projectId: string, taskId: string, sessionId: string },
  project: Mir3Project,
  sourceSystemId: string,
  onError?: (reason: unknown) => void,
): void {
  manageScopeLease({
    identity,
    lease,
    renew: async (previous) => {
      const renewed = await issueTaskScope(
        project.id,
        identity.taskId,
        previous.readSystems,
        previous.writeSystems,
        previous.draftIds,
        previous.pluginVersions,
      )
      const posted = postSessionMessage(
        'mir3/globalSession.prompt',
        project.id,
        sourceSystemId,
        identity.taskId,
        identity.sessionId,
        {
          content: `[MIR3 Scope Renewal] scopeToken=${renewed.token}; expiresAt=${renewed.expiresAt}.`,
          mode: 'steer',
        },
      )
      if (!posted) {
        await revokeTaskScope(project.id, renewed.token)
        throw new Error('HARNESS_BRIDGE_UNAVAILABLE: renewed global scope was not delivered')
      }
      return renewed
    },
    revoke: value => revokeTaskScope(project.id, value.token),
    onError,
  })
}

function activeSystemTaskId(projectId: string, systemId: string) {
  const key = systemTaskStorageKey(projectId, systemId)
  const stored = window.localStorage.getItem(key)
  if (stored)
    return stored
  const taskId = createSystemTaskId(projectId, systemId)
  window.localStorage.setItem(key, taskId)
  return taskId
}

function createSystemTaskId(projectId: string, systemId: string) {
  return `system-${projectId}-${systemId}-${bridgeRequestId()}`
}

function rememberSystemTaskId(projectId: string, systemId: string, taskId: string) {
  window.localStorage.setItem(systemTaskStorageKey(projectId, systemId), taskId)
}

function systemTaskStorageKey(projectId: string, systemId: string) {
  return `mir3-system-task:${projectId}:${systemId}`
}

function scopedPrompt(
  content: string,
  manifest: DomainManifest,
  project: Mir3Project,
  selectedPath?: string | null,
  selectedResourceId?: string | null,
  draftId?: string | null,
  scopeToken?: string,
  memories: DomainMemory[] = [],
  receipts: TaskReceipt[] = [],
  capabilities: CapabilityResolution[] = [],
) {
  const context = [
    `[MIR3 System Scope] project=${project.id}; system=${manifest.systemId}; plugin=${manifest.version}; writeSystems=${manifest.systemId}; readSystems=${[manifest.systemId, ...manifest.dependencies].join(',')}; draft=${draftId ?? 'none'}; selectedFile=${selectedPath ?? 'none'}; selectedResource=${selectedResourceId ?? 'none'}; scopeToken=${scopeToken ?? 'none'}.`,
    systemTaskSafetyInstructions(manifest),
  ]
  if (memories.length > 0)
    context.push(`[Activated domain memories]\n${memories.slice(0, 8).map(memory => `- ${memory.summary}`).join('\n')}`)
  if (receipts.length > 0) {
    context.push(`[Relevant task receipts]\n${receipts.slice(0, 6).map(receipt => (
      `- id=${receipt.id}; status=${receipt.status}; draft=${receipt.draftId ?? 'none'}; summary=${receipt.summary.slice(0, 160)}`
    )).join('\n')}`)
  }
  if (capabilities.length > 0) {
    context.push(`[Resolved reusable capabilities]\n${capabilities.slice(0, 12).map(item => (
      `- ${item.capability.id}@${item.capability.version}; scope=${item.resolvedScope}; writes=${item.capability.writeSystems.join(',')}`
    )).join('\n')}`)
  }
  return appendScopedUserRequest(context, content)
}

function applySnapshot(
  snapshot: SessionSnapshot,
  setMessages: (value: AiMessage[]) => void,
  setRunning: (value: boolean) => void,
  setPending: (value: PendingInteraction[]) => void,
  setRunningCalls: (value: unknown[]) => void,
  setError: (value: string | null) => void,
) {
  const projected = projectMessages(snapshot.nodes ?? [], snapshot.partial)
  if (projected.length > 0)
    setMessages(projected)
  setRunning(Boolean(snapshot.running))
  setPending(snapshot.pending ?? [])
  setRunningCalls(snapshot.runningCalls ?? [])
  setError(snapshot.openError ?? snapshot.promptError ?? null)
}

function toolCallLabel(call: unknown) {
  if (typeof call === 'string')
    return call
  if (call && typeof call === 'object') {
    const value = call as Record<string, unknown>
    return String(value.name ?? value.tool ?? value.id ?? JSON.stringify(value))
  }
  return String(call)
}

function toolCallKey(call: unknown) {
  if (call && typeof call === 'object') {
    const value = call as Record<string, unknown>
    return String(value.id ?? value.callId ?? JSON.stringify(value))
  }
  return String(call)
}

function projectMessages(nodes: unknown[], partial?: unknown): AiMessage[] {
  return projectTaskMessages(nodes, partial)
}

function bridgeError(message: Mir3BridgeEnvelope) {
  const payload = message.payload as { code?: string, message?: string }
  return [payload.code, payload.message].filter(Boolean).join(': ')
}

function isRecoverableSessionStartupError(reason: string): boolean {
  return reason.includes('PROJECT_SCOPE_')
    || reason.includes('SESSION_NOT_FOUND')
    || reason.includes('SESSION_BINDING')
    || reason.includes('SYSTEM_SESSION_CREATE_FAILED')
}

interface PendingInteraction {
  key: string
  kind: 'approval' | 'question' | string
  payload: Record<string, unknown>
}

function PendingCard({ interaction, onRespond }: { interaction: PendingInteraction, onRespond: (response: unknown) => void }) {
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
