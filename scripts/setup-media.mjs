import { createWriteStream } from 'node:fs'
import { mkdir, readFile, readdir, copyFile, writeFile, access } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { pipeline } from 'node:stream/promises'
import { Readable } from 'node:stream'
import { spawnSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const tools = path.join(root, '.tools', 'media')
const resources = path.join(root, 'src-tauri', 'resources')
await mkdir(tools, { recursive: true })
await mkdir(resources, { recursive: true })
try {
  await access(path.join(resources, 'ffmpeg.exe'))
  await access(path.join(resources, 'ffprobe.exe'))
  await access(path.join(resources, 'FFMPEG-LICENSE.txt'))
  console.log('Existing bundled FFmpeg components found; no download needed.')
  process.exit(0)
} catch { /* A clean checkout needs the media components. */ }

const headers = { 'User-Agent': 'SnowDown-Build/0.1', Accept: 'application/vnd.github+json' }
const response = await fetch('https://api.github.com/repos/yt-dlp/FFmpeg-Builds/releases/tags/latest', { headers, signal: AbortSignal.timeout(30000) })
if (!response.ok) throw new Error(`Cannot retrieve FFmpeg release: HTTP ${response.status}`)
const release = await response.json()
const asset = release.assets.find(item => item.name === 'ffmpeg-master-latest-win64-gpl.zip')
if (!asset) throw new Error('The official FFmpeg-Builds Windows asset was not found.')
const archive = path.join(tools, asset.name)
console.log(`Downloading FFmpeg from ${asset.browser_download_url}`)
const download = await fetch(asset.browser_download_url, { signal: AbortSignal.timeout(300000) })
if (!download.ok || !download.body) throw new Error(`FFmpeg download: HTTP ${download.status}`)
await pipeline(Readable.fromWeb(download.body), createWriteStream(archive))
const digest = createHash('sha256').update(await readFile(archive)).digest('hex')
if (asset.digest && asset.digest !== `sha256:${digest}`) throw new Error('FFmpeg checksum mismatch.')
const expanded = path.join(tools, 'expanded')
const extracted = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Expand-Archive -LiteralPath $env:SNOWDOWN_ARCHIVE -DestinationPath $env:SNOWDOWN_EXPANDED -Force'], {
  env: { ...process.env, SNOWDOWN_ARCHIVE: archive, SNOWDOWN_EXPANDED: expanded }, stdio: 'inherit', windowsHide: true,
})
if (extracted.status !== 0) throw new Error('Could not extract FFmpeg archive.')
async function findFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const nested = await Promise.all(entries.map(entry => entry.isDirectory() ? findFiles(path.join(directory, entry.name)) : [path.join(directory, entry.name)]))
  return nested.flat()
}
const files = await findFiles(expanded)
for (const name of ['ffmpeg.exe', 'ffprobe.exe']) {
  const source = files.find(file => path.basename(file) === name)
  if (!source) throw new Error(`Archive is missing ${name}`)
  await copyFile(source, path.join(resources, name))
}
const license = files.find(file => /^(LICENSE|COPYING)(\.txt|\.md)?$/i.test(path.basename(file)))
if (license) await copyFile(license, path.join(resources, 'FFMPEG-LICENSE.txt'))
else {
  const licenseResponse = await fetch('https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/COPYING.GPLv3')
  if (!licenseResponse.ok) throw new Error('Cannot retrieve FFmpeg GPL license.')
  await writeFile(path.join(resources, 'FFMPEG-LICENSE.txt'), await licenseResponse.text())
}
await writeFile(path.join(resources, 'ffmpeg-build.json'), JSON.stringify({
  release: release.tag_name, publishedAt: release.published_at, source: asset.browser_download_url,
  sha256: digest, license: 'GPL-3.0-or-later', sourceCode: 'https://github.com/yt-dlp/FFmpeg-Builds',
}, null, 2))
console.log('FFmpeg and ffprobe are ready in src-tauri/resources.')
