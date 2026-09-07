import { describe, expect, it } from 'vitest'
import { coerceXlsCellValue, domainWorkbookViewport, isEditableXlsExtension, nextXlsCellEdit, XLS_COLUMN_PAGE_SIZE, XLS_ROW_PAGE_SIZE, xlsCellKey, xlsColumnLabel } from './domain-workbook-model'

describe('domain XLS workbook edits', () => {
  it('keeps the original expected value across continuous unsynced input', () => {
    const first = nextXlsCellEdit(undefined, 'Items', 2, 3, '10', '11')
    const second = nextXlsCellEdit(first, 'Items', 2, 3, '10', '12')
    expect(first.expectedValue).toBe('10')
    expect(second.expectedValue).toBe('10')
    expect(second.value).toBe(12)
  })

  it('moves the conflict baseline only after an edit was synced', () => {
    const synced = { ...nextXlsCellEdit(undefined, 'Items', 2, 3, '10', '11'), synced: true }
    const next = nextXlsCellEdit(synced, 'Items', 2, 3, '10', '12')
    expect(next.expectedValue).toBe('11')
  })

  it('preserves text identifiers while retaining number and blank cell types', () => {
    expect(coerceXlsCellValue('0012', '0011')).toBe('0012')
    expect(coerceXlsCellValue('12.5', '11')).toBe(12.5)
    expect(coerceXlsCellValue('', 'value')).toBeNull()
    expect(xlsCellKey('a.xls', 'Sheet', 1, 2)).not.toBe(xlsCellKey('a.xls', 'Sheet', 12, 0))
  })

  it('makes the last row and column of a large sheet reachable', () => {
    const rowCount = XLS_ROW_PAGE_SIZE * 2 + 1
    const columnCount = XLS_COLUMN_PAGE_SIZE * 2 + 1
    const sheet = {
      sheet: 'Large',
      rowCount,
      columnCount,
      rows: Array.from({ length: rowCount }, (_, row) => Array.from({ length: columnCount }, (_, column) => `${row}:${column}`)),
      sourceSha256: 'base',
    }
    const viewport = domainWorkbookViewport(sheet, Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER)

    expect(viewport.rowPage).toBe(2)
    expect(viewport.columnPage).toBe(2)
    expect(viewport.rows).toEqual([sheet.rows[rowCount - 1]])
    expect(viewport.columns).toEqual([columnCount - 1])
    expect(viewport.rows[0][viewport.columns[0]]).toBe(`${rowCount - 1}:${columnCount - 1}`)
    expect(xlsColumnLabel(26)).toBe('AA')
  })

  it('accepts only BIFF .xls files for structured editing', () => {
    expect(isEditableXlsExtension('xls')).toBe(true)
    expect(isEditableXlsExtension('XLS')).toBe(true)
    expect(isEditableXlsExtension('xlsx')).toBe(false)
    expect(isEditableXlsExtension('xlsm')).toBe(false)
  })
})
