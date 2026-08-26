import type { ComponentType } from 'react'
import {
  Box,
  BroomMotion,
  Calendar,
  ChartBar,
  ChartLineArrowUp,
  CirclesConcentric,
  CircleTree,
  CrownDiamond,
  Cubes3,
  Diamond,
  Factory,
  Flag,
  Flask,
  Geo,
  Ghost,
  Gift,
  Globe,
  Hammer,
  Hourglass,
  House,
  MagicWand,
  Medal,
  Person,
  Persons,
  Rocket,
  Shield,
  ShoppingCart,
  Skull,
  Sliders,
  SquareCheck,
  Stopwatch,
  Timeline,
  Wallet,
} from '@gravity-ui/icons'

export const DEV_TOOL_CATEGORIES = [
  'resources',
  'growth',
  'equipment',
  'activities',
  'commercial',
  'social',
  'featured',
  'extension',
] as const

export type DevToolCategory = typeof DEV_TOOL_CATEGORIES[number]
export type DevToolStatus = 'ready'
export type DevToolIcon = ComponentType<{ className?: string }>

export interface DevToolDefinition {
  id: string
  order: number
  category: DevToolCategory
  icon: DevToolIcon
  status: DevToolStatus
}

export const DEV_TOOLS = [
  tool('map', 1, 'resources', Geo),
  tool('npc', 2, 'resources', Person),
  tool('monster', 3, 'resources', Skull),
  tool('equipment', 4, 'resources', Shield),
  tool('item', 5, 'resources', Box),
  tool('level', 6, 'growth', ChartLineArrowUp),
  tool('rebirth', 7, 'growth', CirclesConcentric),
  tool('title', 8, 'growth', Medal),
  tool('buff', 9, 'growth', Flask),
  tool('skill', 10, 'growth', MagicWand),
  tool('enhance', 11, 'equipment', Hammer),
  tool('crafting', 12, 'equipment', Cubes3),
  tool('gem', 13, 'equipment', Diamond),
  tool('refine', 14, 'equipment', Sliders),
  tool('quest', 15, 'activities', SquareCheck),
  tool('checkin', 16, 'activities', Calendar),
  tool('online_reward', 17, 'activities', Stopwatch),
  tool('limited_event', 18, 'activities', Hourglass),
  tool('launch_event', 19, 'activities', Rocket),
  tool('first_charge', 20, 'commercial', Gift),
  tool('cumulative_charge', 21, 'commercial', Wallet),
  tool('vip', 22, 'commercial', CrownDiamond),
  tool('shop', 23, 'commercial', ShoppingCart),
  tool('recycle', 24, 'commercial', BroomMotion),
  tool('guild', 25, 'social', Persons),
  tool('sabac', 26, 'social', Flag),
  tool('ranking', 27, 'social', ChartBar),
  tool('resource_production', 28, 'featured', Factory),
  tool('manor', 29, 'featured', House),
  tool('hero_soul', 30, 'featured', Ghost),
  tool('talent', 31, 'featured', CircleTree),
  tool('season', 32, 'featured', Timeline),
  tool('cross_server', 33, 'extension', Globe),
] as const satisfies readonly DevToolDefinition[]

export type DevToolId = typeof DEV_TOOLS[number]['id']

export function getDevTool(id: DevToolId): DevToolDefinition {
  const entry = DEV_TOOLS.find(item => item.id === id)
  if (!entry)
    throw new Error(`DEV_TOOL_NOT_FOUND: ${id}`)
  return entry
}

export function devToolTitleKey(id: string): string {
  return `studio.devtools.tool.${id}.title`
}

export function devToolDescriptionKey(id: string): string {
  return `studio.devtools.tool.${id}.description`
}

export function devToolCategoryKey(category: DevToolCategory): string {
  return `studio.devtools.category.${category}`
}

function tool<const TId extends string>(
  id: TId,
  order: number,
  category: DevToolCategory,
  icon: DevToolIcon,
  status: DevToolStatus = 'ready',
): DevToolDefinition & { id: TId } {
  return { id, order, category, icon, status }
}
