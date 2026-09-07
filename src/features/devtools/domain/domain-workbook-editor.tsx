import type { DomainWorkbookCellEdit } from './domain-workbook-model'
import type { DomainFileRecord, DomainWorkbookData } from './types'
import { Button } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { domainWorkbookViewport, nextXlsCellEdit, xlsCellKey, xlsColumnLabel, xlsInputValue } from './domain-workbook-model'

export function DomainWorkbookEditor({ file, data, loading, error, editable, busy, sheetName, edits, onSheet, onCellChange }: {
  file: DomainFileRecord
  data?: DomainWorkbookData
  loading: boolean
  error: Error | null
  editable: boolean
  busy: boolean
  sheetName: string
  edits: Record<string, DomainWorkbookCellEdit>
  onSheet: (sheet: string) => void
  onCellChange: (edit: DomainWorkbookCellEdit) => void
}) {
  const { t } = useTranslation()
  const [rowPage, setRowPage] = useState(0)
  const [columnPage, setColumnPage] = useState(0)
  if (loading)
    return <WorkbookNotice title={t('studio.devtools.workbook.loading')} description={file.path} />
  if (error || !data)
    return <WorkbookNotice title={t('studio.devtools.workbook.failed')} description={String(error ?? '')} />
  const sheet = data.sheets.find(item => item.sheet === sheetName) ?? data.sheets[0]
  if (!sheet)
    return <WorkbookNotice title={t('studio.devtools.workbook.empty')} description={file.path} />
  const viewport = domainWorkbookViewport(sheet, rowPage, columnPage)

  function changeSheet(value: string) {
    setRowPage(0)
    setColumnPage(0)
    onSheet(value)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      <header className="flex shrink-0 items-center justify-between gap-3 border-b border-line px-4 py-2">
        <span className="min-w-0">
          <strong className="block truncate text-xs text-ink">{file.path}</strong>
          <small className="text-[9px] text-muted">{t(workbookFormatKey(editable))}</small>
        </span>
        <div className="flex items-center gap-2">
          <span className="text-[9px] text-muted">{t('studio.devtools.workbook.loaded', { sheets: data.sheets.length, rows: totalRows(data) })}</span>
          <select className="max-w-52 rounded-md border border-line bg-panel2 px-2 py-1 text-[10px] text-ink outline-none" value={sheet.sheet} aria-label={t('studio.devtools.workbook.sheet')} onChange={event => changeSheet(event.target.value)}>
            {data.sheets.map(item => <option key={item.sheet} value={item.sheet}>{item.sheet}</option>)}
          </select>
        </div>
      </header>
      <WorkbookPager
        rowPage={viewport.rowPage}
        rowPages={viewport.rowPages}
        columnPage={viewport.columnPage}
        columnPages={viewport.columnPages}
        onRowPage={setRowPage}
        onColumnPage={setColumnPage}
      />
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="min-w-max border-separate border-spacing-0 font-mono text-[10px] text-ink">
          <thead className="sticky top-0 z-20 bg-panel2">
            <tr>
              <th className="sticky left-0 z-30 min-w-12 border-b border-r border-line bg-panel2 px-2 py-1.5 text-muted">#</th>
              {viewport.columns.map(column => <th key={column} className="min-w-28 border-b border-r border-line px-2 py-1.5 text-muted">{xlsColumnLabel(column)}</th>)}
            </tr>
          </thead>
          <tbody>
            {viewport.rows.map((row, relativeRow) => {
              const rowIndex = viewport.rowStart + relativeRow
              return (
                <tr key={rowIndex}>
                  <th className="sticky left-0 z-10 border-b border-r border-line bg-panel2 px-2 py-1 text-right font-normal text-muted">{rowIndex + 1}</th>
                  {viewport.columns.map((column) => {
                    const sourceValue = row[column] ?? ''
                    const edit = edits[xlsCellKey(file.path, sheet.sheet, rowIndex, column)]
                    const value = edit ? xlsInputValue(edit.value) : sourceValue
                    return (
                      <td key={column} className="border-b border-r border-line bg-canvas p-0 focus-within:bg-panel2">
                        <input
                          className="h-7 w-28 bg-transparent px-2 text-ink outline-none"
                          value={value}
                          readOnly={!editable || busy}
                          aria-label={t('studio.devtools.workbook.cell', { sheet: sheet.sheet, row: rowIndex + 1, column: xlsColumnLabel(column) })}
                          onChange={event => onCellChange(nextXlsCellEdit(edit, sheet.sheet, rowIndex, column, sourceValue, event.target.value))}
                        />
                      </td>
                    )
                  })}
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function WorkbookPager({ rowPage, rowPages, columnPage, columnPages, onRowPage, onColumnPage }: {
  rowPage: number
  rowPages: number
  columnPage: number
  columnPages: number
  onRowPage: (page: number) => void
  onColumnPage: (page: number) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-3 py-1.5">
      <span className="text-[9px] text-muted">{t('studio.devtools.workbook.page_status', { row: rowPage + 1, rows: rowPages, column: columnPage + 1, columns: columnPages })}</span>
      <div className="flex gap-1">
        <Button size="sm" variant="ghost" isDisabled={rowPage <= 0} onPress={() => onRowPage(rowPage - 1)}>{t('studio.devtools.workbook.previous_rows')}</Button>
        <Button size="sm" variant="ghost" isDisabled={rowPage + 1 >= rowPages} onPress={() => onRowPage(rowPage + 1)}>{t('studio.devtools.workbook.next_rows')}</Button>
        <Button size="sm" variant="ghost" isDisabled={columnPage <= 0} onPress={() => onColumnPage(columnPage - 1)}>{t('studio.devtools.workbook.previous_columns')}</Button>
        <Button size="sm" variant="ghost" isDisabled={columnPage + 1 >= columnPages} onPress={() => onColumnPage(columnPage + 1)}>{t('studio.devtools.workbook.next_columns')}</Button>
      </div>
    </div>
  )
}

function WorkbookNotice({ title, description }: { title: string, description: string }) {
  return (
    <div className="grid min-h-0 flex-1 place-items-center p-6">
      <div className="max-w-sm text-center">
        <strong className="text-sm text-ink">{title}</strong>
        <p className="mt-2 text-xs leading-5 text-muted">{description}</p>
      </div>
    </div>
  )
}

function totalRows(data: DomainWorkbookData): number {
  return data.sheets.reduce((total, sheet) => total + sheet.rowCount, 0)
}

function workbookFormatKey(editable: boolean): string {
  if (editable)
    return 'studio.devtools.workbook.format'
  return 'studio.devtools.source.xls_readonly'
}
