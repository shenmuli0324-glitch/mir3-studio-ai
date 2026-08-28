import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const root = join(scriptDirectory, '..')
const template = readFileSync(join(root, 'src-tauri', 'templates', 'installer.nsi'), 'utf8')
const mainBinaryName = '${' + 'MAINBINARYNAME}'
const mainBinarySourcePath = '${' + 'MAINBINARYSRCPATH}'

function positionWithin(section, text) {
  const position = section.indexOf(text)
  assert.notEqual(position, -1, `Windows installer is missing: ${text}`)
  return position
}

function sectionBetween(start, end) {
  const startPosition = template.indexOf(start)
  const endPosition = template.indexOf(end, startPosition + start.length)
  assert.notEqual(startPosition, -1, `Windows installer is missing section: ${start}`)
  assert.notEqual(endPosition, -1, `Windows installer is missing section boundary: ${end}`)
  return template.slice(startPosition, endPosition)
}

const cleanup = sectionBetween('!macro TerminateStudioProcessTrees', '!macroend')
const mainTree = positionWithin(cleanup, `/IM "${mainBinaryName}.exe" /T /F`)
const liveCoreParent = positionWithin(cleanup, '$$mcp.ParentProcessId')
const persistedCore = positionWithin(cleanup, '.mir3-core.pid')
const mcpTree = positionWithin(cleanup, '/IM "mir3-mcp.exe" /T /F')
assert.ok(mainTree < persistedCore, 'Windows installer must stop the Studio tree before stale Core')
assert.ok(mainTree < liveCoreParent, 'Windows installer must stop Studio before discovering live Core')
assert.ok(liveCoreParent < persistedCore, 'Windows installer must stop live Core before PID fallback')
assert.ok(persistedCore < mcpTree, 'Windows installer must stop stale Core before mir3-mcp')
assert.match(
  cleanup,
  /GetFullPath\('\$INSTDIR\\mir3-mcp\.exe'\).*ExecutablePath.*-eq \$\$target/,
  'Live Core discovery must be scoped to the installed mir3-mcp path',
)
assert.match(
  cleanup,
  /\$\$parent\.Name -eq 'node\.exe'.*\$\$parent\.CommandLine -like '\*deepseek-ai\*dsh\*lib\*bin\.js\*--profile web\*'/,
  'Live Core discovery must verify the Dsh web node command line',
)
assert.match(
  cleanup,
  /taskkill\.exe \/PID \$\$parent\.ProcessId \/T \/F/,
  'Windows installer must terminate the supervising Core process tree',
)
assert.match(cleanup, /IntFmt \$R9 "%u" \$R9/, 'Persisted Core PID must be sanitized')
assert.match(cleanup, /IntFmt \$R6 "%u" \$R6/, 'Persisted Core port must be sanitized')
assert.match(
  cleanup,
  /Get-NetTCPConnection -State Listen -LocalPort \$R6.*Where-Object OwningProcess -eq \$R9/,
  'Persisted Core PID must own its recorded listening port before termination',
)

for (const [start, end, mutation] of [
  ['Section Install', 'SectionEnd', `File "${mainBinarySourcePath}"`],
  ['Section Uninstall', 'SectionEnd', `Delete "$INSTDIR\\${mainBinaryName}.exe"`],
]) {
  const section = sectionBetween(start, end)
  const cleanupPosition = positionWithin(section, '!insertmacro TerminateStudioProcessTrees')
  const mainCheck = positionWithin(section, `!insertmacro CheckIfAppIsRunning "${mainBinaryName}.exe"`)
  const mcpCheck = positionWithin(section, '!insertmacro CheckIfAppIsRunning "mir3-mcp.exe"')
  const mutationPosition = positionWithin(section, mutation)
  assert.ok(cleanupPosition < mainCheck, `${start} must terminate process trees before checks`)
  assert.ok(mainCheck < mcpCheck, `${start} must verify the main process before MCP`)
  assert.ok(mcpCheck < mutationPosition, `${start} must verify file unlock before mutation`)
}

console.log('Windows installer process cleanup audit passed')
