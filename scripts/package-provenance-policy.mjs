const allowedUntrackedRoots = [
  '.cache/mir3-runtime',
  'artifacts',
  'dist',
  'node_modules',
  'src-tauri/binaries',
  'src-tauri/gen',
  'src-tauri/resources/runtime-baseline',
  'src-tauri/target',
]

const allowedUntrackedFiles = new Set([
  'src-tauri/resources/build-provenance.json',
])

/**
 * 解析 `git status --porcelain=v1 -z`，避免空格、引号或非 ASCII 路径改变安全判断。
 */
export function inspectPackageSourceStatus(rawStatus) {
  const trackedChanges = []
  const unsafeUntracked = []
  for (const entry of rawStatus.split('\0').filter(Boolean)) {
    if (entry.startsWith('?? ') || entry.startsWith('!! ')) {
      const path = normalizeRepositoryPath(entry.slice(3))
      if (!isAllowedUntrackedPath(path))
        unsafeUntracked.push(path)
      continue
    }
    trackedChanges.push(entry)
  }
  return { trackedChanges, unsafeUntracked }
}

export function isAllowedUntrackedPath(value) {
  const path = normalizeRepositoryPath(value)
  if (allowedUntrackedFiles.has(path))
    return true
  return allowedUntrackedRoots.some(root => path === root || path.startsWith(`${root}/`))
}

/**
 * 全仓扫描 ignored 项，但让 Git 直接跳过已知的大型可重建目录，避免遍历 node_modules/target。
 */
export function packageIgnoredScanPathspecs() {
  return [
    '.',
    ...allowedUntrackedRoots.map(root => `:(exclude)${root}`),
    ...[...allowedUntrackedFiles].map(path => `:(exclude)${path}`),
  ]
}

function normalizeRepositoryPath(value) {
  return value
    .replaceAll('\\', '/')
    .replace(/^\.\//u, '')
    .replace(/\/+$/u, '')
}
