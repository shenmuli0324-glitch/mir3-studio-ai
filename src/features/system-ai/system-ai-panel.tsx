import type { DomainDraftHandoff } from './ai-handoff'
import type { CapabilityResolution, DomainManifest, DomainMemory, TaskReceipt, TaskScopeLease, UserCapability } from '@/features/devtools/domain/types'
import type { Mir3Project } from '@/features/projects/types'
import type { Mir3BridgeEnvelope } from '@/features/projects/workspace-bridge'
import { ArrowUp, CircleStop, MagicWand, Plus, Sparkles } from '@gravity-ui/icons'
import { Button } from '@heroui/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { If } from 'react-if-lite'
import { DEV_TOOLS } from '@/features/devtools/devtool-registry'
import { activateMemoryCandidate, associateDomainDraftComposite, bindSystemSession, compileUserCapability, getSystemSession, issueTaskScope, listDomainDrafts, listDomainMemories, listDomainSystems, listMemoryCandidates, listTaskReceipts, openDomainDraft, previewDomainDraft, resolveUserCapabilities, revokeMemoryCandidate, revokeTaskScope, revokeTaskScopes, saveTaskReceipt, setUserCapabilityStatus, validateDomainDraft } from '@/features/devtools/domain/api'
import { bridgeRequestId, postHarnessBridge, subscribeHarnessBridge } from '@/features/projects/workspace-bridge'
import { draftHandoffs, includeGlobalTaskDraft, markGlobalTaskMcpDisabled, matchesTaskIdentity, registeredGlobalTask, registerGlobalTask, requestGlobalWorkbench, unregisterGlobalTask } from './ai-handoff'
import { CapabilityGovernance } from './capability-governance'
import { compensateGlobalDraftSetup } from './global-draft-compensation'
import { appendScopedUserRequest, buildGlobalTaskHandoff, buildTaskSemanticSummary, formatTaskReceiptSummary, projectTaskMessages, sanitizeTaskSemanticList, taskGoalFromMessages } from './global-task-handoff'
import { retireSourceTaskScope } from './global-task-recovery'
import { currentScopeLease, includeScopeLeaseDraft, manageScopeLease, stopScopeLease } from './scope-lease-manager'

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
  const [receiptStatus, setReceiptStatus] = useState<'idle' | 'saving' | 'saved'>('idle')
  const [scopeDraftId, setScopeDraftId] = useState<string | null>(null)
  const [capabilityDraft, setCapabilityDraft] = useState<UserCapability | null>(null)
  const [selectedCapabilityId, setSelectedCapabilityId] = useState('')
  const [memoryCandidates, setMemoryCandidates] = useState<DomainMemory[]>([])
  const [activeMemories, setActiveMemories] = useState<DomainMemory[]>([])
  const [reusableReceipts, setReusableReceipts] = useState<TaskReceipt[]>([])
  const [resolvedCapabilities, setResolvedCapabilities] = useState<CapabilityResolution[]>([])
  const [globalWriteSystems, setGlobalWriteSystems] = useState<string[]>([])
  const [globalPending, setGlobalPending] = useState(false)
  const resumedSessionRef = useRef('')
  const lastSequenceRef = useRef(new Map<string, number>())
  const awaitingSnapshotRef = useRef(new Set<string>())
  const expectedSessionRef = useRef('')

  const usedCapabilities = extractUsedCapabilities(taskToolCalls, manifest)
    .filter(capability => capability.writeSystems.length > 0)

  useEffect(() => {
    let cancelled = false
    void getSystemSession(project.id, taskId).then((binding) => {
      if (!cancelled && binding) {
        expectedSessionRef.current = binding.sessionId
        setSessionId(binding.sessionId)
      }
    }).catch(reason => setError(String(reason)))
    return () => {
      cancelled = true
    }
  }, [project.id, taskId])

  useEffect(() => {
    void Promise.all([
      listMemoryCandidates(project.id, manifest.systemId),
      listDomainMemories(project.id, manifest.systemId, true),
      listTaskReceipts(project.id, manifest.systemId),
      resolveUserCapabilities(project.id, manifest.systemId),
    ]).then(([candidates, active, receipts, capabilities]) => {
      setMemoryCandidates(candidates.filter(memory => memory.sourceTaskId === taskId))
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
        setError(bridgeError(message))
        setRunning(false)
        void stopScopeLease(identity)
        return
      }
      if (message.type === 'mir3/systemSession.created') {
        setConnected(true)
        setSessionReady(false)
        if (message.sessionId) {
          awaitingSnapshotRef.current.add(message.sessionId)
          postSessionMessage('mir3/systemSession.snapshot', project.id, manifest.systemId, taskId, message.sessionId, {})
        }
      }
      if (message.type === 'mir3/systemSession.resumed') {
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
    resumedSessionRef.current = sessionId
    lastSequenceRef.current.delete(sessionId)
    postSessionMessage('mir3/systemSession.resume', project.id, manifest.systemId, taskId, sessionId, {})
  }, [connected, manifest.systemId, project.id, sessionId, taskId])

  async function sendPrompt() {
    const content = input.trim()
    if (!content)
      return
    if (!connected) {
      setError(t('studio.devtools.ai.unavailable'))
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
        activeLease = await issueTaskScope(
          project.id,
          taskId,
          [manifest.systemId, ...manifest.dependencies],
          [manifest.systemId],
          optionalValue(draftId),
          { [manifest.systemId]: manifest.version },
        )
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

  function startNewTask() {
    if (sessionId) {
      postSessionMessage('mir3/systemSession.complete', project.id, manifest.systemId, taskId, sessionId, {})
      void stopScopeLease({ projectId: project.id, taskId, sessionId })
    }
    const nextTaskId = createSystemTaskId(project.id, manifest.systemId)
    rememberSystemTaskId(project.id, manifest.systemId, nextTaskId)
    setTaskId(nextTaskId)
    setSessionId('')
    setMessages([])
    setRunning(false)
    setSessionReady(false)
    setPending([])
    setRunningCalls([])
    setTaskToolCalls([])
    setError(null)
    setReceiptStatus('idle')
    setScopeDraftId(null)
    setCapabilityDraft(null)
    setSelectedCapabilityId('')
    setMemoryCandidates([])
    setActiveMemories([])
    setReusableReceipts([])
    setResolvedCapabilities([])
    setGlobalWriteSystems([])
    setGlobalPending(false)
    lastSequenceRef.current.clear()
    awaitingSnapshotRef.current.clear()
    resumedSessionRef.current = ''
    expectedSessionRef.current = ''
  }

  async function promoteCapability() {
    const sourceDraftId = draftId ?? scopeDraftId
    if (!sourceDraftId) {
      setError(t('studio.devtools.ai.promote_needs_draft'))
      return
    }
    const now = Date.now()
    const summary = messages.slice(-6).map(message => message.content).join('\n').slice(0, 2_000)
    try {
      const [receipts, drafts] = await Promise.all([
        listTaskReceipts(project.id, manifest.systemId),
        listDomainDrafts(project.id),
      ])
      const sourceDraft = drafts.find(item => item.id === sourceDraftId)
      if (sourceDraft?.status !== 'applied') {
        setError(t('studio.devtools.ai.promote_needs_applied_receipt'))
        return
      }
      const receipt = receipts
        .filter(item => item.taskId === taskId && item.draftId === sourceDraftId && isSuccessfulReceipt(item.status))
        .sort((left, right) => right.createdAt - left.createdAt)[0]
      if (!receipt) {
        setError(t('studio.devtools.ai.promote_needs_applied_receipt'))
        return
      }
      if (sessionId) {
        await bindSystemSession(project.id, {
          taskId,
          systemId: manifest.systemId,
          sessionId,
          pluginVersion: manifest.version,
          draftId: sourceDraftId,
          status: 'active',
          updatedAt: now,
        })
      }
      const capability = await compileUserCapability(project.id, {
        receiptId: receipt.id,
        id: `user-${manifest.systemId}-${now}`,
        name: t('studio.devtools.ai.capability_name', { system: t(`studio.devtools.tool.${manifest.systemId}.title`) }),
        description: summary || t('studio.devtools.ai.receipt_empty'),
      })
      setCapabilityDraft(capability)
      setError(null)
    }
    catch (reason) {
      setError(String(reason))
    }
  }

  async function activateCapability() {
    if (!capabilityDraft)
      return
    try {
      const active = await setUserCapabilityStatus(project.id, capabilityDraft.id, capabilityDraft.version, 'active', true)
      setCapabilityDraft(active)
    }
    catch (reason) {
      setError(String(reason))
    }
  }

  async function summarizeTask() {
    const semanticSummary = buildTaskSemanticSummary({
      messages,
      decisions: usedCapabilities.map(capability => `capability:${capability.id}@${capability.version}`),
      completedOperations: taskToolCalls.map(toolCallLabel),
      constraints: [
        `writeSystems:${manifest.systemId}`,
        `readSystems:${[manifest.systemId, ...manifest.dependencies].join(',')}`,
      ],
      openQuestions: pending.map(interaction => `${interaction.kind}:${interaction.key}`),
      unfinishedSteps: runningCalls.map(toolCallLabel),
    })
    const receiptDraftId = draftId ?? scopeDraftId
    setReceiptStatus('saving')
    try {
      const now = Date.now()
      const draftEvidence = await taskDraftEvidence(project.id, manifest.systemId, receiptDraftId)
      const summary = formatTaskReceiptSummary(semanticSummary) || t('studio.devtools.ai.receipt_empty')
      await saveTaskReceipt(project.id, {
        id: `receipt-summary-${taskId}-${stableTextKey(JSON.stringify(semanticSummary))}`,
        taskId,
        systemId: manifest.systemId,
        summary,
        status: taskReceiptStatus(running),
        draftId: receiptDraftId,
        pluginVersions: { [manifest.systemId]: manifest.version },
        evidence: {
          selectedPath,
          sessionId,
          messageCount: messages.length,
          toolCalls: semanticSummary.completedOperations ?? [],
          semanticSummary,
          ...draftEvidence,
        },
        createdAt: now,
      })
      const candidates = await listMemoryCandidates(project.id, manifest.systemId)
      setMemoryCandidates(candidates.filter(memory => memory.sourceTaskId === taskId))
      setReceiptStatus('saved')
    }
    catch (reason) {
      setReceiptStatus('idle')
      setError(String(reason))
    }
  }

  async function reviewMemory(memory: DomainMemory, status: 'active' | 'revoked') {
    try {
      let reviewed: DomainMemory
      if (status === 'active')
        reviewed = await activateMemoryCandidate(project.id, memory.id)
      else
        reviewed = await revokeMemoryCandidate(project.id, memory.id)
      setMemoryCandidates(value => value.filter(item => item.id !== reviewed.id))
      setActiveMemories((value) => {
        if (reviewed.status === 'active')
          return replaceMemory(value, reviewed)
        return value.filter(item => item.id !== reviewed.id)
      })
    }
    catch (reason) {
      setError(String(reason))
    }
  }

  async function openGlobalTask() {
    const sourceSessionId = sessionId
    if (!sourceSessionId) {
      setError(t('studio.devtools.ai.global_needs_session'))
      return
    }
    setGlobalPending(true)
    setError(null)
    const createdDraftIds: string[] = []
    let associatedDraft: { draftId: string, systemId: string, pluginVersion: string, compositeId: string } | null = null
    let globalIdentity: { projectId: string, taskId: string, sessionId: string } | null = null
    try {
      const [manifests, taskReceipts] = await Promise.all([
        listDomainSystems(),
        listTaskReceipts(project.id, manifest.systemId),
      ])
      const writeSystems = uniqueStrings([manifest.systemId, ...globalWriteSystems])
      const readSystems = domainReadScope(manifests, writeSystems)
      const pluginVersions = domainPluginVersions(manifests, readSystems)
      const globalTaskId = `global-${taskId}-${bridgeRequestId()}`
      const globalSessionId = `global-${bridgeRequestId()}`
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
      await retireSourceTaskScope(
        { projectId: project.id, taskId, sessionId: sourceSessionId },
        { revokeTask: revokeTaskScopes },
      )
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
          prompt: `${t('studio.devtools.ai.global_context')}\n${JSON.stringify(structuredContext)}`,
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
    <aside className="flex h-full min-h-0 w-full flex-col border-l border-line bg-panel">
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-line px-4">
        <span>
          <strong className="flex items-center gap-2 text-xs font-semibold text-ink">
            <Sparkles className="size-4 text-accent" />
            {t('studio.devtools.ai.title')}
          </strong>
          <small className="mt-0.5 block text-[10px] text-muted">{t(connectionStatusKey(connected))}</small>
        </span>
        <span className="flex items-center gap-1">
          <span className="rounded-full border border-line bg-panel2 px-2 py-1 text-[9px] text-muted">{draftStatusLabel(t, draftId)}</span>
          <Button isIconOnly size="sm" variant="ghost" aria-label={t('studio.devtools.ai.new_task')} onPress={startNewTask}><Plus className="size-4" /></Button>
        </span>
      </header>
      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-4">
        <If cond={messages.length > 0} else={<AiWelcome manifest={manifest} />}>
          {messages.map(message => <AiBubble key={message.id} message={message} />)}
        </If>
        <If cond={running}>
          <div className="rounded-xl border border-accent/20 bg-accent/5 px-3 py-2 text-xs text-accent">{t('studio.devtools.ai.running')}</div>
        </If>
        <If cond={runningCalls.length > 0}>
          <div className="rounded-xl border border-line bg-panel2 p-3">
            <strong className="text-[10px] uppercase tracking-wider text-muted">{t('studio.devtools.ai.tools')}</strong>
            <div className="mt-2 space-y-1">{runningCalls.map(call => <div key={toolCallKey(call)} className="rounded-md bg-canvas px-2 py-1.5 font-mono text-[9px] text-ink">{toolCallLabel(call)}</div>)}</div>
          </div>
        </If>
        <If cond={pending.length > 0}>
          <div className="space-y-2">
            {pending.map(interaction => <PendingCard key={interaction.key} interaction={interaction} onRespond={response => respond(interaction.key, response)} />)}
          </div>
        </If>
        <If cond={usedCapabilities.length > 0}>
          <div className="rounded-xl border border-line bg-panel2 p-3">
            <label className="block text-[10px] font-semibold uppercase tracking-wider text-muted" htmlFor={`capability-${taskId}`}>{t('studio.devtools.ai.capability_operation')}</label>
            <select
              id={`capability-${taskId}`}
              className="mt-2 w-full rounded-lg border border-line bg-panel px-2 py-2 text-xs text-ink outline-none focus:border-accent"
              value={selectedCapabilityId}
              onChange={event => setSelectedCapabilityId(event.target.value)}
            >
              <option value="">{t('studio.devtools.ai.capability_choose')}</option>
              {usedCapabilities.map(capability => <option key={capability.id} value={capability.id}>{capability.id}</option>)}
            </select>
            <If cond={selectedCapabilityId.length > 0}>
              <CapabilityContract capability={usedCapabilities.find(capability => capability.id === selectedCapabilityId)} />
            </If>
          </div>
        </If>
        <If cond={capabilityDraft != null}>
          <div className="rounded-xl border border-accent/30 bg-accent/5 p-3">
            <strong className="text-xs text-ink">{capabilityDraft?.name}</strong>
            <p className="mt-1 text-[10px] text-muted">
              {capabilityDraft?.id}
              @
              {capabilityDraft?.version}
              {' '}
              ·
              {' '}
              {capabilityDraft?.status}
            </p>
            <p className="mt-2 line-clamp-3 text-[10px] leading-4 text-muted">{capabilityDraft?.description}</p>
            <div className="mt-2 rounded-md border border-line bg-panel px-2 py-1.5 font-mono text-[9px] text-ink">{capabilityDraft?.steps.map(step => step.operation).join(' → ')}</div>
            <If cond={capabilityDraft?.status === 'draft'}><Button className="mt-2 bg-accent text-white" size="sm" onPress={() => void activateCapability()}>{t('studio.devtools.ai.capability_confirm')}</Button></If>
          </div>
        </If>
        <If cond={memoryCandidates.length > 0}>
          <div className="space-y-2">
            <strong className="text-[10px] uppercase tracking-wider text-muted">{t('studio.devtools.ai.memory_candidates')}</strong>
            {memoryCandidates.map(memory => <MemoryCandidate key={memory.id} memory={memory} onReview={status => void reviewMemory(memory, status)} />)}
          </div>
        </If>
        <CapabilityGovernance
          projectId={project.id}
          systemId={manifest.systemId}
          refreshToken={`${capabilityDraft?.id ?? ''}:${capabilityDraft?.status ?? ''}`}
        />
        <If cond={error != null}><p className="rounded-xl border border-danger/30 bg-danger/8 p-3 text-xs text-danger">{error}</p></If>
      </div>
      <div className="shrink-0 border-t border-line p-3">
        <details className="mb-2 rounded-lg border border-line bg-panel2 px-2 py-1.5">
          <summary className="cursor-pointer text-[9px] text-muted">{t('studio.devtools.ai.global_scope')}</summary>
          <div className="mt-2 max-h-36 space-y-1.5 overflow-auto">
            <label className="flex items-center gap-2 text-[10px] text-ink">
              <input type="checkbox" checked disabled />
              {t(`studio.devtools.tool.${manifest.systemId}.title`)}
            </label>
            {globalSelectableSystems(manifest.systemId).map(systemId => (
              <label key={systemId} className="flex items-center gap-2 text-[10px] text-ink">
                <input type="checkbox" checked={globalWriteSystems.includes(systemId)} onChange={() => toggleGlobalWriteSystem(systemId)} />
                {t(`studio.devtools.tool.${systemId}.title`)}
                <If cond={manifest.dependencies.includes(systemId)}><small className="text-[8px] text-muted">{t('studio.devtools.ai.global_dependency_readonly')}</small></If>
              </label>
            ))}
          </div>
        </details>
        <div className="mb-2 grid grid-cols-3 gap-1">
          <Button size="sm" variant="ghost" isPending={receiptStatus === 'saving'} onPress={() => void summarizeTask()}>{t(receiptButtonKey(receiptStatus))}</Button>
          <Button size="sm" variant="ghost" isDisabled={!(draftId ?? scopeDraftId)} onPress={() => void promoteCapability()}>
            <MagicWand className="size-3.5" />
            {t('studio.devtools.ai.promote')}
          </Button>
          <Button size="sm" variant="ghost" isDisabled={!sessionId} isPending={globalPending} onPress={() => void openGlobalTask()}>{t('studio.devtools.ai.global')}</Button>
        </div>
        <textarea
          rows={3}
          className="w-full resize-none rounded-xl border border-line bg-panel2 px-3 py-2 text-xs leading-5 text-ink outline-none focus:border-accent"
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
        <div className="mt-2 flex items-center justify-between">
          <small className="text-[9px] text-muted">{t('studio.devtools.ai.scoped_write')}</small>
          <If cond={running} else={<Button isIconOnly size="sm" className="bg-accent text-white" aria-label={t('studio.devtools.ai.send')} onPress={() => void sendPrompt()}><ArrowUp className="size-4" /></Button>}>
            <Button isIconOnly size="sm" variant="ghost" aria-label={t('studio.devtools.ai.cancel')} onPress={cancel}><CircleStop className="size-4" /></Button>
          </If>
        </div>
      </div>
    </aside>
  )
}

function AiWelcome({ manifest }: { manifest: DomainManifest }) {
  const { t } = useTranslation()
  return (
    <div className="rounded-xl border border-line bg-panel2 p-4">
      <Sparkles className="size-5 text-accent" />
      <strong className="mt-3 block text-sm text-ink">{t('studio.devtools.ai.welcome')}</strong>
      <p className="mt-1 text-xs leading-5 text-muted">{t('studio.devtools.ai.scope', { system: t(`studio.devtools.tool.${manifest.systemId}.title`) })}</p>
    </div>
  )
}

function AiBubble({ message }: { message: AiMessage }) {
  return <div className={aiBubbleClass(message.role)}>{message.content}</div>
}

function connectionStatusKey(connected: boolean) {
  if (connected)
    return 'studio.devtools.ai.connected'
  return 'studio.devtools.ai.connecting'
}

function draftStatusLabel(t: ReturnType<typeof useTranslation>['t'], draftId?: string | null) {
  if (draftId)
    return t('studio.devtools.ai.draft_active', { id: draftId })
  return t('studio.devtools.ai.no_draft')
}

function receiptButtonKey(status: 'idle' | 'saving' | 'saved') {
  if (status === 'saved')
    return 'studio.devtools.ai.receipt_saved'
  return 'studio.devtools.ai.summarize'
}

function taskReceiptStatus(running: boolean) {
  if (running)
    return 'in_progress'
  return 'summary'
}

function stableTextKey(value: string) {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return (hash >>> 0).toString(16).padStart(8, '0')
}

function isSuccessfulReceipt(status: string): boolean {
  return status === 'applied'
}

function aiBubbleClass(role: AiMessage['role']) {
  if (role === 'user')
    return 'ml-8 rounded-xl bg-accent px-3 py-2 text-xs leading-5 text-white'
  return 'mr-4 whitespace-pre-wrap rounded-xl border border-line bg-panel2 px-3 py-2 text-xs leading-5 text-ink'
}

function memoryStatusKey(status: DomainMemory['status']) {
  if (status === 'active')
    return 'studio.devtools.ai.memory_active'
  if (status === 'disabled')
    return 'studio.devtools.ai.memory_disabled'
  return 'studio.devtools.ai.memory_candidate'
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

function replaceMemory(memories: DomainMemory[], replacement: DomainMemory) {
  return [replacement, ...memories.filter(memory => memory.id !== replacement.id)]
}

async function taskDraftEvidence(projectId: string, systemId: string, draftId?: string | null) {
  if (!draftId)
    return { draftReviewed: false }
  const [preview, validation] = await Promise.all([
    previewDomainDraft(projectId, draftId),
    validateDomainDraft(projectId, draftId),
  ])
  return {
    draftReviewed: true,
    diffHash: preview.preview.diffHash,
    draftRevision: preview.preview.draft.revision,
    changedFiles: preview.preview.changes.map(change => change.path),
    validationValid: validation.valid && validation.systemId === systemId,
    validationDiagnostics: sanitizeTaskSemanticList(validation.diagnostics),
  }
}

function CapabilityContract({ capability }: { capability: DomainManifest['capabilities'][number] | undefined }) {
  const { t } = useTranslation()
  if (!capability)
    return null
  return (
    <div className="mt-2 space-y-1 rounded-lg border border-line bg-panel p-2 font-mono text-[9px] text-muted">
      <p>
        {t('studio.devtools.ai.capability_parameters')}
        :
        {' '}
        {JSON.stringify(capability.parameterSchema)}
      </p>
      <p>
        {t('studio.devtools.ai.capability_permissions')}
        : R[
        {capability.readSystems.join(', ')}
        ] W[
        {capability.writeSystems.join(', ')}
        ]
      </p>
      <p>
        {t('studio.devtools.ai.capability_steps')}
        :
        {' '}
        {capability.steps.map(step => step.operation).join(' → ')}
      </p>
    </div>
  )
}

function MemoryCandidate({ memory, onReview }: { memory: DomainMemory, onReview: (status: 'active' | 'revoked') => void }) {
  const { t } = useTranslation()
  return (
    <div className="rounded-xl border border-line bg-panel2 p-3">
      <div className="flex items-center justify-between gap-2">
        <strong className="text-xs text-ink">{memory.summary}</strong>
        <span className="rounded-full border border-line bg-panel px-2 py-1 text-[9px] text-muted">{t(memoryStatusKey(memory.status))}</span>
      </div>
      <p className="mt-1 font-mono text-[9px] text-muted">
        {memory.kind}
        {' '}
        ·
        {' '}
        {memory.pluginVersion}
      </p>
      <If cond={memory.status === 'candidate'}>
        <div className="mt-2 flex gap-2">
          <Button size="sm" className="bg-accent text-white" onPress={() => onReview('active')}>{t('studio.devtools.ai.memory_activate')}</Button>
          <Button size="sm" variant="ghost" className="text-danger" onPress={() => onReview('revoked')}>{t('studio.devtools.ai.memory_reject')}</Button>
        </div>
      </If>
    </div>
  )
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
      const renewed = await issueTaskScope(
        project.id,
        identity.taskId,
        previous.readSystems,
        previous.writeSystems,
        previous.draftIds,
        previous.pluginVersions,
      )
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
  const projected = projectMessages(snapshot.nodes ?? [])
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

function projectMessages(nodes: unknown[]): AiMessage[] {
  return projectTaskMessages(nodes)
}

function bridgeError(message: Mir3BridgeEnvelope) {
  const payload = message.payload as { code?: string, message?: string }
  return [payload.code, payload.message].filter(Boolean).join(': ')
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
    <div className="rounded-xl border border-warning/30 bg-warning/8 p-3">
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
