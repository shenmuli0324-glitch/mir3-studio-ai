import type { DomainResourceProjection, DomainTextProjection, DomainXlsSheetProjection } from './types'

export interface ProjectionTable {
  columns: string[]
  rows: string[][]
  source: 'json' | 'delimited' | 'key-value' | 'lines' | 'xls' | 'record'
  totalRows: number
  totalColumns: number
}

const MAX_STRUCTURED_ROWS = 100
const MAX_STRUCTURED_COLUMNS = 32

export function projectionTable(projection: DomainResourceProjection, sheetIndex = 0): ProjectionTable | null {
  if (projection.kind === 'xls') {
    const sheet = projection.sheets[sheetIndex]
    if (!sheet)
      return null
    return xlsTable(sheet)
  }
  if (projection.kind === 'text')
    return textProjectionTable(projection)
  if (projection.kind === 'record') {
    const columns = Object.keys(projection.fields).slice(0, MAX_STRUCTURED_COLUMNS)
    return {
      columns,
      rows: [columns.map(column => displayValue(projection.fields[column]))],
      source: 'record',
      totalRows: 1,
      totalColumns: columns.length,
    }
  }
  return null
}

export function textProjectionTable(projection: DomainTextProjection): ProjectionTable {
  const content = projection.content.trim()
  const json = parseJsonTable(content)
  if (json)
    return json
  const lines = content.split(/\r?\n/).filter(line => line.trim().length > 0)
  const delimited = parseDelimitedTable(lines)
  if (delimited)
    return delimited
  const keyValues = parseKeyValueTable(lines)
  if (keyValues)
    return keyValues
  return {
    columns: [],
    rows: lines.slice(0, MAX_STRUCTURED_ROWS).map(line => [line]),
    source: 'lines',
    totalRows: lines.length,
    totalColumns: 1,
  }
}

function xlsTable(sheet: DomainXlsSheetProjection): ProjectionTable {
  const maximumColumns = Math.min(
    Math.max(0, ...sheet.rows.map(row => row.length)),
    MAX_STRUCTURED_COLUMNS,
  )
  const firstRow = sheet.rows[0] ?? []
  const hasHeader = firstRow.some(value => value.trim().length > 0)
  const columns = hasHeader
    ? Array.from({ length: maximumColumns }, (_, index) => firstRow[index] ?? '')
    : []
  return {
    columns,
    rows: sheet.rows.slice(hasHeader ? 1 : 0, MAX_STRUCTURED_ROWS).map(row => row.slice(0, maximumColumns)),
    source: 'xls',
    totalRows: sheet.rowCount,
    totalColumns: sheet.columnCount,
  }
}

function parseJsonTable(content: string): ProjectionTable | null {
  if (!content.startsWith('{') && !content.startsWith('['))
    return null
  try {
    const value: unknown = JSON.parse(content)
    const records = jsonRecords(value)
    if (records.length === 0)
      return null
    const columns = [...new Set(records.flatMap(record => Object.keys(record)))].slice(0, MAX_STRUCTURED_COLUMNS)
    return {
      columns,
      rows: records.slice(0, MAX_STRUCTURED_ROWS).map(record => columns.map(column => displayValue(record[column]))),
      source: 'json',
      totalRows: records.length,
      totalColumns: columns.length,
    }
  }
  catch {
    return null
  }
}

function jsonRecords(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value))
    return value.map((entry, index) => recordFromValue(entry, index))
  if (!isRecord(value))
    return []
  const arrays = Object.values(value).filter(Array.isArray)
  const recordArray = arrays.find(entries => entries.some(isRecord)) ?? arrays[0]
  if (recordArray)
    return recordArray.map((entry, index) => recordFromValue(entry, index))
  return [value]
}

function recordFromValue(value: unknown, index: number): Record<string, unknown> {
  if (isRecord(value))
    return flattenRecord(value)
  return { index, value }
}

function flattenRecord(record: Record<string, unknown>): Record<string, unknown> {
  const flattened: Record<string, unknown> = {}
  Object.entries(record).forEach(([key, value]) => {
    if (isRecord(value)) {
      Object.entries(value).forEach(([nestedKey, nestedValue]) => {
        flattened[`${key}.${nestedKey}`] = nestedValue
      })
      return
    }
    flattened[key] = value
  })
  return flattened
}

function parseDelimitedTable(lines: string[]): ProjectionTable | null {
  if (lines.length < 2)
    return null
  const delimiter = ['\t', ',', '|'].find(candidate => lines[0].split(candidate).length > 1)
  if (!delimiter)
    return null
  const rows = lines.map(line => line.split(delimiter).map(value => value.trim()))
  const columnCount = Math.max(...rows.map(row => row.length))
  if (columnCount < 2)
    return null
  return {
    columns: rows[0].slice(0, MAX_STRUCTURED_COLUMNS),
    rows: rows.slice(1, MAX_STRUCTURED_ROWS + 1).map(row => row.slice(0, MAX_STRUCTURED_COLUMNS)),
    source: 'delimited',
    totalRows: rows.length - 1,
    totalColumns: columnCount,
  }
}

function parseKeyValueTable(lines: string[]): ProjectionTable | null {
  const entries = lines.map((line) => {
    const separator = line.indexOf('=')
    if (separator <= 0)
      return null
    return [line.slice(0, separator).trim(), line.slice(separator + 1).trim()]
  })
  if (entries.length === 0 || entries.some(entry => entry == null))
    return null
  return {
    columns: [],
    rows: entries.slice(0, MAX_STRUCTURED_ROWS) as string[][],
    source: 'key-value',
    totalRows: entries.length,
    totalColumns: 2,
  }
}

function displayValue(value: unknown): string {
  if (value == null)
    return ''
  if (typeof value === 'string')
    return value
  if (typeof value === 'number' || typeof value === 'boolean')
    return String(value)
  return JSON.stringify(value)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value != null && !Array.isArray(value)
}
