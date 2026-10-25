// Optional real-browser regression: install Playwright, or set
// SNOWDOWN_PLAYWRIGHT_PATH to its index.mjs. Start Vite before running this file.
import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
const { chromium } = await import(process.env.SNOWDOWN_PLAYWRIGHT_PATH ? pathToFileURL(process.env.SNOWDOWN_PLAYWRIGHT_PATH).href : 'playwright')
const output = path.resolve('.tools/v023-mode-ui')
await mkdir(output, { recursive: true })
const browser = await chromium.launch({ headless: true })
const page = await browser.newPage({ viewport: { width: 1180, height: 820 }, reducedMotion: 'no-preference' })
const errors = []
page.on('pageerror', error => errors.push(error.message))
const rect = selector => page.locator(`.app-shell ${selector}`).boundingBox()
async function settle() { await page.waitForFunction(() => !document.querySelector('.mode-morph-bar') && !document.querySelector('.mode-switching')) }
async function switchMode(forward, screenshots = false) {
  const selector = forward ? '.standard-mode .brand' : '.minimal-brand'
  const from = await rect(forward ? '.link-form' : '.minimal-input-bar')
  const brand = await rect(selector)
  const logo = await rect(`${selector} .brand-mark`)
  const word = await rect(`${selector} > span:last-child`)
  const origin = { x: brand.x + 17, y: brand.y + 19 }
  // Capture in the page's own rAF loop, independent of screenshot I/O latency.
  const recording = page.evaluate(() => new Promise(resolve => {
    const frames = []; let began = false
    function read() {
      const bar = document.querySelector('.mode-morph-bar')
      if (bar) {
        began = true
        const b = bar.getBoundingClientRect(), s = getComputedStyle(bar)
        const mark=document.querySelector('.mode-morph-brand .brand-mark').getBoundingClientRect(),word=document.querySelector('.mode-morph-brand > span:last-child').getBoundingClientRect()
        frames.push({ x: b.x, y: b.y, width: b.width, height: b.height, radius: parseFloat(s.borderRadius), clip: getComputedStyle(document.querySelector('.mode-reveal-layer')).clipPath, shaft: bar.querySelector('.mode-icon-shaft')?.getAttribute('d'), tray: Number(bar.querySelector('.mode-icon-tray')?.getAttribute('opacity')), button: bar.querySelector('.mode-morph-action').getBoundingClientRect().width,logo:{x:mark.x,y:mark.y,width:mark.width,height:mark.height},word:{x:word.x,y:word.y,width:word.width,height:word.height} })
      }
      if (began && !bar) resolve(frames)
      else requestAnimationFrame(read)
    }
    requestAnimationFrame(read)
  }))
  await page.mouse.click(origin.x, origin.y)
  if (screenshots) {
    await page.waitForTimeout(120)
    await page.screenshot({ path: path.join(output, `${forward ? 'enter' : 'return'}-early.png`) })
    await page.waitForTimeout(180)
    await page.screenshot({ path: path.join(output, `${forward ? 'enter' : 'return'}-middle.png`) })
  }
  const frames = await recording
  await settle()
  const to = await rect(forward ? '.minimal-input-bar' : '.link-form')
  assert.ok(frames.length >= 8, 'must render intermediate frames')
  const first = frames[0], last = frames.at(-1)
  for (const [frame, expected] of [[first, from], [last, to]]) {
    for (const key of ['x', 'y', 'width', 'height']) assert.ok(Math.abs(frame[key] - expected[key]) < 9, `${key} should join the real form without a jump`)
  }
  assert.ok(frames.some(frame => Math.abs(frame.y - from.y) > 8 && Math.abs(frame.y - to.y) > 8), 'input must travel continuously')
  assert.ok(frames.some(frame => frame.radius > 9 && frame.radius < 21), 'corner radius must morph')
  assert.ok(frames.some(frame => frame.tray > .15 && frame.tray < .85), 'download tray must gradually dissolve')
  assert.ok(new Set(frames.map(frame => frame.shaft)).size > 5, 'arrow paths must interpolate')
  assert.ok(frames.every(frame => frame.clip.includes(`at ${origin.x}px ${origin.y}px`)), 'reveal must use actual pointer coordinates')
  assert.ok(Math.abs(first.button - last.button) > 30, 'action must continuously change width')
  const nextSelector=forward ? '.minimal-brand' : '.standard-mode .brand'
  for(const [key,before,after] of [['logo',logo,await rect(`${nextSelector} .brand-mark`)],['word',word,await rect(`${nextSelector} > span:last-child`)]]) {
    for(const field of ['x','y','width','height']) {
      assert.ok(Math.abs(before[field]-after[field])<.1,`${key} ${field} must be pixel aligned between modes`)
      assert.ok(frames.every(frame=>Math.abs(frame[key][field]-before[field])<.1),`${key} ${field} must remain fixed throughout the reveal`)
    }
  }
  assert.equal(await page.locator('.mode-switch-capture').count(), 0)
  assert.equal(await page.locator('.mode-morph-brand').count(), 0)
  assert.equal(await page.locator('.app-shell input:focus').count(), 1)
  if (screenshots) await page.screenshot({ path: path.join(output, forward ? 'minimal.png' : 'full.png') })
  return { frames: frames.length, from, to, origin,logoStationary:true,textBaselineStable:true }
}
try {
  await page.goto(process.env.SNOWDOWN_UI_URL || 'http://127.0.0.1:56969')
  await page.locator('.link-form').waitFor()
  await page.screenshot({ path: path.join(output, 'before.png') })
  const forward = await switchMode(true, true)
  const reverse = await switchMode(false, true)
  await page.locator('.app-shell .link-form input').fill('尚未解析的链接 · 保留输入')
  await switchMode(true)
  assert.equal(await page.locator('.app-shell .minimal-input-bar input').inputValue(), '尚未解析的链接 · 保留输入')
  await switchMode(false)
  assert.equal(await page.locator('.app-shell .link-form input').inputValue(), '尚未解析的链接 · 保留输入')
  await page.locator('.app-shell .link-form input').fill('')
  // Duplicate activation is ignored while the first transition owns its bridge.
  await page.locator('.app-shell .standard-mode .brand').click()
  await page.waitForSelector('.mode-morph-bar')
  await page.evaluate(() => document.querySelector('.app-shell .minimal-brand').click())
  await settle()
  assert.equal(await page.locator('.app-shell .minimal-mode').count(), 1)
  // Resizing snaps to the newly laid-out real form and removes temporary layers.
  await page.locator('.app-shell .minimal-brand').click()
  await page.waitForSelector('.mode-morph-bar')
  await page.setViewportSize({ width: 960, height: 660 })
  await settle()
  assert.equal(await page.locator('.mode-switch-capture').count(), 0)
  const minimum = await switchMode(true)
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.locator('.app-shell .minimal-brand').click()
  await settle()
  assert.equal(await page.locator('.mode-morph-bar').count(), 0)
  await page.locator('.app-shell .standard-mode .brand').focus()
  await page.keyboard.press('Enter')
  await settle()
  assert.equal(await page.locator('.app-shell .minimal-mode').count(), 1)
  assert.equal(await page.locator('.mode-switch-capture').count(), 0)
  assert.deepEqual(errors, [])
  const report = { success: true, forward, reverse, minimum, preservesInput: true, rapidClickGuard: true, resizeCleanup: true, reducedMotion: true, keyboardSwitch: true, errors }
  await writeFile(path.join(output, 'verification.json'), JSON.stringify(report, null, 2))
  console.log(JSON.stringify(report, null, 2))
} finally { await browser.close() }
