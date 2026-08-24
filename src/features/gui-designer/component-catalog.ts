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
}

export const GUI_COMPONENTS: readonly GuiComponentDefinition[] = [
  { kind: 'Node', category: 'basic', container: true, approximate: false, defaultWidth: 0, defaultHeight: 0 },
  { kind: 'Panel', category: 'basic', container: true, approximate: false, defaultWidth: 240, defaultHeight: 160 },
  { kind: 'Image', category: 'basic', container: false, approximate: false, defaultWidth: 100, defaultHeight: 40 },
  { kind: 'Button', category: 'basic', container: false, approximate: false, defaultWidth: 100, defaultHeight: 40 },
  { kind: 'Text', category: 'basic', container: false, approximate: false, defaultWidth: 80, defaultHeight: 24 },
  { kind: 'TextAtlas', category: 'text-input', container: false, approximate: false, defaultWidth: 120, defaultHeight: 24 },
  { kind: 'RichText', category: 'text-input', container: false, approximate: false, defaultWidth: 240, defaultHeight: 80 },
  { kind: 'ScrollText', category: 'text-input', container: false, approximate: false, defaultWidth: 240, defaultHeight: 28 },
  { kind: 'CheckBox', category: 'text-input', container: false, approximate: false, defaultWidth: 32, defaultHeight: 32 },
  { kind: 'TextInput', category: 'text-input', container: false, approximate: false, defaultWidth: 180, defaultHeight: 32 },
  { kind: 'MenuItem', category: 'text-input', container: false, approximate: false, defaultWidth: 180, defaultHeight: 32 },
  { kind: 'PageView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240 },
  { kind: 'ListView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240 },
  { kind: 'ScrollView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240 },
  { kind: 'QuickCell', category: 'container', container: true, approximate: true, defaultWidth: 320, defaultHeight: 72 },
  { kind: 'TableView', category: 'container', container: true, approximate: false, defaultWidth: 320, defaultHeight: 240 },
  { kind: 'Slider', category: 'progress', container: false, approximate: false, defaultWidth: 180, defaultHeight: 24 },
  { kind: 'ProgressTimer', category: 'progress', container: false, approximate: false, defaultWidth: 64, defaultHeight: 64 },
  { kind: 'LoadingBar', category: 'progress', container: false, approximate: false, defaultWidth: 180, defaultHeight: 24 },
  { kind: 'ItemShow', category: 'runtime', container: false, approximate: true, defaultWidth: 64, defaultHeight: 64 },
  { kind: 'Effect', category: 'runtime', container: false, approximate: true, defaultWidth: 96, defaultHeight: 96 },
  { kind: 'UIModel', category: 'runtime', container: false, approximate: true, defaultWidth: 160, defaultHeight: 220 },
  { kind: 'SpineAnim', category: 'runtime', container: false, approximate: true, defaultWidth: 160, defaultHeight: 160 },
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
