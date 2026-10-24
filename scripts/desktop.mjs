import { existsSync } from 'node:fs'
import { homedir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawn } from 'node:child_process'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const cargoDirectory = path.join(homedir(), '.cargo', 'bin')
const environment = { ...process.env }
if (existsSync(path.join(cargoDirectory, process.platform === 'win32' ? 'cargo.exe' : 'cargo'))) {
  const pathKey = Object.keys(environment).find(key => key.toLowerCase() === 'path') ?? 'PATH'
  environment[pathKey] = `${cargoDirectory}${path.delimiter}${environment[pathKey] ?? ''}`
}
const cli = path.join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], { cwd: root, env: environment, stdio: 'inherit', windowsHide: true })
child.on('error', error => { console.error(error.message); process.exitCode = 1 })
child.on('exit', code => { process.exitCode = code ?? 1 })
