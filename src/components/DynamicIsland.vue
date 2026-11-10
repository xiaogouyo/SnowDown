<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getClipboardStatus, getSnapshot, subscribe, subscribeClipboard } from '../api'
import { bytes } from '../format'
import type { AppSnapshot, ClipboardSnapshot } from '../types'

const storageKey = 'snowdown.dynamicIsland'
const defaults = {
  width: 710, height: 48, font: 13, opacity: 68, frost: 65,
  downloads: true, clock: true, usage: true, date: false,
  zones: ['local'],
}
function loadPreferences(): typeof defaults {
  try { return { ...defaults, ...JSON.parse(localStorage.getItem(storageKey) ?? '{}') } }
  catch { return { ...defaults } }
}
const prefs = reactive(loadPreferences())
const editing = ref(false)
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
  backgroundColor: `rgb(18 25 40 / ${prefs.opacity / 100})`,
  boxShadow: `inset 0 0 22px rgb(204 225 255 / ${prefs.frost / 550})`,
}))
const percent = (value?: number | null) => value == null ? '—' : value + '%'
const restore = () => invoke('restore_from_island')

function resize() {
  return invoke('resize_island', {
    width: editing.value ? 460 : prefs.width,
    height: editing.value ? 410 : prefs.height,
  })
}
function openSettings() { editing.value = true; void resize() }
function closeSettings() { editing.value = false; void resize() }
function addZone() {
  if (!prefs.zones.includes(zoneToAdd.value) && prefs.zones.length < 5) prefs.zones.push(zoneToAdd.value)
}

function startResize(event: PointerEvent, axis: 'width' | 'height') {
  if (event.button !== 0) return
  const handle = event.currentTarget as HTMLElement
  handle.setPointerCapture(event.pointerId)
  handle.addEventListener('pointerup', end, { once: true })
  handle.addEventListener('lostpointercapture', cancel, { once: true })
  function cancel() { handle.removeEventListener('pointerup', end) }
  function end(up: PointerEvent) {
    if (axis === 'width') {
      prefs.width = Math.max(180, Math.min(2400, window.innerWidth + 2 * (up.screenX - event.screenX)))
    } else {
      prefs.height = Math.max(26, Math.min(600, window.innerHeight + up.screenY - event.screenY))
    }
    void resize()
  }
}

watch(prefs, value => localStorage.setItem(storageKey, JSON.stringify(value)))
watch(() => prefs.frost > 0, enabled => { void invoke('set_island_frost', { enabled }).catch(() => {}) }, { immediate: true })

onMounted(async () => {
  const timer = window.setInterval(() => { now.value = new Date() }, 1000)
  stops.push(() => window.clearInterval(timer))
  stops.push(await listen<typeof usage.value>('snowdown://island-usage', event => { usage.value = event.payload }))
  stops.push(await listen('snowdown://island-shown', () => {
    now.value = new Date()
    usage.value = undefined
    closeSettings()
  }))
  stops.push(await subscribe(value => { state.value = value }))
  stops.push(await subscribeClipboard(value => { capture.value = value.capture }))
  state.value ??= await getSnapshot()
  capture.value ??= (await getClipboardStatus()).capture
  await resize()
})
onUnmounted(() => stops.forEach(stop => stop()))
</script>

<template>
  <section v-if="editing" class="island-settings" aria-label="灵动岛设置" @contextmenu.prevent @keydown.esc="closeSettings">
    <header><strong>灵动岛设置</strong><button @click="closeSettings">完成</button></header>
    <div class="island-sliders">
      <label>宽度 <output>{{ prefs.width }} px</output><input v-model.number="prefs.width" type="range" min="180" max="2400"></label>
      <label>高度 <output>{{ prefs.height }} px</output><input v-model.number="prefs.height" type="range" min="26" max="600"></label>
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
  </section>
  <section v-else class="island-bar" :style="panelStyle" aria-label="SnowDown 灵动岛" @contextmenu.prevent="openSettings" @dblclick="restore">
    <div v-if="prefs.downloads" class="island-download" :title="capture?.source.url">{{ linkState }} · {{ active.length }} 项 · {{ bytes(speed) }}/s</div>
    <div v-if="prefs.clock" class="island-clock">
      <time v-for="(formatter, index) in clockFormat" :key="prefs.zones[index]" :title="prefs.zones[index]">{{ formatter.format(now) }}</time>
    </div>
    <div v-if="prefs.usage" class="island-usage">CPU {{ percent(usage?.cpu) }} · GPU {{ percent(usage?.gpu) }} · 内存 {{ percent(usage?.memory) }}</div>
    <button v-if="!prefs.downloads && !prefs.clock && !prefs.usage" class="island-empty" @click="openSettings">SnowDown</button>
    <div class="island-grip island-grip-width" title="拖动调整宽度" @pointerdown="startResize($event, 'width')" @dblclick.stop />
    <div class="island-grip island-grip-height" title="拖动调整高度" @pointerdown="startResize($event, 'height')" @dblclick.stop />
  </section>
</template>

<style>
html.island-page, .island-page body, .island-page #app { margin: 0; width: 100%; height: 100%; min-width: 0; min-height: 0; background: transparent; overflow: hidden; }
.island-bar, .island-settings { width: 100%; height: 100%; border-radius: 0 0 16px 16px; color: #f4f7ff; }
.island-bar { position: relative; display: flex; align-items: center; gap: .85em; padding: 2px 15px 5px; user-select: none; font-variant-numeric: tabular-nums; text-shadow: 0 1px 3px #000; }
.island-bar > :is(.island-download, .island-clock, .island-usage) { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.island-clock { text-align: center; }
.island-clock time + time { margin-inline-start: .8em; }
.island-usage { text-align: right; }
.island-grip { position: absolute; touch-action: none; }
.island-grip-width { inset: 0 0 10px auto; width: 9px; cursor: ew-resize; }
.island-grip-height { inset: auto 12px 0; height: 7px; cursor: ns-resize; }
.island-grip-height:hover { background: #ffffff36; }
.island-empty { margin: auto; }
.island-settings { padding: 13px 16px; background: #182134; overflow: auto; font-size: 13px; }
.island-settings header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
.island-settings fieldset { margin: 8px 0 0; border: 1px solid #ffffff38; border-radius: 8px; padding: 6px; }
.island-settings label { margin-inline-end: 8px; }
.island-sliders label { display: grid; grid-template-columns: 64px 55px 1fr; align-items: center; min-height: 25px; }
.island-settings button, .island-settings select, .island-empty { background: #293852; color: inherit; border: 1px solid #ffffff44; border-radius: 6px; padding: 3px 7px; }
.island-zones { display: flex; flex-wrap: wrap; gap: 6px; margin: 5px 0; }
.island-settings select { max-width: 70%; margin-inline-end: 6px; }
</style>
