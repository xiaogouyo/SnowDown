<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { cursorPosition, getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { ArrowDown, GripHorizontal, Settings2, Snowflake } from '@lucide/vue'
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getClipboardStatus, getSnapshot, subscribe, subscribeClipboard } from '../api'
import { bytes } from '../format'
import type { AppSnapshot, ClipboardSnapshot } from '../types'

type Point = { x: number; y: number }
type Placement = Point & { docked: boolean }
type Rect = Point & { width: number; height: number }
type SettingsLayout = { source: Rect; studio: Rect }

const desktopWindow = getCurrentWindow()
const storageKey = 'snowdown.dynamicIsland'
const defaults = {
  width: 710, height: 42, font: 13, opacity: 92, frost: 55,
  position: null as Point | null,
  downloads: true, clock: true, usage: true, date: false,
  zones: ['local'],
}
const zones = ['local', 'UTC', ...(Intl.supportedValuesOf?.('timeZone') ?? ['Asia/Shanghai'])]
function loadPreferences(): typeof defaults {
  try {
    const saved = { ...defaults, ...JSON.parse(localStorage.getItem(storageKey) ?? '{}') }
    saved.width = Number.isInteger(saved.width) && saved.width >= 240 && saved.width <= 2400 ? saved.width : defaults.width
    saved.height = saved.height >= 32 && saved.height <= 120 ? saved.height : defaults.height
    if (!Number.isInteger(saved.height)) saved.height = defaults.height
    if (!Number.isInteger(saved.position?.x) || !Number.isInteger(saved.position?.y)) saved.position = null
    saved.zones = Array.isArray(saved.zones) ? saved.zones.filter((zone: string) => zones.includes(zone)) : []
    if (!saved.zones.length) saved.zones = defaults.zones
    return saved
  }
  catch { return { ...defaults } }
}
const prefs = reactive(loadPreferences())
const studio = ref<Rect>()
const settingsVisible = ref(false)
const preview = ref<HTMLElement>()
const settingsPanel = ref<HTMLElement>()
const barMotion = ref<'enter' | 'exit'>()
const tilt = ref({ x: 0, y: 0 })
let settingsRun = 0
let opening = false
let flight: Animation | undefined
const editing = computed(() => !!studio.value)
const studioStyle = computed(() => studio.value && ({
  left: studio.value.x + 'px', top: studio.value.y + 'px',
  width: studio.value.width + 'px', height: studio.value.height + 'px',
  '--preview-width': prefs.width * .78 + 'px',
  '--preview-height': Math.max(32, prefs.height * .78) + 'px',
  '--tilt-x': tilt.value.x + 'deg', '--tilt-y': tilt.value.y + 'deg',
}))
const docked = ref(true)
const moving = ref(false)
const resizing = ref(false)
let suppressRestoreUntil = 0
let cancelGesture: (() => void) | undefined
const state = ref<AppSnapshot>()
const capture = ref<ClipboardSnapshot['capture']>(null)
const usage = ref<{ cpu: number | null; gpu: number | null; memory: number | null }>()
const now = ref(new Date())
const zoneToAdd = ref('Asia/Shanghai')
const stops: (() => void)[] = []

const clockFormat = computed(() => prefs.zones.map(zone => new Intl.DateTimeFormat('zh-CN', {
  timeZone: zone === 'local' ? undefined : zone,
  hourCycle: 'h23', hour: '2-digit', minute: '2-digit', second: '2-digit',
  ...(prefs.date ? { year: 'numeric', month: '2-digit', day: '2-digit' } : {}),
})))
const active = computed(() => state.value?.tasks.filter(task => ['queued', 'downloading', 'merging'].includes(task.status)) ?? [])
const speed = computed(() => active.value.reduce((sum, task) => sum + task.speed, 0))
const linkState = computed(() => {
  switch (capture.value?.phase) {
    case 'parsing': return '正在解析链接'
    case 'ready': return '链接已解析'
    case 'error': return '链接解析失败'
    case 'detected': return '已识别链接'
    default: return '等待链接'
  }
})
const panelStyle = computed(() => ({
  fontSize: prefs.font + 'px',
  '--island-opacity': prefs.opacity / 100,
  '--island-frost': prefs.frost / 100,
}))
const percent = (value?: number | null) => value == null ? '—' : value + '%'
const restore = () => { if (performance.now() >= suppressRestoreUntil) void invoke('restore_from_island') }

async function place(width: number, height: number, position: Point | null, snap = false) {
  const result = await invoke<Placement>('resize_island', { width, height, position, snap })
  docked.value = result.docked
  return result
}

async function layout() {
  try {
    await place(prefs.width, prefs.height, prefs.position)
  } catch (error) {
    console.warn('灵动岛窗口位置调整失败', error)
  }
}
async function openSettings() {
  if (opening || editing.value || barMotion.value === 'exit' || moving.value || resizing.value) return
  opening = true
  const run = ++settingsRun
  const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches
  try {
    const position = await invoke<SettingsLayout>('layout_island_settings', { flight: !reduceMotion })
    if (run !== settingsRun) return
    studio.value = position.studio
    await nextTick()
    if (!reduceMotion && run === settingsRun && preview.value?.animate) {
      const target = preview.value.getBoundingClientRect()
      const from = position.source
      const duration = 380
      flight = preview.value.animate([
        { transform: 'translate(' + (from.x - target.x) + 'px,' + (from.y - target.y) + 'px) scale(' + from.width / target.width + ',' + from.height / target.height + ')' },
        { transform: 'none' },
      ], { duration, easing: 'cubic-bezier(.18,.76,.25,1)' })
      // Some WebViews suspend animation callbacks while a window is moving or hiding.
      let timeout: ReturnType<typeof setTimeout> | undefined
      await Promise.race([flight.finished.catch(() => {}), new Promise<void>(resolve => { timeout = setTimeout(resolve, duration + 60) })])
      clearTimeout(timeout)
    }
    if (run !== settingsRun) return
    if (!reduceMotion) {
      const settled = await invoke<SettingsLayout>('layout_island_settings', { flight: false })
      if (run !== settingsRun) return
      studio.value = settled.studio
    }
    settingsVisible.value = true
    await nextTick()
    settingsPanel.value?.focus({ preventScroll: true })
  } catch (error) {
    if (run === settingsRun) closeSettings()
    console.warn('灵动岛设置窗口定位失败', error)
  } finally {
    if (run === settingsRun) {
      opening = false
      flight = undefined
    }
  }
}

function closeSettings() {
  settingsRun++
  flight?.cancel()
  flight = undefined
  opening = false
  studio.value = undefined
  settingsVisible.value = false
  resetTilt()
  void layout()
}

function tiltPreview(event: PointerEvent) {
  if (!settingsVisible.value || event.pointerType !== 'mouse' || matchMedia('(prefers-reduced-motion: reduce)').matches) return
  const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const x = (event.clientX - bounds.left) / bounds.width - .5
  const y = (event.clientY - bounds.top) / bounds.height - .5
  tilt.value = { x: -y * 9, y: x * 11 }
}

function resetTilt() { tilt.value = { x: 0, y: 0 } }
function resetPosition() { prefs.position = null; closeSettings() }
function addZone() {
  if (!prefs.zones.includes(zoneToAdd.value) && prefs.zones.length < 5) prefs.zones.push(zoneToAdd.value)
}

function startGesture(event: PointerEvent, mode: 'move' | 'resize') {
  if (event.button !== 0 || cancelGesture || (mode === 'move' && (event.target as HTMLElement).closest('button'))) return
  const target = event.currentTarget as HTMLElement
  const pointerId = event.pointerId
  target.setPointerCapture(pointerId)
  const start = Promise.all([cursorPosition(), desktopWindow.outerPosition(), desktopWindow.scaleFactor()])
  const initialSize = { width: window.innerWidth, height: window.innerHeight }
  let position: Point | null = null
  let size = initialSize
  let frame = 0
  let pending: Promise<void> | undefined
  let changed = false
  let finishing = false

  function update() {
    frame = 0
    pending = (async () => {
      const [cursor, origin, scale] = await start
      const next = await cursorPosition()
      const dx = Math.round(next.x - cursor.x)
      const dy = Math.round(next.y - cursor.y)
      if (!changed && Math.hypot(dx, dy) < 4) return
      changed = true
      if (mode === 'move') {
        moving.value = true
        position = { x: origin.x + dx, y: origin.y + dy }
        await desktopWindow.setPosition(new PhysicalPosition(position.x, position.y))
      } else {
        resizing.value = true
        size = {
          width: Math.max(240, Math.min(2400, initialSize.width + Math.round(dx / scale))),
          height: Math.max(32, Math.min(120, initialSize.height + Math.round(dy / scale))),
        }
        position = origin
        await place(size.width, size.height, position)
      }
    })().catch(() => {}).finally(() => { pending = undefined })
  }

  function queue() {
    if (!finishing && !frame && !pending) frame = requestAnimationFrame(update)
  }

  async function finish() {
    if (finishing) return
    finishing = true
    if (frame) cancelAnimationFrame(frame)
    await pending
    update()
    await pending
    target.removeEventListener('pointermove', queue)
    target.removeEventListener('pointerup', finish)
    target.removeEventListener('lostpointercapture', finish)
    try {
      if (changed && position) {
        suppressRestoreUntil = performance.now() + 350
        const result = await place(size.width, size.height, position, mode === 'move')
        prefs.position = { x: result.x, y: result.y }
        if (mode === 'resize') Object.assign(prefs, size)
      }
    } catch {
      void layout()
    } finally {
      moving.value = false
      resizing.value = false
      cancelGesture = undefined
    }
  }

  cancelGesture = () => { void finish() }
  target.addEventListener('pointermove', queue)
  target.addEventListener('pointerup', finish, { once: true })
  target.addEventListener('lostpointercapture', finish, { once: true })
}

watch(prefs, value => localStorage.setItem(storageKey, JSON.stringify(value)))

onMounted(async () => {
  const timer = window.setInterval(() => { now.value = new Date() }, 1000)
  stops.push(() => window.clearInterval(timer))
  void layout()
  const subscriptions = await Promise.allSettled([
    listen<typeof usage.value>('snowdown://island-usage', event => { usage.value = event.payload }),
    listen('snowdown://island-shown', () => {
      now.value = new Date()
      usage.value = undefined
      closeSettings()
      barMotion.value = 'enter'
    }),
    listen('snowdown://island-hiding', () => {
      settingsRun++
      flight?.cancel()
      flight = undefined
      opening = false
      barMotion.value = 'exit'
    }),
    subscribe(value => { state.value = value }),
    subscribeClipboard(value => { capture.value = value.capture }),
  ])
  for (const subscription of subscriptions) {
    if (subscription.status === 'fulfilled') stops.push(subscription.value)
  }
  const [snapshot, clipboard] = await Promise.allSettled([getSnapshot(), getClipboardStatus()])
  if (snapshot.status === 'fulfilled') state.value ??= snapshot.value
  if (clipboard.status === 'fulfilled') capture.value ??= clipboard.value.capture
})
onUnmounted(() => { settingsRun++; flight?.cancel(); cancelGesture?.(); stops.forEach(stop => stop()) })
</script>

<template>
  <div class="island-window" :class="{ 'island-enter': barMotion === 'enter', 'island-exit': barMotion === 'exit' }">
    <div :class="editing ? 'island-studio' : 'island-home'" :style="studioStyle" @keydown.esc="editing && closeSettings()">
      <div ref="preview" :class="editing ? 'island-preview' : 'island-home'"
        @pointermove="editing && tiltPreview($event)" @pointerleave="resetTilt">
        <div class="island-shell" :class="{ docked: docked && !editing, moving, resizing }" :style="panelStyle"
          aria-label="SnowDown 灵动岛" @pointerdown="!editing && !opening && startGesture($event, 'move')"
          @contextmenu.prevent="openSettings" @dblclick="!editing && restore()">
          <div class="island-bar">
            <div class="island-leading">
              <Snowflake class="island-mark" :size="16" />
              <div v-if="prefs.downloads" class="island-download" :title="capture?.source.url">
                <span class="island-link">{{ linkState }}</span>
                <span class="island-rate"><ArrowDown :size="12" />{{ bytes(speed) }}/s</span>
                <span v-if="active.length" class="island-count">{{ active.length }} 项</span>
              </div>
            </div>
            <div v-if="prefs.clock" class="island-clock">
              <time v-for="(formatter, index) in clockFormat" :key="prefs.zones[index]" :title="prefs.zones[index]">{{ formatter.format(now) }}</time>
            </div>
            <div class="island-trailing">
              <div v-if="prefs.usage" class="island-usage">
                <span><small>CPU</small> {{ percent(usage?.cpu) }}</span>
                <span><small>GPU</small> {{ percent(usage?.gpu) }}</span>
                <span><small>内存</small> {{ percent(usage?.memory) }}</span>
              </div>
              <button class="island-options" aria-label="灵动岛设置" title="灵动岛设置" @click="openSettings" :disabled="editing" @dblclick.stop><Settings2 :size="15" /></button>
            </div>
            <GripHorizontal v-if="!editing" class="island-handle" :size="17" aria-hidden="true" />
          </div>
          <div v-if="!editing" class="island-resize" title="拖动调整宽高" @pointerdown.stop="startGesture($event, 'resize')" @dblclick.stop />
        </div>
      </div>
      <section v-if="settingsVisible" ref="settingsPanel" tabindex="-1" class="island-settings" aria-label="灵动岛设置" @contextmenu.prevent>
        <header><strong>灵动岛设置</strong><button @click="closeSettings">完成</button></header>
        <div class="island-sliders">
          <label>宽度 <output>{{ prefs.width }} px</output><input v-model.number="prefs.width" type="range" min="240" max="2400"></label>
          <label>高度 <output>{{ prefs.height }} px</output><input v-model.number="prefs.height" type="range" min="32" max="120"></label>
          <label>字体 <output>{{ prefs.font }} px</output><input v-model.number="prefs.font" type="range" min="9" max="32"></label>
          <label>不透明度 <output>{{ prefs.opacity }}%</output><input v-model.number="prefs.opacity" type="range" min="0" max="100"></label>
          <label>磨砂度 <output>{{ prefs.frost }}%</output><input v-model.number="prefs.frost" type="range" min="0" max="100"></label>
        </div>
        <fieldset><legend>显示内容</legend>
          <label><input v-model="prefs.downloads" type="checkbox">链接与下载</label>
          <label><input v-model="prefs.clock" type="checkbox">时钟</label>
          <label><input v-model="prefs.usage" type="checkbox">CPU / GPU / 内存</label>
        </fieldset>
        <fieldset v-if="prefs.clock"><legend>时钟设置</legend>
          <label><input v-model="prefs.date" type="checkbox">显示年月日</label>
          <div class="island-zones">
            <span v-for="(zone, index) in prefs.zones" :key="zone">
              {{ zone === 'local' ? '本地时间' : zone }}
              <button v-if="prefs.zones.length > 1" :aria-label="'移除 ' + zone" @click="prefs.zones.splice(index, 1)">×</button>
            </span>
          </div>
          <select v-model="zoneToAdd" aria-label="添加时区"><option v-for="zone in zones" :key="zone" :value="zone">{{ zone === 'local' ? '本地时间' : zone }}</option></select>
          <button :disabled="prefs.zones.length >= 5 || prefs.zones.includes(zoneToAdd)" @click="addZone">添加时区</button>
        </fieldset>
        <button class="island-reset" @click="resetPosition">回到屏幕顶部中央</button>
      </section>
    </div>
  </div>
</template>

<style>
html.island-page, .island-page body, .island-page #app { margin: 0; width: 100%; height: 100%; min-width: 0; min-height: 0; background: transparent; overflow: hidden; }
.island-shell, .island-settings { width: 100%; height: 100%; color: #edf3ff; box-sizing: border-box; }
.island-shell { position: relative; cursor: grab; touch-action: none; user-select: none; font-variant-numeric: tabular-nums; }
.island-shell.moving { cursor: grabbing; }
.island-bar {
  height: 100%; width: 100%; position: relative; display: grid; align-items: center;
  grid-template-columns: minmax(0, 1fr) fit-content(46%) minmax(0, 1fr);
  gap: .7em; padding: 0 1em; border-radius: 15px; overflow: hidden;
  background: linear-gradient(115deg, rgb(107 151 223 / calc(var(--island-frost) * .22)), transparent 52%),
    linear-gradient(155deg, rgb(31 42 62 / var(--island-opacity)), rgb(12 19 32 / var(--island-opacity)));
  border: 1px solid rgb(176 204 255 / calc(var(--island-opacity) * .26));
  box-shadow: inset 0 1px rgb(255 255 255 / calc(var(--island-frost) * .22));
  transition: border-radius 200ms ease, transform 200ms cubic-bezier(.2,.75,.3,1), border-color 160ms ease;
}
.island-shell.docked .island-bar { border-radius: 0 0 15px 15px; border-top-color: transparent; }
.island-shell.moving .island-bar { border-radius: 15px; transform: scale(.985); border-color: #a4c9ff91; }
.island-shell.resizing .island-bar { border-color: #a4c9ff91; }
.island-leading, .island-trailing, .island-download, .island-rate, .island-usage { display: flex; align-items: center; gap: .55em; min-width: 0; }
.island-leading { grid-column: 1; grid-row: 1; }
.island-trailing { grid-column: 3; grid-row: 1; justify-content: flex-end; }
.island-mark { color: #a9d0ff; flex: none; }
.island-download { overflow: hidden; white-space: nowrap; }
.island-link { color: #c6d3e8; overflow: hidden; text-overflow: ellipsis; }
.island-rate { color: #a9d0ff; flex: none; }
.island-count { color: #c6d3e8; }
.island-clock { grid-column: 2; grid-row: 1; min-width: 0; display: flex; gap: .5em; overflow: auto; scrollbar-width: none; white-space: nowrap; }
.island-clock time { flex: none; padding: .16em .55em; border-radius: 7px; background: #d4e2ff14; font-weight: 650; }
.island-usage { overflow: auto; scrollbar-width: none; white-space: nowrap; }
.island-usage span { flex: none; }
.island-usage small { font-size: .82em; color: #a6b9d5; }
.island-options { display: grid; place-items: center; flex: none; width: 24px; height: 24px; padding: 0; border-radius: 7px; color: #b4c9e9; background: transparent; }
.island-options:hover, .island-options:focus-visible { background: #d4e2ff24; color: white; }
.island-handle { pointer-events: none; position: absolute; top: -4px; left: 50%; translate: -50% 0; color: #c9dcf9; opacity: 0; transition: opacity 160ms; }
.island-shell:hover .island-handle, .island-shell.moving .island-handle { opacity: .6; }
.island-resize { position: absolute; right: 3px; bottom: 3px; width: 13px; height: 13px; cursor: nwse-resize; border-radius: 5px; touch-action: none; }
.island-resize::after { content: ''; position: absolute; right: 2px; bottom: 2px; width: 7px; height: 7px; border: solid #a9d0ff; border-width: 0 1px 1px 0; border-radius: 0 0 3px 0; opacity: 0; transition: opacity 160ms; }
.island-shell:hover .island-resize::after { opacity: .72; }
.island-settings { width: min(100% - 32px, 448px); height: auto; min-height: 0; max-height: 100%; padding: 20px 22px; background: #172236; border: 1px solid #344867; border-radius: 19px; overflow: auto; font-size: 13px; transform-origin: top center; animation: island-settings-unfold 240ms cubic-bezier(.2,.75,.28,1) both; }
.island-settings header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.island-settings header strong { font-size: 17px; }
.island-settings fieldset { margin: 8px 0 0; border: 1px solid #ffffff30; border-radius: 8px; padding: 7px; }
.island-settings label { margin-inline-end: 8px; }
.island-sliders label { display: grid; grid-template-columns: 64px 55px 1fr; align-items: center; min-height: 25px; }
.island-settings button, .island-settings select { background: #283b58; color: inherit; border: 1px solid #ffffff32; border-radius: 7px; padding: 4px 8px; }
.island-settings button:hover { background: #344d71; }
.island-zones { display: flex; flex-wrap: wrap; gap: 6px; margin: 5px 0; }
.island-settings select { max-width: 70%; margin-inline-end: 6px; }
.island-reset { margin-top: 10px; }
.island-window, .island-home { width: 100%; height: 100%; }
.island-window { overflow: hidden; }
.island-enter .island-home > .island-shell { animation: island-enter 230ms cubic-bezier(.2,.72,.24,1); }
.island-exit { animation: island-exit 180ms ease-in both; pointer-events: none; }
.island-studio {
  position: absolute; display: flex; flex-direction: column; align-items: center;
  gap: 18px; padding: 18px 0; box-sizing: border-box;
}
.island-preview {
  width: min(var(--preview-width), 100% - 32px); height: var(--preview-height);
  flex: none; transform-origin: top left;
}
.island-preview .island-shell {
  cursor: default; transform: perspective(850px) rotateX(var(--tilt-x)) rotateY(var(--tilt-y));
  transition: transform 170ms ease-out;
}
.island-preview .island-options { opacity: .72; }
.island-settings:focus { outline: none; }
@keyframes island-enter { from { transform: translateY(-100%); } }
@keyframes island-exit { to { transform: translateY(100%); opacity: 0; } }
@keyframes island-settings-unfold {
  from { transform: translateY(-14px) scale(.96); clip-path: inset(0 0 86% round 19px); opacity: .45; }
  to { transform: none; clip-path: inset(0 round 19px); opacity: 1; }
}
@media (prefers-reduced-motion: reduce) {
  .island-enter .island-home > .island-shell, .island-exit, .island-settings { animation: none; }
  .island-preview .island-shell { transition: none; }
  .island-bar, .island-handle, .island-resize::after { transition: none; }
}
</style>
