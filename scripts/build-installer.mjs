import { spawnSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const environment = { ...process.env }
// Let Windows PowerShell find its own modules when invoked from PowerShell 7/npm.
for (const key of Object.keys(environment)) if (key.toLowerCase() === 'psmodulepath') delete environment[key]
const child = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(root, 'scripts', 'build-installer.ps1'), ...process.argv.slice(2)], {
  cwd: root, env: environment, stdio: 'inherit', windowsHide: true,
})
if (child.error) throw child.error
process.exitCode = child.status ?? 1
