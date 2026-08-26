import { Buffer } from 'node:buffer'
import { createHash } from 'node:crypto'
import { closeSync, lstatSync, openSync, readdirSync, readlinkSync, readSync, realpathSync } from 'node:fs'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'

/**
 * 对实际参与构建的依赖安装树做内容寻址；不跟随符号链接，但把链接目标和权限纳入身份。
 */
export function hashDependencyInputTree(directory) {
  return hashInputTree(directory)
}

export function assertDependencyInputTree(expected, actual) {
  if (!expected
    || expected.sha256 !== actual.sha256
    || expected.fileCount !== actual.fileCount
    || expected.directoryCount !== actual.directoryCount
    || expected.symlinkCount !== actual.symlinkCount
    || expected.totalBytes !== actual.totalBytes
    || JSON.stringify(expected.excludedRelativePaths) !== JSON.stringify(actual.excludedRelativePaths)) {
    throw new Error(
      `PACKAGE_DEPENDENCY_INPUT_CHANGED: expected ${expected?.sha256 ?? 'missing'}, got ${actual.sha256}`,
    )
  }
}

export function hashInputTree(directory, excludedRelativePaths = []) {
  const rootStats = lstatSync(directory)
  if (rootStats.isSymbolicLink())
    throw new Error('PACKAGE_DEPENDENCY_ROOT_SYMLINK_FORBIDDEN')
  if (!rootStats.isDirectory())
    throw new Error('PACKAGE_DEPENDENCY_ROOT_INVALID')
  const canonicalRoot = realpathSync(directory)
  const exclusions = new Set(excludedRelativePaths.map(normalizePath))
  const records = []
  collectTreeRecords(canonicalRoot, canonicalRoot, exclusions, records)
  records.sort((left, right) => comparePaths(left.path, right.path))
  const hasher = createHash('sha256')
  let fileCount = 0
  let directoryCount = 0
  let symlinkCount = 0
  let totalBytes = 0
  for (const record of records) {
    hasher.update(`${record.type}\0${record.path}\0${record.mode}\0${record.size}\0${record.sha256}\n`)
    if (record.type === 'file') {
      fileCount += 1
      totalBytes += record.size
    }
    else if (record.type === 'directory') {
      directoryCount += 1
    }
    else if (record.type === 'symlink') {
      symlinkCount += 1
    }
  }
  return {
    sha256: hasher.digest('hex'),
    fileCount,
    directoryCount,
    symlinkCount,
    totalBytes,
    excludedRelativePaths: [...exclusions].sort(comparePaths),
  }
}

function collectTreeRecords(rootDirectory, directory, exclusions, records) {
  const entries = readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => comparePaths(left.name, right.name))
  for (const entry of entries) {
    const path = `${directory}${sep}${entry.name}`
    const relativePath = normalizePath(relative(rootDirectory, path))
    if (isExcluded(relativePath, exclusions))
      continue
    const stats = lstatSync(path)
    if (stats.isSymbolicLink()) {
      const target = readlinkSync(path)
      const resolvedTarget = resolve(dirname(path), target)
      const resolvedRelative = relative(rootDirectory, resolvedTarget)
      if (resolvedRelative === '..' || resolvedRelative.startsWith(`..${sep}`) || isAbsolute(resolvedRelative))
        throw new Error(`PACKAGE_DEPENDENCY_SYMLINK_ESCAPE: ${relativePath}`)
      let canonicalTarget
      try {
        canonicalTarget = realpathSync(resolvedTarget)
      }
      catch {
        throw new Error(`PACKAGE_DEPENDENCY_SYMLINK_TARGET_INVALID: ${relativePath}`)
      }
      const targetRelative = relative(rootDirectory, canonicalTarget)
      if (targetRelative === '..' || targetRelative.startsWith(`..${sep}`) || isAbsolute(targetRelative))
        throw new Error(`PACKAGE_DEPENDENCY_SYMLINK_ESCAPE: ${relativePath}`)
      records.push({
        type: 'symlink',
        path: relativePath,
        mode: stats.mode & 0o7777,
        size: Buffer.byteLength(target),
        sha256: sha256(target),
      })
      continue
    }
    if (stats.isDirectory()) {
      records.push({ type: 'directory', path: relativePath, mode: stats.mode & 0o7777, size: 0, sha256: '' })
      collectTreeRecords(rootDirectory, path, exclusions, records)
      continue
    }
    if (stats.isFile()) {
      records.push({
        type: 'file',
        path: relativePath,
        mode: stats.mode & 0o7777,
        size: stats.size,
        sha256: hashFile(path),
      })
      continue
    }
    throw new Error(`PACKAGE_DEPENDENCY_INPUT_UNSUPPORTED: ${relativePath}`)
  }
}

function hashFile(path) {
  const descriptor = openSync(path, 'r')
  const hasher = createHash('sha256')
  const buffer = Buffer.allocUnsafe(1024 * 1024)
  try {
    while (true) {
      const bytesRead = readSync(descriptor, buffer, 0, buffer.length, null)
      if (bytesRead === 0)
        break
      hasher.update(buffer.subarray(0, bytesRead))
    }
  }
  finally {
    closeSync(descriptor)
  }
  return hasher.digest('hex')
}

function isExcluded(path, exclusions) {
  return [...exclusions].some(excluded => path === excluded || path.startsWith(`${excluded}/`))
}

function normalizePath(value) {
  return value.split(sep).join('/').replace(/^\.\//u, '').replace(/\/+$/u, '')
}

function comparePaths(left, right) {
  if (left === right)
    return 0
  return left < right ? -1 : 1
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}
