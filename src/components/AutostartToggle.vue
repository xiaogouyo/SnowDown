<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Power } from '@lucide/vue'
import * as api from '../api'
import { errorMessage } from '../format'
const enabled = ref(false), blocked = ref(false), busy = ref(true), ready = ref(false), error = ref('')
onMounted(async () => {
  try { const status = await api.autostartStatus(); enabled.value = status.enabled; blocked.value = status.blocked; ready.value = true }
  catch (e) { error.value = errorMessage(e) }
  finally { busy.value = false }
})
async function toggle() {
  if (busy.value || !ready.value) return
  busy.value = true; error.value = ''
  try { const status = await api.setAutostart(!enabled.value); enabled.value = status.enabled; blocked.value = status.blocked }
  catch (e) { error.value = errorMessage(e) }
  finally { busy.value = false }
}
</script>
<template>
  <div class="autostart-setting">
    <label class="toggle-row"><span><Power :size="17" /><span>开机自启动<small>{{ blocked ? '已被 Windows 禁用，开启此开关可重新启用' : '登录 Windows 后在托盘后台运行，双击托盘图标打开窗口；立即生效' }}</small></span></span><input type="checkbox" role="switch" aria-label="开机自启动" :checked="enabled" :disabled="busy || !ready" @click.prevent="toggle" /></label>
    <p v-if="error" class="task-error" role="alert">{{ error }}</p>
  </div>
</template>
