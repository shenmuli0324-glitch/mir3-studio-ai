import type { SafeXlsSheet } from './types'

export const XLS_ROW_PAGE_SIZE = 60
export const XLS_COLUMN_PAGE_SIZE = 16

export interface DomainWorkbookCellEdit {
  sheet: string
  row: number
  column: number
  expectedValue: string
  value: string | number | boolean | null
  synced: boolean
}

export interface DomainWorkbookViewport {
  rowPage: number
  rowPages: number
  rowStart: number
  rows: string[][]
  columnPage: number
  columnPages: number
  columnStart: number
  columns: number[]
}

export function xlsCellKey(path: string, sheet: string, row: number, column: number): string {
  return JSON.stringify([path, sheet, row, column])
}

export function coerceXlsCellValue(input: string, previous: string): string | number | boolean | null {
  if (input.length === 0)
    return null
  if ((previous === 'true' || previous === 'false') && (input === 'true' || input === 'false'))
    return input === 'true'
  if (isPlainNumber(previous) && isPlainNumber(input))
    return Number(input)
  return input
}

export function nextXlsCellEdit(existing: DomainWorkbookCellEdit | undefined, sheet: string, row: number, column: number, sourceValue: string, input: string): DomainWorkbookCellEdit {
  let expectedValue = sourceValue
  if (existing?.synced)
    expectedValue = xlsInputValue(existing.value)
  else if (existing)
    expectedValue = existing.expectedValue
  return {
    sheet,
    row,
    column,
    expectedValue,
    value: coerceXlsCellValue(input, expectedValue),
    synced: false,
  }
}

export function xlsInputValue(value: DomainWorkbookCellEdit['value']): string {
  if (value == null)
    return ''
  return String(value)
}

export function domainWorkbookViewport(sheet: SafeXlsSheet, rowPage: number, columnPage: number): DomainWorkbookViewport {
  const rowPages = xlsPageCount(sheet.rowCount, XLS_ROW_PAGE_SIZE)
  const columnPages = xlsPageCount(sheet.columnCount, XLS_COLUMN_PAGE_SIZE)
  const safeRowPage = clampPage(rowPage, rowPages)
  const safeColumnPage = clampPage(columnPage, columnPages)
  const rowStart = safeRowPage * XLS_ROW_PAGE_SIZE
  const columnStart = safeColumnPage * XLS_COLUMN_PAGE_SIZE
  return {
    rowPage: safeRowPage,
    rowPages,
    rowStart,
    rows: sheet.rows.slice(rowStart, rowStart + XLS_ROW_PAGE_SIZE),
    columnPage: safeColumnPage,
    columnPages,
    columnStart,
    columns: Array.from(
      { length: Math.min(XLS_COLUMN_PAGE_SIZE, Math.max(0, sheet.columnCount - columnStart)) },
      (_, index) => columnStart + index,
    ),
  }
}

export function xlsColumnLabel(index: number): string {
  let value = index + 1
  let label = ''
  while (value > 0) {
    const remainder = (value - 1) % 26
    label = String.fromCharCode(65 + remainder) + label
    value = Math.floor((value - 1) / 26)
  }
  return label
}

export function isEditableXlsExtension(extension?: string | null): boolean {
  return extension?.toLowerCase() === 'xls'
}

function xlsPageCount(total: number, size: number): number {
  return Math.max(1, Math.ceil(total / size))
}

function clampPage(page: number, pages: number): number {
  return Math.min(Math.max(0, page), pages - 1)
}

function isPlainNumber(value: string): boolean {
  if (!/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value))
    return false
  return !/^[-+]?0\d/.test(value)
}
