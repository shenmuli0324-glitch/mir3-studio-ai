export function isValidGuiRelativePath(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed || trimmed.startsWith('/') || trimmed.startsWith('\\') || trimmed.includes('..'))
    return false
  const hasControlCharacter = Array.from(trimmed).some(character => character.charCodeAt(0) < 32)
  return !hasControlCharacter && !/[<>:"|?*]/.test(trimmed)
}
