import { spawnSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
if (process.platform !== 'win32') throw new Error('The current release targets Windows x64.')
for (const [command, arguments_] of [
  [process.execPath, [path.join(root, 'scripts', 'build-extension.mjs')]],
  [process.execPath, [path.join(root, 'scripts', 'desktop.mjs'), 'build', ...process.argv.slice(2)]],
  [process.execPath, [path.join(root, 'scripts', 'build-installer.mjs')]],
]) {
  const child = spawnSync(command, arguments_, { cwd: root, stdio: 'inherit', windowsHide: true })
  if (child.error) throw child.error
  if (child.status !== 0) process.exit(child.status ?? 1)
}
