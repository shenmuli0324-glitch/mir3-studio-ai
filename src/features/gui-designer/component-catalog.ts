import type { GuiNodeKind } from './types'

export type GuiComponentKind = Exclude<GuiNodeKind, 'Unsupported'>
export type GuiComponentCategory = 'basic' | 'text-input' | 'container' | 'progress' | 'runtime'

export interface GuiComponentDefinition {
  kind: GuiComponentKind
  category: GuiComponentCategory
  container: boolean
  approximate: boolean
  defaultWidth: number
  defaultHeight: number
  assetSlots: readonly GuiAssetSlotDefinition[]
}

export interface GuiAssetSlotDefinition {
  slot: string
  property: string
  render: boolean
}

const NO_ASSETS: readonly GuiAssetSlotDefinition[] = []
const IMAGE_ASSET = [{ slot: 'normal', property: 'image', render: true }] as const

export const GUI_COMPONENTS: readonly GuiComponentDefinition[] = [
  { kind: 'Node', category: 'basic', container: true, approximate: false, defaultWidth: 0, defaultHeight: 0, assetSlots: NO_ASSETS },
  { kind: 'Panel', category: 'basic', container: true, approximate: false, defaultWidth: 240, defaultHeight: 160, assetSlots: [{ slot: 'background', property: 'backgroundImage', render: true }] },
  { kind: 'Image', category: 'basic', container: false, approximate: false, defaultWidth: 100, defaultHeight: 40, assetSlots: IMAGE_ASSET },
  { kind: 'Button', category: 'basic', container: false, approximate: false, defaultWidth: 100, defaultHeight: 40, assetSlots: [{ slot: 'normal', property: 'image', render: true }, { slot: 'pressed', property: 'pressedImage', render: false }, { slot: 'disabled', property: 'disabledImage', render: false }] },
  { kind: 'Text', category: 'basic', container: false, approximate: false, defaultWidth: 80, defaultHeight: 24, assetSlots: NO_ASSETS },
  { kind: 'TextAtlas', category: 'text-input', container: false, approximate: false, defaultWidth: 120, defaultHeight: 24, assetSlots: [{ slot: 'atlas', property: 'atlasImage', render: true }] },
  { kind: 'RichText', category: 'text-input', container: false, approximate: false, defaultWidth: 240, defaultHeight: 80, assetSlots: NO_ASSETS },
  { kind: 'ScrollText', category: 'text-input', container: false, approximate: false, defaultWidth: 240, defaultHeight: 28, assetSlots: NO_ASSETS },
  { kind: 'CheckBox', category: 'text-input', container: false, approximate: false, defaultWidth: 32, defaultHeight: 32, assetSlots: [{ slot: 'normal', property: 'image', render: true }, { slot: 'selected', property: 'selectedImage', render: false }] },
  { kind: 'TextInput', category: 'text-input', container: false, approximate: false, defaultWidth: 180, defaultHeight: 32, assetSlots: NO_ASSETS },
  { kind: 'MenuItem', category: 'text-input', container: false, approximate: false, defaultWidth: 180, defaultHeight: 32, assetSlots: NO_ASSETS },
  { kind: 'PageView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240, assetSlots: NO_ASSETS },
  { kind: 'ListView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240, assetSlots: [{ slot: 'background', property: 'backgroundImage', render: true }] },
  { kind: 'ScrollView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240, assetSlots: [{ slot: 'background', property: 'backgroundImage', render: true }] },
  { kind: 'QuickCell', category: 'container', container: true, approximate: true, defaultWidth: 320, defaultHeight: 72, assetSlots: NO_ASSETS },
  { kind: 'TableView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240, assetSlots: NO_ASSETS },
  { kind: 'Slider', category: 'progress', container: false, approximate: false, defaultWidth: 180, defaultHeight: 24, assetSlots: [{ slot: 'background', property: 'image', render: true }, { slot: 'progress', property: 'progressImage', render: false }, { slot: 'thumb', property: 'thumbImage', render: false }] },
  { kind: 'ProgressTimer', category: 'progress', container: false, approximate: false, defaultWidth: 64, defaultHeight: 64, assetSlots: IMAGE_ASSET },
  { kind: 'LoadingBar', category: 'progress', container: false, approximate: false, defaultWidth: 180, defaultHeight: 24, assetSlots: [{ slot: 'progress', property: 'progressImage', render: true }] },
  { kind: 'ItemShow', category: 'runtime', container: false, approximate: true, defaultWidth: 64, defaultHeight: 64, assetSlots: NO_ASSETS },
  { kind: 'Effect', category: 'runtime', container: false, approximate: true, defaultWidth: 96, defaultHeight: 96, assetSlots: NO_ASSETS },
  { kind: 'UIModel', category: 'runtime', container: false, approximate: true, defaultWidth: 160, defaultHeight: 220, assetSlots: NO_ASSETS },
  { kind: 'SpineAnim', category: 'runtime', container: false, approximate: true, defaultWidth: 160, defaultHeight: 160, assetSlots: [{ slot: 'json', property: 'jsonPath', render: false }, { slot: 'atlas', property: 'atlasPath', render: false }] },
] as const

export function componentDefinition(kind: GuiComponentKind): GuiComponentDefinition {
  return GUI_COMPONENTS.find(item => item.kind === kind) ?? GUI_COMPONENTS[0]
}

export function isContainerKind(kind: GuiNodeKind): boolean {
  if (kind === 'Unsupported')
    return false
  return componentDefinition(kind).container
}

export function isGuiComponentKind(kind: string): kind is GuiComponentKind {
  return GUI_COMPONENTS.some(item => item.kind === kind)
}

export function nodeAssetValue(node: import('./types').Mir3UiNode, property: string): import('./types').BoundValue<string> | undefined {
  if (node.kind !== 'Unsupported') {
    const definition = componentDefinition(node.kind).assetSlots.find(slot => slot.property === property)
    const slotValue = definition ? node.assetSlots?.[definition.slot] : undefined
    if (slotValue)
      return slotValue
  }
  if (property === 'image')
    return node.paint?.image ?? node.paint?.normalImage ?? stringBoundProperty(node, property)
  if (property === 'pressedImage')
    return node.paint?.pressedImage ?? stringBoundProperty(node, property)
  if (property === 'disabledImage')
    return node.paint?.disabledImage ?? stringBoundProperty(node, property)
  return stringBoundProperty(node, property)
}

export function renderAssetValue(node: import('./types').Mir3UiNode): import('./types').BoundValue<string> | undefined {
  if (node.kind === 'Unsupported')
    return undefined
  const slot = componentDefinition(node.kind).assetSlots.find(item => item.render)
  return slot ? nodeAssetValue(node, slot.property) : undefined
}

function stringBoundProperty(node: import('./types').Mir3UiNode, property: string): import('./types').BoundValue<string> | undefined {
  const bound = node.properties?.[property]
  if (bound && typeof bound.value === 'string')
    return bound as import('./types').BoundValue<string>
  return undefined
}
