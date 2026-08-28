import { describe, expect, it } from 'vitest'
import {
  nextProductVersion,
  PRODUCT_VERSION_TARGETS,
  readTargetVersion,
  updateTargetVersion,
} from '../scripts/lib/product-version.mjs'

describe('product version helpers', () => {
  it('bumps stable product versions', () => {
    expect(nextProductVersion('0.9.21', 'patch')).toBe('0.9.22')
    expect(nextProductVersion('0.9.21', 'minor')).toBe('0.10.0')
    expect(nextProductVersion('0.9.21', 'major')).toBe('1.0.0')
    expect(nextProductVersion('0.9.21', '2.3.4')).toBe('2.3.4')
  })

  it.each(['\n', '\r\n'])('updates Cargo files with %j newlines', (newline) => {
    const cargoToml = cargoTarget('src-tauri/Cargo.toml')
    const cargoLock = cargoTarget('src-tauri/Cargo.lock')
    const toml = `[package]${newline}name = "mir3-studio-ai"${newline}version = "0.9.21"${newline}`
    const lock = `[[package]]${newline}name = "mir3-studio-ai"${newline}version = "0.9.21"${newline}`

    const updatedToml = updateTargetVersion(toml, cargoToml, '0.9.22')
    const updatedLock = updateTargetVersion(lock, cargoLock, '0.9.22')

    expect(readTargetVersion(updatedToml, cargoToml)).toBe('0.9.22')
    expect(readTargetVersion(updatedLock, cargoLock)).toBe('0.9.22')
    expect(updatedToml.includes(newline)).toBe(true)
    expect(updatedLock.includes(newline)).toBe(true)
  })

  it('preserves CRLF JSON formatting', () => {
    const packageTarget = cargoTarget('package.json')
    const updated = updateTargetVersion('{\r\n  "version": "0.9.21"\r\n}\r\n', packageTarget, '0.9.22')

    expect(updated).toBe('{\r\n  "version": "0.9.22"\r\n}\r\n')
  })
})

function cargoTarget(path) {
  const target = PRODUCT_VERSION_TARGETS.find(candidate => candidate.path === path)
  if (!target)
    throw new Error(`Missing product version target ${path}`)
  return target
}
