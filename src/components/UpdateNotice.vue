<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { Download, LoaderCircle, ShieldCheck, Snowflake, X } from '@lucide/vue'
import * as api from '../api'
import { errorMessage } from '../format'
import type { UpdateStatus } from '../types'
const status = ref<UpdateStatus | null>(null)
const busy = ref(false)
const error = ref('')
let disposed = false
let stop: (() => void) | undefined
function accept(value: UpdateStatus) {
  if ((value.revision ?? 0) < (status.value?.revision ?? 0)) return
  status.value = value
  if (value.phase === 'ready') busy.value = false
}
async function dismiss() { if (!busy.value) { try { await api.dismissUpdateNotice() } catch (cause) { error.value = errorMessage(cause) } } }
async function install() {
  if (busy.value || status.value?.phase !== 'ready') return
  busy.value = true; error.value = ''
  try { await api.installUpdate() }
  catch (cause) { busy.value = false; error.value = errorMessage(cause) }
}
onMounted(async () => {
  try {
    const unsubscribe = await api.subscribeUpdates(value => { if (!disposed) accept(value) })
    if (disposed) { unsubscribe(); return }; stop = unsubscribe
    const initial = await api.getUpdateStatus()
    if (!disposed) accept(initial)
  } catch (cause) { if (!disposed) error.value = errorMessage(cause) }
})
onUnmounted(() => { disposed = true; stop?.() })
</script>
<template>
  <main class="notice-shell" @keydown.esc="dismiss">
    <section class="notice-card" aria-label="SnowDown 更新已就绪" role="status">
      <header><span class="brand-mark"><Snowflake :size="22" /></span><div><strong>更新已准备好</strong><small>SnowDown v{{ status?.release?.version }}</small></div><button class="icon-button" aria-label="稍后更新" :disabled="busy" @click="dismiss"><X :size="17" /></button></header>
      <p>点击安装后自动覆盖并重新启动，<br />账号登录、下载内容和设置完整保留。</p>
      <footer><span><ShieldCheck :size="13" />已校验安装包</span><button class="button primary" :disabled="busy || status?.phase !== 'ready'" @click="install"><LoaderCircle v-if="busy" :size="15" class="spin" /><Download v-else :size="15" />{{ busy ? '正在准备安装' : '安装更新' }}</button></footer>
      <p v-if="error" class="notice-error" role="alert">{{ error }}</p>
    </section>
  </main>
</template>
<style scoped>
.notice-shell{padding:10px;width:100vw;height:100vh;overflow:hidden}.notice-card{height:100%;background:white;border:1px solid #dfe9f7;border-radius:15px;padding:18px 19px;box-shadow:0 3px 10px #223a5921;display:flex;flex-direction:column;animation:notice-in .25s ease-out}.notice-card header{display:flex;align-items:center;gap:12px}.brand-mark{width:37px;height:39px;border-radius:11px;flex-shrink:0}.notice-card header>div{flex:1}.notice-card strong{font-size:14px;color:#344b6a;font-weight:600}.notice-card small{display:block;font-size:10px;color:#95a7c0;margin-top:4px}.notice-card p{font-size:11px;line-height:1.9;color:#889db9;margin:14px 0}.notice-card footer{display:flex;justify-content:space-between;align-items:center;margin-top:auto;gap:10px}.notice-card footer>span{display:flex;gap:5px;align-items:center;font-size:10px;color:#82ac9b}.notice-card .button{padding:9px 13px;font-size:11px;white-space:nowrap}.notice-card .notice-error{color:#bd7d77;font-size:10px;line-height:1.4;margin:7px 0 0;overflow:auto;max-height:28px}@keyframes notice-in{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:translateY(0)}}
</style>
