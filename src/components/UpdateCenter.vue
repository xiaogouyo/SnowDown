<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ArrowUpRight, CircleAlert, CircleCheck, Download, LoaderCircle, RefreshCw, ShieldCheck, Sparkles, X } from '@lucide/vue'
import * as api from '../api'
import { bytes, errorMessage } from '../format'
import type { UpdateMirror, UpdateStatus } from '../types'

const props = defineProps<{ currentVersion: string; canPrompt: boolean; activeDownloads: number }>()
const emit = defineEmits<{ visibility: [visible: boolean] }>()
const status = ref<UpdateStatus | null>(null)
const visible = ref(false)
const checking = ref(false)
const starting = ref(false)
const installing = ref(false)
const cancelling = ref(false)
const error = ref('')
const checked = ref(false)
const pendingAuto = ref(false)
const trigger = ref<HTMLButtonElement>()
const panel = ref<HTMLElement>()
const closeButton = ref<HTMLButtonElement>()
let unsubscribe: (() => void) | undefined
let startup: ReturnType<typeof setTimeout> | undefined
let periodic: ReturnType<typeof setInterval> | undefined
let disposed = false
let promptedVersion = ''

const release = computed(() => status.value?.release)
const probing = computed(() => status.value?.phase === 'probing')
const downloading = computed(() => probing.value || status.value?.phase === 'downloading' || starting.value)
const mirrors = computed(() => status.value?.mirrors ?? [])
const completed = computed(() => mirrors.value.filter(mirror => !['pending', 'testing'].includes(mirror.state)).length)
const mirrorProgress = computed(() => mirrors.value.length ? completed.value / mirrors.value.length * 100 : 0)
const mirrorLabels: Record<UpdateMirror['state'], string> = { pending: '待检测', testing: '检测中', available: '可用', timeout: '超时', unsupported: '不支持', unavailable: '不可用', failed: '下载失败', cancelled: '已停止' }
const ready = computed(() => status.value?.phase === 'ready')
const progress = computed(() => status.value?.total ? Math.min(100, status.value.downloaded / status.value.total * 100) : 0)
const heading = computed(() => checking.value ? '正在检查新版本' : probing.value ? '正在选择更新镜像' : downloading.value ? '正在下载更新' : ready.value ? '更新已准备好' : release.value ? '发现新版本' : '版本与更新')

watch(visible, async value => {
  emit('visibility', value)
  await nextTick()
  if (value) closeButton.value?.focus()
  else trigger.value?.focus()
}, { flush: 'sync' })
watch([pendingAuto, () => props.canPrompt, visible], () => {
  if (pendingAuto.value && props.canPrompt && !visible.value) {
    pendingAuto.value = false
    promptedVersion = release.value?.version ?? ''
    visible.value = true
  }
})

async function check(manual = true) {
  if (checking.value || downloading.value || installing.value) return
  if (!api.desktop) { if (manual) error.value = '请在 Windows 客户端中检查更新。'; return }
  checking.value = true
  if (manual) error.value = ''
  try {
    const result = await api.checkUpdate()
    if (disposed) return
    status.value = result; checked.value = true; error.value = ''
    if (!manual && result.release && !result.deferred && result.release.version !== promptedVersion) pendingAuto.value = true
  } catch (cause) { if (manual && !disposed) error.value = errorMessage(cause) }
  finally { checking.value = false }
}

function open() { visible.value = true; pendingAuto.value = false; void check(true) }
defineExpose({ open })

async function close() {
  if (installing.value) return
  if (release.value && !downloading.value) {
    try { status.value = await api.deferUpdate() }
    catch (cause) { error.value = errorMessage(cause); return }
  }
  pendingAuto.value = false; visible.value = false
}

async function download() {
  if (downloading.value || checking.value) return
  error.value = ''; starting.value = true
  try { const result = await api.downloadUpdate(); if (!disposed) status.value = result }
  catch (cause) { if (!disposed) error.value = errorMessage(cause) }
  finally { starting.value = false; cancelling.value = false }
}

async function cancel() {
  cancelling.value = true
  try { await api.cancelUpdateDownload() }
  catch (cause) { error.value = errorMessage(cause); cancelling.value = false }
}

async function install() {
  if (!ready.value || props.activeDownloads || installing.value) return
  error.value = ''; installing.value = true
  try { await api.installUpdate() }
  catch (cause) { error.value = errorMessage(cause); installing.value = false }
}

async function openRelease() { try { await api.openUpdateRelease() } catch (cause) { error.value = errorMessage(cause) } }

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); void close() }
  if (event.key !== 'Tab') return
  const buttons = [...(panel.value?.querySelectorAll<HTMLElement>('button:not(:disabled), summary') ?? [])].filter(button => button.getClientRects().length)
  const first = buttons[0], last = buttons.at(-1)
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
}

onMounted(async () => {
  if (!api.desktop) return
  try {
    const unlisten = await api.subscribeUpdates(value => { if (!disposed) status.value = value })
    if (disposed) { unlisten(); return }
    unsubscribe = unlisten
    const initial = await api.getUpdateStatus()
    if (disposed) return
    status.value = initial
  } catch { /* Manual check can recover if initial subscription or status failed. */ }
  if (disposed) return
  startup = setTimeout(() => { void check(false) }, 2500)
  periodic = setInterval(() => { void check(false) }, 6 * 60 * 60 * 1000)
})
onUnmounted(() => { disposed = true; unsubscribe?.(); clearTimeout(startup); clearInterval(periodic) })
</script>

<template>
  <button ref="trigger" :class="['version-button', { available: release }]" aria-label="版本与检查更新" :title="release ? `发现新版 v${release.version}，点击更新` : '版本与检查更新'" @click="open"><span>v{{ currentVersion }}</span><span v-if="release" class="version-dot"></span></button>
  <Teleport to="body">
    <div v-if="visible" class="modal-overlay update-overlay" @mousedown.self="close" @keydown="onKey">
      <section ref="panel" class="modal update-modal" role="dialog" aria-modal="true" aria-labelledby="update-title">
        <div class="modal-header"><div><span class="eyebrow">KEEP SNOWDOWN FRESH</span><h2 id="update-title">{{ heading }}</h2></div><button ref="closeButton" class="icon-button" aria-label="关闭版本更新" :disabled="installing" @click="close"><X :size="20" /></button></div>
        <div class="update-body">
          <div class="update-summary"><span class="update-symbol"><LoaderCircle v-if="checking || downloading || installing" class="spin" :size="27" /><Sparkles v-else-if="release" :size="27" /><CircleCheck v-else :size="27" /></span><div><strong>{{ release ? `SnowDown v${release.version}` : `SnowDown v${currentVersion}` }}</strong><p>{{ release ? `当前版本 v${currentVersion} · Windows x64` : checking ? '正在连接 GitHub 获取正式版本信息…' : checked && !error ? '当前已是最新正式版本' : '从 GitHub 获取最新正式版本' }}</p></div></div>
          <template v-if="release">
            <div class="update-notes"><div><strong>更新说明</strong><span>{{ release.publishedAt.slice(0, 10) }}</span></div><p>{{ release.notes || release.title || '修复与体验改进。' }}</p></div>
            <div v-if="downloading || ready" class="update-transfer"><div><span>{{ probing ? '检测可用镜像与首字节延迟' : ready ? '下载完成 · 校验通过' : '正在下载更新包' }}</span><strong>{{ probing ? `${completed} / ${mirrors.length} 个节点` : `${bytes(status?.downloaded ?? 0)} / ${bytes(status?.total ?? release.size)}` }}</strong></div><div class="update-progress" role="progressbar" :aria-label="probing ? '镜像检测进度' : '更新下载进度'" :aria-valuenow="Math.round(probing ? mirrorProgress : progress)" aria-valuemin="0" aria-valuemax="100"><span :style="{ width: `${probing ? mirrorProgress : progress}%` }"></span></div></div>
            <p v-if="status?.downloadSource" class="update-source">下载源：<strong>{{ status.downloadSource }}</strong><span v-if="status.latencyMs !== null"> · {{ status.latencyMs }} ms</span></p>
            <details v-if="mirrors.length" class="update-mirrors"><summary>镜像库 <span>{{ mirrors.length }} 个节点 · {{ completed ? `已检测 ${completed}` : '下载前自动检测' }}</span></summary><p>按本次实测延迟选择可用节点；下载失败依次切换，最后尝试 GitHub 官方直连。</p><div class="mirror-table-wrap"><table><thead><tr><th>ID</th><th>镜像</th><th>延迟</th><th>状态</th></tr></thead><tbody><tr v-for="mirror in mirrors" :key="mirror.id" :class="{ selected: status?.downloadSource === mirror.host }" :title="mirror.message ?? undefined"><td>{{ mirror.id }}</td><td>{{ mirror.host }}</td><td>{{ mirror.latencyMs === null ? '—' : `${mirror.latencyMs} ms` }}</td><td :class="mirror.state">{{ mirrorLabels[mirror.state] }}</td></tr></tbody></table></div></details>
            <p class="update-hint"><ShieldCheck :size="14" /><span>{{ ready ? '点击下方按钮退出 SnowDown 并打开安装器，确认安装目录后继续。' : `更新包 ${bytes(release.size)}，下载完成并校验后可打开安装器。` }}</span></p>
            <p v-if="ready && activeDownloads" class="update-busy" role="status">当前有 {{ activeDownloads }} 个任务等待或正在下载，请先暂停或完成，再安装更新。</p>
            <p class="update-portable">免安装版也可在 GitHub 下载 ZIP，退出后解压覆盖原文件夹。</p>
          </template>
          <p v-if="error || status?.error" class="update-error" role="alert"><CircleAlert :size="15" />{{ error || status?.error }}</p>
          <div class="update-links"><button class="text-button" @click="openRelease">GitHub 发布页<ArrowUpRight :size="13" /></button><button v-if="release" class="text-button" :disabled="checking || downloading || installing" @click="check(true)"><RefreshCw :size="13" />重新检查</button></div>
        </div>
        <div class="modal-footer"><span>{{ release && !downloading ? '暂不更新：24 小时内不再自动提醒此版本' : downloading ? '可在后台继续下载，稍后从右上角查看进度' : '启动时自动检查，也可随时手动检查' }}</span><div class="update-actions"><button v-if="release" class="button secondary" :disabled="installing" @click="close">{{ downloading ? '后台下载' : ready ? '稍后安装' : '暂不更新' }}</button><button v-if="downloading" class="button secondary" :disabled="cancelling" @click="cancel">{{ cancelling ? '正在取消' : '取消下载' }}</button><button v-else-if="ready" class="button primary update-install" :disabled="!!activeDownloads || installing || checking" @click="install"><LoaderCircle v-if="installing" :size="16" class="spin" /><Download v-else :size="16" />退出并打开安装器</button><button v-else-if="release" class="button primary update-download" :disabled="checking || installing" @click="download"><Download :size="16" />{{ status?.phase === 'error' ? '重试下载' : '下载更新' }}</button><button v-else class="button primary update-check" :disabled="checking" @click="check(true)"><LoaderCircle v-if="checking" :size="16" class="spin" /><RefreshCw v-else :size="16" />{{ checking ? '正在检查' : '检查更新' }}</button></div></div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.version-button{display:inline-flex;align-items:center;gap:7px;padding:6px 9px;color:#8b9bb2;background:#f0f5fc;border:1px solid #e4edf9;border-radius:6px;font-size:10px;white-space:nowrap}.version-button:hover,.version-button.available{color:#3377ed;background:#eaf2ff}.version-dot{width:6px;height:6px;border-radius:50%;background:#3377ed}.update-overlay{z-index:45}.update-modal{width:610px;display:flex;flex-direction:column;overflow:hidden}.modal-header,.modal-footer{flex-shrink:0}.update-body{padding:0 28px 23px;overflow-y:auto;min-height:0}.update-summary{display:flex;align-items:center;gap:15px;margin:3px 0 22px}.update-symbol{display:flex;align-items:center;justify-content:center;width:57px;height:57px;flex-shrink:0;border-radius:17px;background:#edf5ff;color:#508ce4}.update-summary strong{font-size:16px;font-weight:600;color:#405778}.update-summary p{font-size:11px;line-height:1.7;color:#8b9cb4;margin:7px 0 0}.update-notes{padding:14px 16px;background:#f8faff;border:1px solid #e7edf7;border-radius:9px}.update-notes>div{display:flex;justify-content:space-between;gap:12px;font-size:11px;color:#7c90ac}.update-notes strong{font-weight:550}.update-notes span{font-size:10px;color:#a0aec1}.update-notes p{white-space:pre-wrap;overflow-wrap:anywhere;font-size:11px;line-height:1.9;color:#788ba5;max-height:170px;overflow-y:auto;margin:11px 0 0;padding-right:5px}.update-transfer{margin:18px 0 0}.update-transfer>div:first-child{display:flex;justify-content:space-between;gap:12px;font-size:10px;color:#8a9cb5}.update-transfer strong{font-weight:500;color:#668bc0}.update-progress{height:5px;background:#eaf0f8;border-radius:8px;overflow:hidden;margin:10px 0 0}.update-progress>span{display:block;height:100%;background:#4b91ee;transition:width .15s}.update-hint{display:flex;align-items:flex-start;gap:7px;font-size:10px;line-height:1.8;color:#8fa5bf;margin:17px 0 0}.update-hint svg{color:#7aa794;margin-top:2px}.update-portable{font-size:10px;line-height:1.8;color:#a2afc1;margin:7px 0 0 21px}.update-busy{font-size:11px;line-height:1.8;color:#bb925b;padding:9px 11px;border:1px solid #f4e9d6;border-radius:6px;background:#fffbf4}.update-links{display:flex;align-items:center;justify-content:space-between;margin-top:17px}.update-error{display:flex;gap:7px;align-items:flex-start;font-size:11px;color:#bd7d77;line-height:1.8;margin:16px 0 0}.update-error svg{margin-top:2px}.update-actions{display:flex;gap:9px;flex-shrink:0}.modal-footer{align-items:center}.modal-footer>span{font-size:9px;line-height:1.8;white-space:normal;max-width:210px}@media(max-width:700px){.modal-footer{flex-direction:column;align-items:flex-end}.modal-footer>span{max-width:none}}
.update-source{font-size:10px;line-height:1.8;color:#8a9cb5;margin:10px 0;overflow-wrap:anywhere}.update-source strong{font-weight:550;color:#6387b8}.update-mirrors{margin-top:15px;border:1px solid #e7edf7;border-radius:8px;background:#fafcff;font-size:10px;color:#8296b1}.update-mirrors summary{cursor:pointer;padding:11px 13px;color:#6686b1}.update-mirrors summary span{margin-left:8px;color:#98a9bf}.update-mirrors>p{padding:0 13px;line-height:1.8;margin:0 0 9px}.mirror-table-wrap{max-height:180px;overflow:auto;border-top:1px solid #e7edf7}.update-mirrors table{width:100%;border-collapse:collapse;table-layout:fixed}.update-mirrors th,.update-mirrors td{text-align:left;padding:8px 10px;border-bottom:1px solid #eef2f8;font-weight:400;overflow-wrap:anywhere}.update-mirrors th{position:sticky;top:0;background:#f1f6fd;color:#7e94b1}.update-mirrors th:first-child{width:38px}.update-mirrors th:nth-child(3){width:76px}.update-mirrors th:last-child{width:73px}.update-mirrors tr.selected{background:#eaf3ff}.update-mirrors td.available{color:#66a48a}.update-mirrors td.testing{color:#4d89e4}.update-mirrors td.failed,.update-mirrors td.timeout{color:#bb8e78}
</style>
