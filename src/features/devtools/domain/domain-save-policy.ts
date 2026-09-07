import type { DomainDraftPreview } from './types'

const HIGH_RISK_SYSTEMS = new Set([
  'map',
  'quest',
  'limited_event',
  'launch_event',
  'first_charge',
  'cumulative_charge',
  'vip',
  'shop',
  'recycle',
  'sabac',
  'ranking',
  'season',
  'cross_server',
])

export function requiresSaveConfirmation(systemId: string, preview: DomainDraftPreview): boolean {
  if (preview.changes.some(change => change.deleted || change.unifiedDiff == null))
    return true
  return HIGH_RISK_SYSTEMS.has(systemId)
}

export function isSaveConfirmationRequired(reason: unknown): boolean {
  return String(reason).includes('DOMAIN_SAVE_CONFIRMATION_REQUIRED')
}
