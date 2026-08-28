import { resolve } from 'node:path'
import process from 'node:process'
import { nextProductVersion, readProductVersions, updateProductVersions } from './lib/product-version.mjs'

const root = resolve(import.meta.dirname, '..')
const current = readProductVersions(root).get('package.json')
const requested = process.argv.slice(2).find(value => value !== '--') || 'patch'
const version = nextProductVersion(current, requested)
if (version === current)
  throw new Error(`Product version is already ${version}`)

updateProductVersions(root, version)

process.stdout.write(`MIR3 Studio AI version: ${current} -> ${version}\n`)
