import { execFileSync } from 'node:child_process'

const IOREG_PATH = '/usr/sbin/ioreg'
const PLUTIL_PATH = '/usr/bin/plutil'

export function readMacosConsoleSession(run = execFileSync) {
  let registry
  try {
    registry = run(IOREG_PATH, ['-n', 'Root', '-d1', '-a'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    })
  }
  catch {
    throw sessionError(
      'NATIVE_UI_SESSION_UNVERIFIED',
      'macOS console state could not be read with ioreg; unlock this Mac and rerun pnpm smoke:mac',
    )
  }

  let json
  try {
    json = run(PLUTIL_PATH, ['-convert', 'json', '-o', '-', '-'], {
      encoding: 'utf8',
      input: registry,
      stdio: ['pipe', 'pipe', 'pipe'],
    })
  }
  catch {
    throw sessionError(
      'NATIVE_UI_SESSION_UNVERIFIED',
      'macOS console state could not be converted with plutil; unlock this Mac and rerun pnpm smoke:mac',
    )
  }

  return parseMacosConsoleSession(json)
}

export function parseMacosConsoleSession(json) {
  let root
  try {
    root = JSON.parse(json)
  }
  catch {
    throw sessionError(
      'NATIVE_UI_SESSION_UNVERIFIED',
      'macOS console state was not valid JSON; unlock this Mac and rerun pnpm smoke:mac',
    )
  }

  if (!root || typeof root !== 'object' || Array.isArray(root)) {
    throw sessionError(
      'NATIVE_UI_SESSION_UNVERIFIED',
      'macOS console state did not contain an IORegistry root object; unlock this Mac and rerun pnpm smoke:mac',
    )
  }

  const sessions = Array.isArray(root.IOConsoleUsers) ? root.IOConsoleUsers : []
  const consoleSession = sessions.find(session => session?.kCGSSessionOnConsoleKey === true)
  if (!consoleSession) {
    throw sessionError(
      'NATIVE_UI_SESSION_UNAVAILABLE',
      'no logged-in on-console macOS session is available; sign in, unlock this Mac, and rerun pnpm smoke:mac',
    )
  }

  const consoleLocked = root.IOConsoleLocked
  const screenLocked = consoleSession.CGSSessionScreenIsLocked
  const loginDone = consoleSession.kCGSessionLoginDoneKey
  if (typeof consoleLocked !== 'boolean' || typeof screenLocked !== 'boolean' || loginDone !== true) {
    throw sessionError(
      'NATIVE_UI_SESSION_UNVERIFIED',
      `macOS console state was incomplete (${formatSessionEvidence({ consoleLocked, consoleSession, screenLocked })}); unlock this Mac and rerun pnpm smoke:mac`,
    )
  }

  return {
    consoleLocked,
    loginDone,
    onConsole: true,
    screenLocked,
    sessionId: safeInteger(consoleSession.kCGSSessionIDKey),
    lockedAt: lockedAtIso(consoleSession.CGSSessionScreenLockedTime),
  }
}

export function assertUnlockedMacosConsoleSession(readSession = readMacosConsoleSession) {
  const session = readSession()
  if (session.consoleLocked || session.screenLocked) {
    throw sessionError(
      'NATIVE_UI_SESSION_LOCKED',
      `native UI smoke requires an unlocked interactive macOS session (${formatSessionEvidence({ consoleLocked: session.consoleLocked, consoleSession: session, screenLocked: session.screenLocked })}); unlock this Mac and rerun pnpm smoke:mac`,
    )
  }
  return session
}

function formatSessionEvidence({ consoleLocked, consoleSession, screenLocked }) {
  return [
    `IOConsoleLocked=${evidenceValue(consoleLocked)}`,
    `onConsole=${evidenceValue(consoleSession.onConsole ?? consoleSession.kCGSSessionOnConsoleKey)}`,
    `loginDone=${evidenceValue(consoleSession.loginDone ?? consoleSession.kCGSessionLoginDoneKey)}`,
    `screenLocked=${evidenceValue(screenLocked)}`,
    `sessionId=${evidenceValue(consoleSession.sessionId ?? consoleSession.kCGSSessionIDKey)}`,
    `lockedAt=${evidenceValue(consoleSession.lockedAt ?? lockedAtIso(consoleSession.CGSSessionScreenLockedTime))}`,
  ].join(', ')
}

function evidenceValue(value) {
  if (value === undefined || value === null)
    return 'unknown'
  return String(value)
}

function safeInteger(value) {
  return Number.isSafeInteger(value) ? value : null
}

function lockedAtIso(value) {
  if (!Number.isSafeInteger(value) || value < 0)
    return null
  const date = new Date(value * 1_000)
  return Number.isNaN(date.valueOf()) ? null : date.toISOString()
}

function sessionError(code, message) {
  const error = new Error(`${code}: ${message}`)
  error.code = code
  return error
}
