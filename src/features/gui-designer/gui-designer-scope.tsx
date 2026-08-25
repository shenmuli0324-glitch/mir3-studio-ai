import type { GuiComponentKind } from './component-catalog'
import type {
  BoundValue,
  GuiDevice,
  GuiDraftConfirmation,
  GuiLeftPanel,
  GuiMode,
  GuiPropertyValue,
  GuiTemplateRequest,
  Mir3UiDocument,
  Mir3UiNode,
  SourceSpan,
} from './types'
import { useEffect, useRef, useState } from 'react'
import { useMir3Projects } from '@/features/projects/use-mir3-projects'
import { defineScope } from '@/hooks/define-scope'
import { useGuiDesignerStatus, useGuiDocumentActions, useGuiDocumentList } from './api'
import { componentDefinition, isContainerKind } from './component-catalog'
import { MOBILE_VIEWPORT, PC_VIEWPORTS } from './types'

export interface GuiWorkingFile {
  path: string
  originalSource: string
  workingSource: string
  document: Mir3UiDocument
  expectedSha256?: string | null
  history: GuiSourceTransaction[]
  future: GuiSourceTransaction[]
  valid: boolean
  isNew: boolean
  parseError?: string | null
  accessedAt: number
}

export type GuiBehaviorKind = 'timeline' | 'action'

interface GuiSourceTransaction {
  start: number
  before: string
  after: string
}

const MAX_HISTORY_ENTRIES = 200
const MAX_HISTORY_BYTES = 32 * 1024 * 1024
const MAX_WORKING_SOURCE_BYTES = 8 * 1024 * 1024
const MAX_CLEAN_WORKING_FILES = 20

let dirtyOutsideScope = false

export function isGuiDesignerDirty(): boolean {
  return dirtyOutsideScope
}

export const GuiDesignerScope = defineScope(() => {
  const { activeProject } = useMir3Projects()
  const projectId = activeProject?.id
  const status = useGuiDesignerStatus(projectId)
  const documentList = useGuiDocumentList(projectId)
  const actions = useGuiDocumentActions(projectId)
  const [files, setFiles] = useState<Record<string, GuiWorkingFile>>({})
  const [currentPath, setCurrentPath] = useState<string | null>(null)
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null)
  const [deviceState, setDeviceState] = useState<GuiDevice>('mobile')
  const [pcViewportIndex, setPcViewportIndex] = useState(8)
  const [mode, setMode] = useState<GuiMode>('visual')
  const [leftPanel, setLeftPanel] = useState<GuiLeftPanel>('files')
  const [zoomState, setZoomState] = useState(0.72)
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(true)
  const [newDialogOpen, setNewDialogOpen] = useState(false)
  const [draftConfirmation, setDraftConfirmation] = useState<GuiDraftConfirmation | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [parsePending, setParsePending] = useState(false)
  const parseSequenceRef = useRef(0)
  const openSequenceRef = useRef(0)
  const currentFile = currentPath ? files[currentPath] : undefined
  const entries = documentList.data ?? []
  const dirty = Object.values(files).some(file => file.workingSource !== file.originalSource || file.isNew)
  const viewport = deviceState === 'mobile' ? MOBILE_VIEWPORT : PC_VIEWPORTS[pcViewportIndex]
  const previewDocument = currentFile?.document

  useEffect(() => {
    dirtyOutsideScope = dirty
    function onBeforeUnload(event: BeforeUnloadEvent) {
      if (!dirty)
        return
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', onBeforeUnload)
    return () => {
      dirtyOutsideScope = false
      window.removeEventListener('beforeunload', onBeforeUnload)
    }
  }, [dirty])

  useEffect(() => {
    if (!currentFile || (currentFile.workingSource === currentFile.originalSource && currentFile.history.length === 0))
      return
    if (new TextEncoder().encode(currentFile.workingSource).byteLength > MAX_WORKING_SOURCE_BYTES) {
      const oversizedTimer = window.setTimeout(() => {
        setParsePending(false)
        setFiles(value => ({
          ...value,
          [currentFile.path]: { ...value[currentFile.path], valid: false, parseError: 'GUI_SOURCE_TOO_LARGE' },
        }))
      }, 0)
      return () => window.clearTimeout(oversizedTimer)
    }
    const sequence = ++parseSequenceRef.current
    const timer = window.setTimeout(() => {
      void actions.reparse({
        devRelativePath: currentFile.path,
        workingSource: currentFile.workingSource,
        expectedSha256: currentFile.expectedSha256,
      }).then((result) => {
        if (sequence !== parseSequenceRef.current)
          return
        const valid = !result.document.diagnostics.some(item => item.severity === 'error')
        setParsePending(false)
        setFiles(value => ({
          ...value,
          [currentFile.path]: {
            ...value[currentFile.path],
            document: valid ? result.document : value[currentFile.path].document,
            valid,
            parseError: null,
          },
        }))
      }).catch((error) => {
        if (sequence !== parseSequenceRef.current)
          return
        setParsePending(false)
        setFiles(value => ({
          ...value,
          [currentFile.path]: { ...value[currentFile.path], valid: false, parseError: errorMessage(error) },
        }))
      })
    }, 250)
    return () => window.clearTimeout(timer)
  // actions中的mutation随render更新；这里只由工作副本版本驱动防抖解析。
  // eslint-disable-next-line react/exhaustive-deps
  }, [currentFile?.path, currentFile?.workingSource])

  async function openFile(path: string) {
    const sequence = ++openSequenceRef.current
    const cached = files[path]
    if (cached) {
      setFiles(value => ({ ...value, [path]: { ...value[path], accessedAt: Date.now() } }))
      setCurrentPath(path)
      setSelectedNodeId(cached.document.roots[0] ?? null)
      return
    }
    const result = await actions.open({ devRelativePath: path })
    setFiles((value) => {
      const next = {
        ...value,
        [path]: {
          path,
          originalSource: result.source,
          workingSource: result.source,
          document: result.document,
          expectedSha256: result.sha256 ?? result.document.sourceSha256,
          history: [],
          future: [],
          valid: !result.document.diagnostics.some(item => item.severity === 'error'),
          isNew: false,
          accessedAt: Date.now(),
        },
      }
      return pruneWorkingFileCache(next, path)
    })
    if (sequence !== openSequenceRef.current)
      return
    setCurrentPath(path)
    setSelectedNodeId(result.document.roots[0] ?? null)
    setParsePending(false)
  }

  function updateWorkingSource(source: string) {
    if (!currentPath || !currentFile || currentFile.workingSource === source)
      return
    setParsePending(true)
    setFiles((value) => {
      const file = value[currentPath]
      if (!file || file.workingSource === source)
        return value
      const transaction = sourceTransaction(file.workingSource, source)
      return {
        ...value,
        [currentPath]: {
          ...file,
          workingSource: source,
          history: trimHistory([...file.history, transaction]),
          future: [],
          accessedAt: Date.now(),
        },
      }
    })
  }

  function undo() {
    if (!currentPath || !currentFile || currentFile.history.length === 0)
      return
    setParsePending(true)
    setFiles((value) => {
      const file = value[currentPath]
      const transaction = file?.history.at(-1)
      if (!file || transaction == null)
        return value
      const previous = applySourceTransaction(file.workingSource, transaction, false)
      return {
        ...value,
        [currentPath]: {
          ...file,
          workingSource: previous,
          history: file.history.slice(0, -1),
          future: [transaction, ...file.future],
        },
      }
    })
  }

  function redo() {
    if (!currentPath || !currentFile || currentFile.future.length === 0)
      return
    setParsePending(true)
    setFiles((value) => {
      const file = value[currentPath]
      const transaction = file?.future[0]
      if (!file || transaction == null)
        return value
      const next = applySourceTransaction(file.workingSource, transaction, true)
      return {
        ...value,
        [currentPath]: {
          ...file,
          workingSource: next,
          history: trimHistory([...file.history, transaction]),
          future: file.future.slice(1),
        },
      }
    })
  }

  function updateNodeProperty(nodeId: string, property: 'x' | 'y' | 'width' | 'height' | 'text' | 'image', value: number | string) {
    const file = currentFile
    const node = editableSourceNode(file, previewDocument?.nodes[nodeId])
    if (!file || !node)
      return
    const bound = boundForProperty(node, property)
    if (bound?.writable && bound.span) {
      const token = typeof value === 'number' ? formatNumber(value) : luaString(value)
      updateWorkingSource(replaceSpans(file.workingSource, [{ span: bound.span, token }]))
      return
    }
    if ((property === 'width' || property === 'height') && typeof value === 'number' && canInsertSizeSetter(node)) {
      const width = property === 'width' ? value : defaultNodeSize(node).width
      const height = property === 'height' ? value : defaultNodeSize(node).height
      const insertion = node.binding!.safeInsertion!
      const newline = file.document.newline === '\r\n' ? '\r\n' : file.document.newline === '\r' ? '\r' : '\n'
      const setter = `${newline}\tGUI:setContentSize(${node.luaVariable}, ${formatNumber(width)}, ${formatNumber(height)})`
      updateWorkingSource(replaceSpans(file.workingSource, [{ span: insertion, token: setter }]))
    }
  }

  function nodePropertyWritable(node: Mir3UiNode, property: 'x' | 'y' | 'width' | 'height' | 'text' | 'image'): boolean {
    const sourceNode = editableSourceNode(currentFile, node)
    if (!sourceNode)
      return false
    const bound = boundForProperty(sourceNode, property)
    if (bound?.writable && bound.span)
      return true
    return (property === 'width' || property === 'height') && canInsertSizeSetter(sourceNode)
  }

  function updateNodeGenericProperty(nodeId: string, property: string, value: GuiPropertyValue) {
    const file = currentFile
    const node = editableSourceNode(file, previewDocument?.nodes[nodeId])
    const bound = node?.properties?.[property]
    if (!file || !bound?.writable || !bound.span)
      return
    updateWorkingSource(replaceSpans(file.workingSource, [{ span: bound.span, token: luaPropertyToken(value) }]))
  }

  function nodeGenericPropertyWritable(node: Mir3UiNode, property: string): boolean {
    const sourceNode = editableSourceNode(currentFile, node)
    if (!sourceNode)
      return false
    const bound = sourceNode.properties?.[property]
    return Boolean(bound?.writable && bound.span)
  }

  function updateNodePosition(nodeId: string, x: number, y: number) {
    const file = currentFile
    const node = editableSourceNode(file, previewDocument?.nodes[nodeId])
    if (!file || !node || !node.position.x.writable || !node.position.y.writable || !node.position.x.span || !node.position.y.span)
      return
    updateWorkingSource(replaceSpans(file.workingSource, [
      { span: node.position.x.span, token: formatNumber(x) },
      { span: node.position.y.span, token: formatNumber(y) },
    ]))
  }

  function addNode(kind: GuiComponentKind, x = 80, y = 80, canvasCoordinates = false) {
    const file = currentFile
    if (!file)
      return
    const selected = selectedNode(file, selectedNodeId)
    const parent = insertionParent(file, selected)
    const insertion = parent?.binding?.safeInsertion
    if (!parent || !insertion) {
      setNotice('studio.gui.notice.no_insertion')
      return
    }
    const index = nextNodeIndex(file.document, kind)
    const variable = `${kind}_${index}`
    const parentVariable = parent.luaVariable ?? 'parent'
    const parentPosition = canvasCoordinates ? absoluteNodePosition(file.document, parent) : { x: 0, y: 0 }
    const source = componentSource(kind, variable, parentVariable, x - parentPosition.x, y - parentPosition.y)
    updateWorkingSource(replaceSpans(file.workingSource, [{ span: insertion, token: source }]))
    setNotice(null)
  }

  function addNodeBehavior(nodeId: string, behavior: GuiBehaviorKind) {
    const file = currentFile
    const node = file?.document.nodes[nodeId]
    const insertion = node?.binding?.safeInsertion
    if (!file || !node?.luaVariable || !insertion)
      return
    const newline = file.document.newline === '\r\n' ? '\r\n' : file.document.newline === '\r' ? '\r' : '\n'
    const source = behaviorSource(behavior, node.luaVariable, newline)
    updateWorkingSource(replaceSpans(file.workingSource, [{ span: insertion, token: source }]))
  }

  function canAddNodeBehavior(node: Mir3UiNode): boolean {
    return Boolean(node.luaVariable && node.binding?.safeInsertion && !parsePending)
  }

  async function createPage(request: GuiTemplateRequest) {
    const result = await actions.createTemplate({ ...request, pcResolution: PC_VIEWPORTS[pcViewportIndex] })
    const documents = normalizeTemplateResult(result, request)
    setFiles((value) => {
      const next = { ...value }
      for (const item of documents) {
        next[item.path] = {
          path: item.path,
          originalSource: '',
          workingSource: item.source,
          document: item.document,
          history: [],
          future: [],
          valid: true,
          isNew: true,
          accessedAt: Date.now(),
        }
      }
      return next
    })
    const first = documents[0]
    if (first) {
      setCurrentPath(first.path)
      setSelectedNodeId(first.document.roots[0] ?? null)
    }
    setNewDialogOpen(false)
  }

  async function prepareDiff() {
    const changed = Object.values(files).filter(file => file.workingSource !== file.originalSource || file.isNew)
    if (changed.length === 0 || changed.some(file => !file.valid))
      return
    const prepared = await actions.prepareDraft({
      files: changed.map(file => ({
        devRelativePath: file.path,
        source: file.workingSource,
        expectedSha256: file.expectedSha256,
        isNew: file.isNew,
      })),
      expectedRevision: 0,
    })
    const confirmation = await actions.confirmDraft(prepared.draftId)
    setDraftConfirmation(confirmation)
  }

  async function applyDiff() {
    if (!draftConfirmation)
      return
    await actions.applyDraft({ draftId: draftConfirmation.draftId, token: draftConfirmation.confirmationToken })
    setFiles({})
    setCurrentPath(null)
    setSelectedNodeId(null)
    setParsePending(false)
    setDraftConfirmation(null)
  }

  function setDevice(nextDevice: GuiDevice) {
    setDeviceState(nextDevice)
    if (!currentPath)
      return
    const entry = entries.find(item => item.path === currentPath)
    if (entry?.platform === nextDevice) {
      setNotice(null)
      return
    }
    const directPeer = entry?.peerPath
    const inferredPeer = inferPeerPath(currentPath, nextDevice)
    const peer = directPeer && (files[directPeer] || entries.some(item => item.path === directPeer && item.platform === nextDevice))
      ? directPeer
      : files[inferredPeer]
        ? inferredPeer
        : entries.find(item => item.path === inferredPeer && item.platform === nextDevice)?.path
    if (peer) {
      void openFile(peer).catch(() => {})
      setNotice(null)
    }
    else {
      setNotice('studio.gui.notice.no_peer')
    }
  }

  function setZoom(value: number) {
    setZoomState(Math.min(1.6, Math.max(0.2, value)))
  }

  function fitCanvas() {
    const container = document.querySelector<HTMLElement>('[data-gui-canvas-container]')
    if (!container)
      return
    const availableWidth = Math.max(1, container.clientWidth - 112)
    const availableHeight = Math.max(1, container.clientHeight - 112)
    setZoom(Math.min(availableWidth / viewport.width, availableHeight / viewport.height))
  }

  return {
    activeProject,
    status: status.data,
    statusLoading: status.isLoading,
    statusError: status.error,
    entries,
    entriesLoading: documentList.isLoading,
    files,
    currentFile,
    previewDocument,
    currentPath,
    selectedNodeId,
    selectedNode: previewDocument && selectedNodeId ? previewDocument.nodes[selectedNodeId] : undefined,
    device: deviceState,
    viewport,
    pcViewportIndex,
    mode,
    leftPanel,
    zoom: zoomState,
    dirty,
    diagnosticsOpen,
    newDialogOpen,
    draftConfirmation,
    notice,
    parsePending,
    busy: actions.busy,
    error: actions.error,
    openFile,
    updateWorkingSource,
    undo,
    redo,
    updateNodeProperty,
    nodePropertyWritable,
    updateNodeGenericProperty,
    nodeGenericPropertyWritable,
    updateNodePosition,
    addNode,
    addNodeBehavior,
    canAddNodeBehavior,
    createPage,
    prepareDiff,
    applyDiff,
    setCurrentPath,
    setSelectedNodeId,
    setDevice,
    setPcViewportIndex,
    setMode,
    setLeftPanel,
    setZoom,
    fitCanvas,
    setDiagnosticsOpen,
    setNewDialogOpen,
    setDraftConfirmation,
    setNotice,
  }
})

function selectedNode(file: GuiWorkingFile, nodeId: string | null): Mir3UiNode | undefined {
  return nodeId ? file.document.nodes[nodeId] : undefined
}

function editableSourceNode(file: GuiWorkingFile | undefined, previewNode: Mir3UiNode | undefined): Mir3UiNode | undefined {
  if (!file || !previewNode)
    return undefined
  return file.document.nodes[previewNode.id]
}

function insertionParent(file: GuiWorkingFile, selected?: Mir3UiNode): Mir3UiNode | undefined {
  if (selected && isContainerKind(selected.kind))
    return selected
  if (selected?.parentId) {
    const parent = selectedNode(file, selected.parentId)
    if (parent && isContainerKind(parent.kind))
      return parent
  }
  return selectedNode(file, file.document.roots[0] ?? null)
}

function boundForProperty(node: Mir3UiNode, property: 'x' | 'y' | 'width' | 'height' | 'text' | 'image'): BoundValue<number | string> | undefined | null {
  switch (property) {
    case 'x': return node.position.x
    case 'y': return node.position.y
    case 'width': return node.size.width
    case 'height': return node.size.height
    case 'text': return node.paint?.text
    case 'image': return node.paint?.image ?? node.paint?.normalImage
  }
}

function canInsertSizeSetter(node: Mir3UiNode): boolean {
  return node.luaVariable != null && node.luaVariable.length > 0 && node.binding?.safeInsertion != null
}

function defaultNodeSize(node: Mir3UiNode): { width: number, height: number } {
  if (node.kind === 'Unsupported')
    return { width: Math.max(24, node.size.width.value), height: Math.max(24, node.size.height.value) }
  const definition = componentDefinition(node.kind)
  const width = node.size.width.value > 0 ? node.size.width.value : definition.defaultWidth
  const height = node.size.height.value > 0 ? node.size.height.value : definition.defaultHeight
  return { width, height }
}

function replaceSpans(source: string, replacements: Array<{ span: SourceSpan, token: string }>): string {
  const ordered = [...replacements].sort((left, right) => right.span.startByte - left.span.startByte)
  let next = source
  for (const replacement of ordered) {
    const start = stringIndexAtUtf8Byte(next, replacement.span.startByte)
    const end = stringIndexAtUtf8Byte(next, replacement.span.endByte)
    next = `${next.slice(0, start)}${replacement.token}${next.slice(end)}`
  }
  return next
}

function stringIndexAtUtf8Byte(source: string, byteOffset: number): number {
  if (byteOffset <= 0)
    return 0
  let bytes = 0
  let index = 0
  for (const character of source) {
    if (bytes >= byteOffset)
      break
    bytes += new TextEncoder().encode(character).length
    index += character.length
  }
  return index
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : String(Math.round(value * 100) / 100)
}

function luaString(value: string): string {
  return JSON.stringify(value)
}

function luaPropertyToken(value: GuiPropertyValue): string {
  if (value == null)
    return 'nil'
  if (typeof value === 'string')
    return luaString(value)
  if (typeof value === 'number')
    return formatNumber(value)
  if (typeof value === 'boolean')
    return value ? 'true' : 'false'
  return value.luaLiteral
}

function behaviorSource(behavior: GuiBehaviorKind, variable: string, newline: string): string {
  if (behavior === 'timeline')
    return `${newline}\tGUI:Timeline_FadeIn(${variable}, 0.3, nil)`
  return `${newline}\tGUI:runAction(${variable}, GUI:ActionFadeIn(0.3))`
}

function nextNodeIndex(document: Mir3UiDocument, kind: string): number {
  const expressions = Object.values(document.nodes).map(node => node.luaVariable ?? '')
  let index = 1
  while (expressions.includes(`${kind}_${index}`))
    index += 1
  return index
}

function absoluteNodePosition(document: Mir3UiDocument, node: Mir3UiNode): { x: number, y: number } {
  const parent = node.parentId ? document.nodes[node.parentId] : undefined
  if (!parent)
    return { x: node.position.x.value, y: node.position.y.value }
  const parentPosition = absoluteNodePosition(document, parent)
  return { x: parentPosition.x + node.position.x.value, y: parentPosition.y + node.position.y.value }
}

function componentSource(kind: GuiComponentKind, variable: string, parent: string, x: number, y: number): string {
  const prefix = `\n\tlocal ${variable} = GUI:`
  const definition = componentDefinition(kind)
  const width = definition.defaultWidth
  const height = definition.defaultHeight
  switch (kind) {
    case 'Node': return `${prefix}Node_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)})\n`
    case 'Panel': return `${prefix}Layout_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, false)\n`
    case 'Image': return `${prefix}Image_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "")\n`
    case 'Text': return `${prefix}Text_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, 16, "#ffffff", [[]])\n`
    case 'Button': return `${prefix}Button_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "")\n`
    case 'TextAtlas': return `${prefix}TextAtlas_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "0", "", 16, 24, "0")\n`
    case 'RichText': return `${prefix}RichText_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, [[]], ${width}, 16, "#ffffff", 2, nil, "Arial")\n`
    case 'ScrollText': return `${prefix}ScrollText_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, 16, "#ffffff", [[]])\n`
    case 'ItemShow': return `${prefix}ItemShow_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, {})\n`
    case 'CheckBox': return `${prefix}CheckBox_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "", "")\n`
    case 'TextInput': return `${prefix}TextInput_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, 16)\n`
    case 'Slider': return `${prefix}Slider_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "", "", "")\n\tGUI:setContentSize(${variable}, ${width}, ${height})\n`
    case 'ProgressTimer': return `${prefix}ProgressTimer_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "")\n\tGUI:setContentSize(${variable}, ${width}, ${height})\n`
    case 'LoadingBar': return `${prefix}LoadingBar_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "", 0)\n\tGUI:setContentSize(${variable}, ${width}, ${height})\n`
    case 'Effect': return `${prefix}Effect_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, 0, 0, 0, 0, 0, 1)\n`
    case 'UIModel': return `${prefix}UIModel_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, 0, 0, 1)\n`
    case 'SpineAnim': return `${prefix}SpineAnim_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, "", "", 0, "", true)\n`
    case 'PageView': return `${prefix}PageView_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, 1)\n`
    case 'ListView': return `${prefix}ListView_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, 1)\n`
    case 'ScrollView': return `${prefix}ScrollView_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, 1)\n`
    case 'QuickCell': return `${prefix}QuickCell_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, function(quickParent)\n\t\tlocal ${variable}_root = GUI:Layout_Create(quickParent, "${variable}_root", 0, 0, ${width}, ${height}, false)\n\t\treturn ${variable}_root\n\tend)\n`
    case 'MenuItem': return `${prefix}MenuItem_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, { itemname = "", select = "", direction = 0, width = ${width}, height = ${height}, fontSize = 16, fontColor = "#ffffff" })\n`
    case 'TableView': return `${prefix}TableView_Create(${parent}, "${variable}", ${formatNumber(x)}, ${formatNumber(y)}, ${width}, ${height}, 1, 80, 40, 0)\n`
  }
}

function sourceTransaction(beforeSource: string, afterSource: string): GuiSourceTransaction {
  let start = 0
  const commonLength = Math.min(beforeSource.length, afterSource.length)
  while (start < commonLength && beforeSource[start] === afterSource[start])
    start += 1
  let beforeEnd = beforeSource.length
  let afterEnd = afterSource.length
  while (beforeEnd > start && afterEnd > start && beforeSource[beforeEnd - 1] === afterSource[afterEnd - 1]) {
    beforeEnd -= 1
    afterEnd -= 1
  }
  return { start, before: beforeSource.slice(start, beforeEnd), after: afterSource.slice(start, afterEnd) }
}

function applySourceTransaction(source: string, transaction: GuiSourceTransaction, forward: boolean): string {
  const find = forward ? transaction.before : transaction.after
  const replacement = forward ? transaction.after : transaction.before
  return `${source.slice(0, transaction.start)}${replacement}${source.slice(transaction.start + find.length)}`
}

function trimHistory(history: GuiSourceTransaction[]): GuiSourceTransaction[] {
  let bytes = 0
  const kept: GuiSourceTransaction[] = []
  for (let index = history.length - 1; index >= 0 && kept.length < MAX_HISTORY_ENTRIES; index -= 1) {
    const transaction = history[index]
    const size = (transaction.before.length + transaction.after.length) * 2
    if (kept.length > 0 && bytes + size > MAX_HISTORY_BYTES)
      break
    bytes += size
    kept.push(transaction)
  }
  return kept.reverse()
}

function pruneWorkingFileCache(files: Record<string, GuiWorkingFile>, activePath: string): Record<string, GuiWorkingFile> {
  const cleanFiles = Object.values(files)
    .filter(file => file.path !== activePath && !file.isNew && file.workingSource === file.originalSource)
    .sort((left, right) => right.accessedAt - left.accessedAt)
  const keep = new Set(cleanFiles.slice(0, MAX_CLEAN_WORKING_FILES).map(file => file.path))
  return Object.fromEntries(Object.entries(files).filter(([path, file]) => {
    if (path === activePath || file.isNew || file.workingSource !== file.originalSource)
      return true
    return keep.has(path)
  }))
}

function inferPeerPath(path: string, device: GuiDevice): string {
  if (device === 'pc')
    return path.endsWith('_win32.lua') ? path : path.replace(/\.lua$/i, '_win32.lua')
  return path.replace(/_win32\.lua$/i, '.lua')
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function normalizeTemplateResult(result: unknown, request: GuiTemplateRequest): Array<{ path: string, source: string, document: Mir3UiDocument }> {
  const value = result as { documents?: Array<{ path: string, source: string, document: Mir3UiDocument }>, source?: string, document?: Mir3UiDocument }
  if (Array.isArray(value.documents))
    return value.documents
  if (value.source && value.document)
    return [{ path: value.document.devRelativePath || request.relativePath, source: value.source, document: value.document }]
  return []
}
