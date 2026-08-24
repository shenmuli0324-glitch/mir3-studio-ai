const dirtyPaths = new Set<string>()

export const MIR3_SAFE_FILES_PACKAGE = '@mir3-studio/dsh-mir3-safe-files'

export function setSafeFileDirty(path: string, dirty: boolean) {
  if (dirty)
    dirtyPaths.add(path)
  else
    dirtyPaths.delete(path)
}

export function hasDirtySafeFiles() {
  return dirtyPaths.size > 0
}

export function clearSafeFilesState() {
  dirtyPaths.clear()
}
