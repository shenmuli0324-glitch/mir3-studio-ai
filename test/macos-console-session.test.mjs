import { describe, expect, it, vi } from 'vitest'
import {
  assertUnlockedMacosConsoleSession,
  parseMacosConsoleSession,
  readMacosConsoleSession,
} from '../scripts/macos-console-session.mjs'

describe('macOS console session smoke prerequisite', () => {
  it('reads ioreg XML through plutil and accepts an unlocked console session', () => {
    const registry = '<?xml version="1.0"?><plist><dict /></plist>'
    const json = sessionJson({ consoleLocked: false, screenLocked: false })
    const run = vi.fn((command) => {
      if (command === '/usr/sbin/ioreg')
        return registry
      return json
    })

    const session = readMacosConsoleSession(run)

    expect(session).toEqual({
      consoleLocked: false,
      lockedAt: '2023-11-14T22:13:20.000Z',
      loginDone: true,
      onConsole: true,
      screenLocked: false,
      sessionId: 257,
    })
    expect(run).toHaveBeenNthCalledWith(1, '/usr/sbin/ioreg', ['-n', 'Root', '-d1', '-a'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    expect(run).toHaveBeenNthCalledWith(2, '/usr/bin/plutil', ['-convert', 'json', '-o', '-', '-'], {
      encoding: 'utf8',
      input: registry,
      stdio: ['pipe', 'pipe', 'pipe'],
    })
  })

  it('fails fast with non-sensitive evidence when the console is locked', () => {
    function readLockedSession() {
      return parseMacosConsoleSession(sessionJson({ consoleLocked: true, screenLocked: true }))
    }

    function assertUnlocked() {
      return assertUnlockedMacosConsoleSession(readLockedSession)
    }

    expect(assertUnlocked).toThrowError(
      /NATIVE_UI_SESSION_LOCKED:.*IOConsoleLocked=true.*onConsole=true.*loginDone=true.*screenLocked=true.*sessionId=257.*lockedAt=2023-11-14T22:13:20\.000Z.*unlock this Mac and rerun pnpm smoke:mac/u,
    )
    expect(assertUnlocked).not.toThrowError(/private-user-name-must-not-appear/u)
  })

  it('uses the root console lock state when macOS omits the per-session lock field', () => {
    const unlocked = parseMacosConsoleSession(sessionJson({ consoleLocked: false }))
    const locked = parseMacosConsoleSession(sessionJson({ consoleLocked: true }))

    expect(unlocked.screenLocked).toBe(false)
    expect(locked.screenLocked).toBe(true)
    expect(() => assertUnlockedMacosConsoleSession(() => unlocked)).not.toThrow()
    expect(() => assertUnlockedMacosConsoleSession(() => locked)).toThrowError(/NATIVE_UI_SESSION_LOCKED/u)
  })

  it('fails closed when no on-console session exists', () => {
    const json = JSON.stringify({
      IOConsoleLocked: false,
      IOConsoleUsers: [{
        CGSSessionScreenIsLocked: false,
        kCGSessionLoginDoneKey: true,
        kCGSSessionIDKey: 300,
        kCGSSessionOnConsoleKey: false,
      }],
    })

    expect(() => parseMacosConsoleSession(json)).toThrowError(
      /NATIVE_UI_SESSION_UNAVAILABLE:.*sign in, unlock this Mac, and rerun pnpm smoke:mac/u,
    )
  })

  it('fails closed when console state JSON cannot be parsed', () => {
    expect(() => parseMacosConsoleSession('{invalid')).toThrowError(
      /NATIVE_UI_SESSION_UNVERIFIED:.*not valid JSON.*unlock this Mac and rerun pnpm smoke:mac/u,
    )
  })
})

function sessionJson({ consoleLocked, screenLocked }) {
  return JSON.stringify({
    IOConsoleLocked: consoleLocked,
    IOConsoleUsers: [{
      CGSSessionScreenIsLocked: screenLocked,
      CGSSessionScreenLockedTime: 1_700_000_000,
      kCGSessionLoginDoneKey: true,
      kCGSSessionIDKey: 257,
      kCGSSessionOnConsoleKey: true,
      kCGSSessionUserIDKey: 501,
      kCGSSessionUserNameKey: 'private-user-name-must-not-appear',
    }],
  })
}
