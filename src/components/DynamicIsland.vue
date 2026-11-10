<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { cursorPosition, getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { ArrowDown, GripHorizontal, Settings2, Snowflake } from '@lucide/vue'
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getClipboardStatus, getSnapshot, subscribe, subscribeClipboard } from '../api'
import { bytes } from '../format'
import type { AppSnapshot, ClipboardSnapshot } from '../types'

type Point = { x: number; y: number }
type Placement = Point & { docked: boolean }

const desktopWindow = getCurrentWindow()
const storageKey = 'snowdown.dynamicIsland'
const defaults = {
  width: 710, height: 42, font: 13, opacity: 92, frost: 55,
  position: null as Point | null,
  downloads: true, clock: true, usage: true, date: false,
  zones: ['local'],
}
function loadPreferences(): typeof defaults {
  try {
    const saved = { ...defaults, ...JSON.parse(localStorage.getItem(storageKey) ?? '{}') }
    saved.height = saved.height >= 32 && saved.height <= 120 ? saved.height : defaults.height
    return saved
  }
  catch { return { ...defaults } }
}
const prefs = reactive(loadPreferences())
const editing = ref(false)
const docked = ref(true)
const shown = ref(false)
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

const zones = ['local', ...Intl.supportedValuesOf('timeZone')]
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
  await place(editing.value ? 436 : prefs.width, editing.value ? 400 : prefs.height, prefs.position)
}
function openSettings() { editing.value = true; void layout() }
function closeSettings() { editing.value = false; void layout() }
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
  stops.push(await listen<typeof usage.value>('snowdown://island-usage', event => { usage.value = event.payload }))
  stops.push(await listen('snowdown://island-shown', () => {
    now.value = new Date()
    usage.value = undefined
    editing.value = false
    shown.value = false
    void layout().then(() => requestAnimationFrame(() => { shown.value = true }))
  }))
  stops.push(await subscribe(value => { state.value = value }))
  stops.push(await subscribeClipboard(value => { capture.value = value.capture }))
  state.value ??= await getSnapshot()
  capture.value ??= (await getClipboardStatus()).capture
  await layout()
  shown.value = true
})
onUnmounted(() => { cancelGesture?.(); stops.forEach(stop => stop()) })
</script>

<template>
  <section v-if="editing" class="island-settings" aria-label="灵动岛设置" @contextmenu.prevent @keydown.esc="closeSettings">
    <header><div><small>SNOWDOWN</small><strong>灵动岛设置</strong></div><button @click="closeSettings">完成</button></header>
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
  <section v-else class="island-shell" :class="{ docked, moving, resizing, shown }" :style="panelStyle"
    aria-label="SnowDown 灵动岛" @pointerdown="startGesture($event, 'move')" @contextmenu.prevent="openSettings" @dblclick="restore">
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
        <button class="island-options" aria-label="灵动岛设置" title="灵动岛设置" @click="openSettings" @dblclick.stop><Settings2 :size="15" /></button>
      </div>
      <GripHorizontal class="island-handle" :size="17" aria-hidden="true" />
    </div>
    <div class="island-resize" title="拖动调整宽高" @pointerdown.stop="startGesture($event, 'resize')" @dblclick.stop />
  </section>
</template>

<style>
html.island-page, .island-page body, .island-page #app { margin: 0; width: 100%; height: 100%; min-width: 0; min-height: 0; background: transparent; overflow: hidden; }
.island-shell, .island-settings { width: 100%; height: 100%; color: #edf3ff; box-sizing: border-box; }
.island-shell { position: relative; cursor: grab; touch-action: none; user-select: none; font-variant-numeric: tabular-nums; }
.island-shell.moving { cursor: grabbing; }
.island-bar {
  height: 100%; width: 100%; position: relative; display: flex; align-items: center; justify-content: space-between;
  gap: .7em; padding: 0 1em; border-radius: 15px; overflow: hidden;
  background: linear-gradient(115deg, rgb(107 151 223 / calc(var(--island-frost) * .22)), transparent 52%),
    linear-gradient(155deg, rgb(31 42 62 / var(--island-opacity)), rgb(12 19 32 / var(--island-opacity)));
  border: 1px solid rgb(176 204 255 / calc(var(--island-opacity) * .26));
  box-shadow: inset 0 1px rgb(255 255 255 / calc(var(--island-frost) * .22));
  opacity: 0; transform: translateY(-4px) scale(.985);
  transition: border-radius 200ms ease, transform 200ms cubic-bezier(.2,.75,.3,1), opacity 160ms ease, border-color 160ms ease;
}
.island-shell.shown .island-bar { opacity: 1; transform: none; }
.island-shell.docked .island-bar { border-radius: 0 0 15px 15px; border-top-color: transparent; }
.island-shell.moving .island-bar { border-radius: 15px; transform: scale(.985); border-color: #a4c9ff91; }
.island-shell.resizing .island-bar { border-color: #a4c9ff91; }
.island-leading, .island-trailing, .island-download, .island-rate, .island-usage { display: flex; align-items: center; gap: .55em; min-width: 0; }
.island-leading, .island-trailing { flex: 1 1 auto; }
.island-trailing { justify-content: flex-end; }
.island-mark { color: #a9d0ff; flex: none; }
.island-download { overflow: hidden; white-space: nowrap; }
.island-link { color: #c6d3e8; overflow: hidden; text-overflow: ellipsis; }
.island-rate { color: #a9d0ff; flex: none; }
.island-count { color: #c6d3e8; }
.island-clock { flex: 0 1 auto; min-width: 0; display: flex; gap: .5em; overflow: auto; scrollbar-width: none; white-space: nowrap; }
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
.island-settings { padding: 15px 18px; background: #172236; border: 1px solid #344867; border-radius: 15px; overflow: auto; font-size: 13px; }
.island-settings header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.island-settings header div { display: grid; gap: 2px; }
.island-settings header small { font-size: 9px; letter-spacing: .12em; color: #a9c6eb; }
.island-settings header strong { font-size: 17px; }
.island-settings fieldset { margin: 8px 0 0; border: 1px solid #ffffff30; border-radius: 8px; padding: 7px; }
.island-settings label { margin-inline-end: 8px; }
.island-sliders label { display: grid; grid-template-columns: 64px 55px 1fr; align-items: center; min-height: 25px; }
.island-settings button, .island-settings select { background: #283b58; color: inherit; border: 1px solid #ffffff32; border-radius: 7px; padding: 4px 8px; }
.island-settings button:hover { background: #344d71; }
.island-zones { display: flex; flex-wrap: wrap; gap: 6px; margin: 5px 0; }
.island-settings select { max-width: 70%; margin-inline-end: 6px; }
.island-reset { margin-top: 10px; }
@media (prefers-reduced-motion: reduce) {
  .island-bar, .island-handle, .island-resize::after { transition: none; }
}
</style>
