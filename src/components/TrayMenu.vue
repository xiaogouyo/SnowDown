<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { ArrowUpRight, FolderOpen, LogOut, Snowflake } from '@lucide/vue'
import * as api from '../api'
import { errorMessage } from '../format'
import type { AppSnapshot } from '../types'

const snapshot = ref<AppSnapshot | null>(null)
const root = ref<HTMLElement>()
const busy = ref(false)
const error = ref('')
const footer = computed(() => snapshot.value?.settings.clipboardEnabled ? snapshot.value.settings.clipboardAutoParse ? '后台运行 · 自动解析已开启' : '后台运行 · 剪贴板监听中' : '后台运行 · 剪贴板识别已关闭')
let disposed = false
let unsubscribe: (() => void) | undefined
let unwatchFocus: (() => void) | undefined

function focusFirst() { root.value?.querySelector<HTMLButtonElement>('button')?.focus() }
async function action(value: 'show' | 'folder' | 'quit' | 'dismiss') {
  if (busy.value) return
  busy.value = true; error.value = ''
  try { await api.trayAction(value) }
  catch (cause) { error.value = errorMessage(cause) }
  finally { busy.value = false }
}
function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); void action('dismiss'); return }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const items = [...(root.value?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])]
  const index = items.indexOf(document.activeElement as HTMLButtonElement)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length
  items[next]?.focus()
}
onMounted(async () => {
  try {
    const stop = await api.subscribe(value => { if (!disposed) snapshot.value = value })
    if (disposed) { stop(); return }; unsubscribe = stop
    const initial = await api.getSnapshot()
    if (disposed) return
    snapshot.value = initial
    const stopFocus = await api.watchFocus(focused => { if (focused && !disposed) focusFirst() })
    if (disposed) { stopFocus(); return }; unwatchFocus = stopFocus
    focusFirst()
  } catch (cause) { if (!disposed) error.value = errorMessage(cause) }
})
onUnmounted(() => { disposed = true; unsubscribe?.(); unwatchFocus?.() })
</script>

<template>
  <main ref="root" class="tray-shell" @keydown="onKey" @contextmenu.prevent>
    <section class="tray-card" aria-label="SnowDown 托盘菜单">
      <header class="tray-brand"><span class="brand-mark"><Snowflake :size="18" /></span><div><strong>SnowDown<span>.</span></strong><small>好内容，慢慢收藏。</small></div><span class="tray-version">v{{ snapshot?.version ?? '0.1.8' }}</span></header>
      <nav role="menu" aria-label="托盘操作">
        <button role="menuitem" :disabled="busy" @click="action('show')"><ArrowUpRight :size="17" /><span>打开 SnowDown</span></button>
        <button role="menuitem" :disabled="busy" @click="action('folder')"><FolderOpen :size="17" /><span>打开下载目录</span></button>
        <button role="menuitem" class="tray-quit" :disabled="busy" @click="action('quit')"><LogOut :size="17" /><span>退出 SnowDown</span></button>
      </nav>
      <footer :class="{ error: error }" role="status"><span class="tray-status-dot"></span>{{ error || footer }}</footer>
    </section>
  </main>
</template>

<style scoped>
.tray-shell{padding:10px;width:100vw;height:100vh;overflow:hidden}.tray-card{height:100%;padding:12px 10px 0;border:1px solid #e1eaf6;border-radius:13px;background:#fff;box-shadow:0 3px 10px #223a5921;display:flex;flex-direction:column}.tray-brand{display:flex;align-items:center;gap:9px;padding:1px 4px 13px}.tray-brand .brand-mark{width:29px;height:31px;border-radius:9px;flex-shrink:0}.tray-brand>div{display:flex;flex:1;flex-direction:column;gap:4px}.tray-brand strong{font-size:14px;letter-spacing:-.4px;color:#29384d}.tray-brand strong span{color:var(--blue)}.tray-brand small{font-size:9px;color:#9baac0}.tray-version{color:#95a7c0;font-size:9px;align-self:flex-start;padding-top:3px}.tray-card nav{display:flex;flex-direction:column;gap:3px}.tray-card button{display:flex;align-items:center;gap:11px;min-height:36px;padding:0 11px;border-radius:7px;background:transparent;text-align:left;color:#788da9;font-size:12px;transition:color .12s,background .12s}.tray-card button:hover,.tray-card button:focus-visible{background:#edf4ff;color:#3377ed;outline:none}.tray-card .tray-quit:hover,.tray-card .tray-quit:focus-visible{background:#fff4f3;color:#bc7b78}.tray-card footer{display:flex;align-items:center;gap:6px;min-height:32px;margin-top:auto;border-top:1px solid #edf1f7;color:#9aaac0;font-size:9px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;padding:0 4px}.tray-status-dot{width:5px;height:5px;flex-shrink:0;border-radius:50%;background:#7ab69c}.tray-card footer.error{color:#bd7d77}.tray-card footer.error .tray-status-dot{background:#bd7d77}
</style>
