<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import {
  ArrowDown, ArrowDownToLine, ArrowUpRight, Check, CheckCheck, ChevronDown, CircleAlert,
  CircleCheck, Clock3, ClipboardPaste, Download, FileText, Film, FolderOpen,
  HardDrive, Info, Link2, LoaderCircle, Music2, Pause, Play, Plus, RotateCcw,
  Search, Settings2, ShieldCheck, SlidersHorizontal, Snowflake, Trash2, UserRound, X, Minus, Square, Copy, QrCode,
} from '@lucide/vue'
import * as api from './api'
import UiSelect from './components/UiSelect.vue'
import BatchDownload from './components/BatchDownload.vue'
import UpdateCenter from './components/UpdateCenter.vue'
import { identifySource, platformLabels, sourceSummary } from './sources'
import { bytes, duration, errorMessage, statusLabels } from './format'
import type { AppSnapshot, BatchResult, ClipboardCapture, ClipboardSnapshot, DownloadTask, OutputFormat, Settings, VideoInfo, LoginQr, Platform, SourceLink } from './types'

const snapshot = ref<AppSnapshot | null>(null)
const view = ref<'active' | 'completed' | 'history'>('active')
const link = ref('')
const inputEdited = ref(false)
const linkInput = ref<HTMLInputElement>()
const analyzing = ref(false)
const submitting = ref(false)
const video = ref<VideoInfo | null>(null)
const selectedPages = ref<number[]>([])
const quality = ref(32)
const format = ref<OutputFormat>('video')
const dialog = ref<'settings' | 'about' | null>(null)
const batchOpen = ref(false)
const batchButton = ref<HTMLButtonElement>()
const updateCenter = ref<InstanceType<typeof UpdateCenter>>()
const updateOpen = ref(false)
const settingsTab = ref<'download' | 'account'>('download')
const settingsDraft = reactive<Settings>({ downloadDir: '', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false })
const accountPlatform = ref<Platform>('bilibili')
const clipboardSuggestion = ref<SourceLink | null>(null)
const pendingClipboard = ref<ClipboardCapture | null>(null)
const mainFocused = ref(false)
let clipboardRevision = -1
let acknowledgedClipboard = 0
let unwatchClipboard: (() => void) | undefined
let disposed = false
let unwatchFocus: (() => void) | undefined
let checkingClipboard = false
const detectedSource = computed(() => identifySource(link.value))
const platformList: Platform[] = ['bilibili', 'douyin', 'xiaohongshu']
const filterOptions = [{ value: 'all', label: '全部状态' }, { value: 'downloading', label: '进行中' }, { value: 'paused', label: '已暂停' }, { value: 'failed', label: '失败' }]
const concurrencyOptions = [1, 2, 3, 4].map(value => ({ value, label: `${value} 个任务` }))
const siteSessionSaved = computed(() => snapshot.value?.siteSessions.some(site => site.platform === accountPlatform.value && site.saved))
const loginQr = ref<LoginQr | null>(null)
const loginState = ref<'idle' | 'loading' | 'waiting' | 'scanned' | 'expired' | 'error'>('idle')
const loginError = ref('')
const maximized = ref(false)
let loginTimer: ReturnType<typeof setTimeout> | undefined
let loginGeneration = 0
let loginPlatform: Platform = 'bilibili'
let unwatchWindow: (() => void) | undefined
const savingSettings = ref(false)
const query = ref('')
const selectedTasks = ref<string[]>([])
const filter = ref<'all' | 'downloading' | 'paused' | 'failed'>('all')
const toast = ref<{ text: string; type: 'success' | 'error' | 'info' } | null>(null)
let toastTimer: ReturnType<typeof setTimeout> | undefined
let unsubscribe: (() => void) | undefined

const tasks = computed(() => snapshot.value?.tasks ?? [])
const activeTasks = computed(() => tasks.value.filter(t => !['completed', 'cancelled'].includes(t.status)))
const completedTasks = computed(() => tasks.value.filter(t => t.status === 'completed'))
const runningTasks = computed(() => tasks.value.filter(t => ['downloading', 'merging'].includes(t.status)))
const currentTasks = computed(() => {
  let list = view.value === 'active' ? activeTasks.value : view.value === 'completed' ? completedTasks.value : tasks.value
  if (filter.value !== 'all' && view.value === 'active') list = list.filter(t => filter.value === 'downloading' ? ['queued', 'downloading', 'merging'].includes(t.status) : t.status === filter.value)
  const term = query.value.trim().toLocaleLowerCase()
  return term ? list.filter(t => `${t.title} ${t.pageTitle} ${t.bvid}`.toLocaleLowerCase().includes(term)) : list
})
const totalSpeed = computed(() => tasks.value.reduce((sum, t) => sum + t.speed, 0))
const viewTitle = computed(() => ({ active: '下载任务', completed: '已完成', history: '下载历史' }[view.value]))
const allSelected = computed(() => currentTasks.value.length > 0 && currentTasks.value.every(t => selectedTasks.value.includes(t.id)))
const allPagesSelected = computed(() => !!video.value && video.value.pages.every(p => selectedPages.value.includes(p.cid)))

function notify(text: string, type: 'success' | 'error' | 'info' = 'info') {
  clearTimeout(toastTimer)
  toast.value = { text, type }
  toastTimer = setTimeout(() => { toast.value = null }, type === 'error' ? 8000 : 4500)
}

function setView(next: typeof view.value) { view.value = next; selectedTasks.value = []; filter.value = 'all'; query.value = '' }
function focusLink() { setView('active'); linkInput.value?.focus() }
function openUpdateCenter() { dialog.value = null; updateCenter.value?.open() }
function openBatch() {
  if (!snapshot.value || analyzing.value || submitting.value || video.value || dialog.value || updateOpen.value) return
  batchOpen.value = true
}
async function closeBatch() { batchOpen.value = false; await nextTick(); batchButton.value?.focus() }
function batchQueued(result: BatchResult) {
  setView('active')
  notify(`已加入 ${result.added} 个下载任务${result.skipped ? `，跳过 ${result.skipped} 个已有任务` : ''}。`, result.added ? 'success' : 'info')
}

async function analyze() {
  if (analyzing.value || updateOpen.value) return
  if (!link.value.trim()) { focusLink(); notify('先粘贴一个B站、抖音或小红书视频链接。'); return }
  const source = identifySource(link.value)
  if (!source) { notify('没有识别到支持的视频链接，可粘贴平台的完整分享内容。', 'error'); return }
  link.value = source.url
  clipboardSuggestion.value = null
  analyzing.value = true
  if (pendingClipboard.value?.source.url === source.url) discardClipboard()
  try {
    const result = await api.inspectVideo(link.value)
    if (!disposed) presentVideo(result)
  } catch (error) { notify(errorMessage(error), 'error') }
  finally { analyzing.value = false }
}

async function paste() {
  try {
    const source = await api.readClipboardLink()
    if (!source) { notify('剪贴板中没有B站、抖音或小红书视频链接。'); return }
    link.value = source.url; linkInput.value?.focus()
    await analyze()
  } catch { focusLink(); notify('无法读取剪贴板，请按 Ctrl+V 粘贴链接，再按 Enter 解析。') }
}

function onPaste(event: ClipboardEvent) {
  const source = identifySource(event.clipboardData?.getData('text/plain') ?? '')
  if (source) { event.preventDefault(); inputEdited.value = true; link.value = source.url; clipboardSuggestion.value = null }
}

function presentVideo(result: VideoInfo) {
  inputEdited.value = false
  video.value = result; selectedPages.value = [result.selectedCid]
  quality.value = result.qualities[0]?.id ?? 32; format.value = 'video'
}

function receiveClipboard(status: ClipboardSnapshot) {
  if (disposed || status.revision < clipboardRevision) return
  clipboardRevision = status.revision
  pendingClipboard.value = status.capture && status.capture.id > acknowledgedClipboard ? status.capture : null
  if (!pendingClipboard.value) clipboardSuggestion.value = null
  applyClipboard()
}

function discardClipboard() {
  const capture = pendingClipboard.value
  pendingClipboard.value = null; clipboardSuggestion.value = null
  if (capture) {
    acknowledgedClipboard = Math.max(acknowledgedClipboard, capture.id)
    void api.acknowledgeClipboard(capture.id).catch(() => {})
  }
}

function applyClipboard() {
  if (!mainFocused.value || !snapshot.value?.settings.clipboardEnabled || analyzing.value || submitting.value || video.value || dialog.value || batchOpen.value || updateOpen.value) return
  const capture = pendingClipboard.value
  if (!capture) return
  clipboardSuggestion.value = capture.source
  if (inputEdited.value && link.value.trim() && detectedSource.value?.url !== capture.source.url) return
  link.value = capture.source.url
  if (capture.phase === 'ready' && capture.video && snapshot.value.settings.clipboardAutoParse) {
    presentVideo(capture.video); discardClipboard()
  }
}

async function checkClipboard() {
  if (checkingClipboard || !api.desktop || disposed) return
  checkingClipboard = true
  try {
    receiveClipboard(await api.getClipboardStatus())
  } catch { /* The native monitor keeps the result for the next successful sync. */ }
  finally { checkingClipboard = false }
}

async function parseClipboardSuggestion() {
  if (!clipboardSuggestion.value) return
  link.value = clipboardSuggestion.value.url
  const capture = pendingClipboard.value
  if (capture?.phase === 'ready' && capture.video && capture.source.url === link.value) {
    presentVideo(capture.video); discardClipboard(); return
  }
  await analyze()
}

const clipboardMessage = computed(() => pendingClipboard.value?.phase === 'parsing' ? '正在后台解析' : pendingClipboard.value?.phase === 'ready' ? '已在后台解析，点击查看' : pendingClipboard.value?.phase === 'error' ? '后台解析失败，可点击重试' : '识别到新链接')
watch([mainFocused, analyzing, submitting, video, dialog, batchOpen, updateOpen, () => snapshot.value?.settings.clipboardEnabled, () => snapshot.value?.settings.clipboardAutoParse], applyClipboard)

function setFilter(value: string | number) { filter.value = String(value) as typeof filter.value }

async function openSourceLogin() {
  const source = detectedSource.value
  if (!source || source.platform === 'bilibili') return
  openSettings('account')
  accountPlatform.value = source.platform
  await startLogin()
}

function togglePage(cid: number) {
  selectedPages.value = selectedPages.value.includes(cid) ? selectedPages.value.filter(id => id !== cid) : [...selectedPages.value, cid]
}

async function submit() {
  if (!video.value || !selectedPages.value.length || submitting.value) return
  submitting.value = true
  try {
    await api.createTasks({ video: video.value, pageCids: selectedPages.value, quality: quality.value, format: format.value })
    notify(`已将 ${selectedPages.value.length} 个${video.value.platform === 'bilibili' ? '分P' : '视频'}加入下载队列。`, 'success')
    video.value = null; link.value = ''; setView('active')
  } catch (error) { notify(errorMessage(error), 'error') }
  finally { submitting.value = false }
}

async function action(task: DownloadTask, name: 'pause' | 'resume' | 'cancel' | 'remove' | 'reveal') {
  try { await api.taskAction(task.id, name) }
  catch (error) { notify(errorMessage(error), 'error') }
}

function toggleTask(id: string) {
  selectedTasks.value = selectedTasks.value.includes(id) ? selectedTasks.value.filter(i => i !== id) : [...selectedTasks.value, id]
}

async function bulk(name: 'pause' | 'resume') {
  const selected = tasks.value.filter(t => selectedTasks.value.includes(t.id))
  for (const task of selected) {
    if (name === 'pause' && ['queued', 'downloading'].includes(task.status)) await action(task, name)
    if (name === 'resume' && ['paused', 'failed', 'cancelled'].includes(task.status)) await action(task, name)
  }
}

function openSettings(tab: 'download' | 'account' = 'download') {
  if (snapshot.value) Object.assign(settingsDraft, snapshot.value.settings)
  settingsTab.value = tab; dialog.value = 'settings'
  if (tab === 'account') accountPlatform.value = detectedSource.value?.platform ?? 'bilibili'
}

async function chooseFolder() {
  try { const selected = await api.chooseDirectory(); if (selected) settingsDraft.downloadDir = selected }
  catch (error) { notify(errorMessage(error), 'error') }
}

async function saveSettings() {
  savingSettings.value = true
  try { await api.saveSettings({ ...settingsDraft }); dialog.value = null; notify('下载设置已保存，新任务将使用这些设置。', 'success') }
  catch (error) { notify(errorMessage(error), 'error') }
  finally { savingSettings.value = false }
}

function stopLogin() {
  loginGeneration++
  clearTimeout(loginTimer)
  loginQr.value = null; loginState.value = 'idle'
  void api.cancelLogin(loginPlatform).catch(() => {})
}

async function startLogin() {
  const platform = accountPlatform.value
  const generation = ++loginGeneration
  const previousPlatform = loginPlatform
  loginPlatform = platform
  clearTimeout(loginTimer)
  loginQr.value = null; loginError.value = ''; loginState.value = 'loading'
  try {
    await api.cancelLogin(previousPlatform)
    if (generation !== loginGeneration) return
    const qr = await api.beginLogin(platform)
    if (generation !== loginGeneration) return
    if (qr.state === 'confirmed') {
      loginState.value = 'idle'
      snapshot.value = await api.getSnapshot()
      notify(`${platformLabels[platform]}登录成功，已加密保存。`, 'success')
      return
    }
    loginQr.value = qr; loginState.value = 'waiting'
    const poll = async () => {
      if (generation !== loginGeneration) return
      if (Date.now() >= (loginQr.value?.expiresAt ?? qr.expiresAt)) { await startLogin(); return }
      try {
        const result = await api.pollLogin(qr.id, platform)
        if (generation !== loginGeneration) return
        if (result.state === 'confirmed') {
          loginQr.value = null; loginState.value = 'idle'
          snapshot.value = await api.getSnapshot()
          notify(`${platformLabels[platform]}登录成功，已安全保存。重启后将自动恢复。`, 'success')
          return
        }
        if (result.state === 'expired') { await startLogin(); return }
        if (result.qr) loginQr.value = result.qr
        loginState.value = result.state
        loginTimer = setTimeout(poll, 2000)
      } catch (error) {
        if (generation !== loginGeneration) return
        loginState.value = 'error'; loginError.value = errorMessage(error)
      }
    }
    loginTimer = setTimeout(poll, 1500)
  } catch (error) {
    if (generation !== loginGeneration) return
    loginState.value = 'error'; loginError.value = errorMessage(error)
  }
}

async function logout() {
  savingSettings.value = true
  stopLogin()
  try { await api.logout(accountPlatform.value); snapshot.value = await api.getSnapshot(); notify('已退出账号，并清除软件内此平台的登录状态。', 'success') }
  catch (error) { notify(errorMessage(error), 'error') }
  finally { savingSettings.value = false }
}

async function checkLogin() {
  savingSettings.value = true
  try { await api.checkLogin(accountPlatform.value); snapshot.value = await api.getSnapshot(); if (accountPlatform.value !== 'bilibili') notify(siteSessionSaved.value ? '登录状态有效。' : '登录已失效，请重新扫码。', siteSessionSaved.value ? 'success' : 'info') }
  catch (error) { notify(errorMessage(error), 'error') }
  finally { savingSettings.value = false }
}

async function controlWindow(action: 'minimize' | 'maximize' | 'close') {
  try { await api.windowAction(action) }
  catch (error) { notify(errorMessage(error), 'error') }
}

watch(() => dialog.value === 'settings' && settingsTab.value === 'account' ? accountPlatform.value : null, () => { stopLogin() }, { flush: 'sync' })

async function openFolder() {
  try { await api.openDirectory() } catch (error) { notify(errorMessage(error), 'error') }
}

function onKey(event: KeyboardEvent) {
  if (batchOpen.value || updateOpen.value) return
  if (event.ctrlKey && event.key.toLowerCase() === 'n') { event.preventDefault(); openBatch() }
  if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'v') { event.preventDefault(); void paste() }
  if (event.key === 'Escape' && !submitting.value) { video.value = null; dialog.value = null }
}

onMounted(async () => {
  document.addEventListener('keydown', onKey)
  try {
    const stopState = await api.subscribe((state) => { if (!disposed) snapshot.value = state })
    if (disposed) { stopState(); return }; unsubscribe = stopState
    const initial = await api.getSnapshot()
    if (disposed) return
    snapshot.value = initial
    unwatchWindow = await api.watchMaximized(value => { if (!disposed) maximized.value = value })
    if (disposed) { unwatchWindow(); return }
    unwatchClipboard = await api.subscribeClipboard(receiveClipboard)
    if (disposed) { unwatchClipboard(); return }
    unwatchFocus = await api.watchFocus(focused => { if (disposed) return; mainFocused.value = focused; if (focused) void checkClipboard() })
    if (disposed) { unwatchFocus(); return }
    mainFocused.value = await api.windowFocused()
    if (disposed) return
    if (snapshot.value.settings.clipboardEnabled) await checkClipboard()
  } catch (error) { if (!disposed) notify(`初始化失败：${errorMessage(error)}`, 'error') }
})

onUnmounted(() => { disposed = true; unsubscribe?.(); unwatchWindow?.(); unwatchFocus?.(); unwatchClipboard?.(); stopLogin(); clearTimeout(toastTimer); document.removeEventListener('keydown', onKey) })
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <a class="brand" href="#" @click.prevent="focusLink" aria-label="SnowDown 首页">
        <span class="brand-mark"><Snowflake :size="23" :stroke-width="1.8" /></span>
        <span>SnowDown<span class="brand-dot">.</span></span>
      </a>
      <div class="workspace-label">我的工作空间</div>
      <nav aria-label="主导航">
        <button :class="['nav-item', { active: view === 'active' }]" @click="setView('active')">
          <ArrowDownToLine :size="18" /><span>下载任务</span><span class="nav-count">{{ activeTasks.length }}</span>
        </button>
        <button :class="['nav-item', { active: view === 'completed' }]" @click="setView('completed')">
          <CircleCheck :size="18" /><span>已完成</span><span class="nav-count">{{ completedTasks.length }}</span>
        </button>
        <button :class="['nav-item', { active: view === 'history' }]" @click="setView('history')">
          <Clock3 :size="18" /><span>下载历史</span>
        </button>
      </nav>

      <div class="sidebar-collection">
        <div class="mini-snow"><Snowflake :size="18" /></div>
        <p>好内容，<br />慢慢收藏。</p>
        <span>你的私人离线内容库</span>
        <svg class="sidebar-mountains" viewBox="0 0 180 90" aria-hidden="true"><path d="m0 90 53-67 46 49 29-34 52 52" fill="#dbe9f7"/><path d="m26 90 65-81 70 81" fill="#c5daf1"/><path d="m68 38 23-29 25 29-15-5-10 9-10-9z" fill="#fff"/><path d="m0 90 53-67 15 18-13-4-10 10-10-2z" fill="#edf5fc"/></svg>
      </div>

      <div class="sidebar-bottom">
        <button class="nav-item" @click="openFolder"><FolderOpen :size="18" /><span>打开下载目录</span><ArrowUpRight class="subtle" :size="14" /></button>
        <button class="nav-item" @click="openSettings()"><Settings2 :size="18" /><span>设置</span></button>
        <div class="sidebar-divider"></div>
        <button class="profile" @click="openSettings('account')">
          <span class="avatar"><img v-if="snapshot?.auth.account?.avatar" :src="snapshot.auth.account.avatar" alt="" referrerpolicy="no-referrer" /><UserRound v-else :size="17" /></span>
          <span class="profile-text"><strong>{{ snapshot?.auth.account?.name || '本地收藏家' }}</strong><small>{{ snapshot?.authenticated ? 'B站账号 · 登录已保存' : '连接平台账号 · 使用官方会话' }}</small></span>
          <ChevronDown :size="13" />
        </button>
      </div>
    </aside>

    <div class="main-shell">
      <header class="topbar" data-tauri-drag-region>
        <div class="breadcrumb" data-tauri-drag-region>工作空间 <span>/</span> <strong>{{ viewTitle }}</strong></div>
        <div class="topbar-right"><span class="platform-badge"><span class="tiny-dot"></span> Windows 桌面版</span><UpdateCenter v-if="snapshot" ref="updateCenter" :current-version="snapshot.version" :can-prompt="!video && !dialog && !batchOpen && !analyzing && !submitting" :active-downloads="tasks.filter(task => ['queued', 'downloading', 'merging'].includes(task.status)).length" @visibility="value => updateOpen = value" /><button class="icon-button" aria-label="关于 SnowDown" @click="dialog = 'about'"><Info :size="18" /></button><div class="window-controls"><button aria-label="最小化窗口" title="最小化" @click="controlWindow('minimize')"><Minus :size="15" /></button><button :aria-label="maximized ? '还原窗口' : '最大化窗口'" :title="maximized ? '还原' : '最大化'" @click="controlWindow('maximize')"><Copy v-if="maximized" :size="13" /><Square v-else :size="13" /></button><button class="window-close" aria-label="关闭到托盘" title="关闭到托盘，继续下载；完全退出请使用托盘菜单" @click="controlWindow('close')"><X :size="17" /></button></div></div>
      </header>

      <main class="workspace">
        <section class="page-heading">
          <div><div class="eyebrow">YOUR CONTENT, YOUR SPACE</div><h1>{{ viewTitle }}<span class="title-dot">.</span></h1><p>{{ view === 'active' ? '留住每一次灵感，让喜欢的内容随时相伴。' : view === 'completed' ? '下载完成的内容，都在这里等你。' : '每一次收藏，都有迹可循。' }}</p></div>
          <button ref="batchButton" class="button primary new-task" :disabled="analyzing || submitting || !snapshot" @click="openBatch"><Plus :size="17" />批量下载<kbd>Ctrl N</kbd></button>
        </section>

        <section class="link-card" aria-label="添加下载">
          <div class="link-card-label"><div class="platform-list"><span v-for="platform in platformList" :key="platform" :class="['platform-chip', platform, { detected: detectedSource?.platform === platform }]">{{ platformLabels[platform] }}</span></div><span>从一个链接开始收藏</span><span class="local-badge"><ShieldCheck :size="12" />本地下载</span></div>
          <form class="link-form" @submit.prevent="analyze">
            <Link2 :size="19" class="link-icon" />
            <input ref="linkInput" v-model="link" aria-label="视频链接" placeholder="粘贴B站、抖音、小红书链接，或完整分享内容…" :disabled="analyzing" spellcheck="false" @paste="onPaste" @input="inputEdited = true" />
            <button class="paste-button" type="button" title="读取剪贴板并解析 · Ctrl+Shift+V" :disabled="analyzing" @click="paste"><ClipboardPaste :size="15" /><span>粘贴并解析</span></button>
            <button class="button primary analyze-button" type="submit" :disabled="analyzing"><LoaderCircle v-if="analyzing" :size="16" class="spin" /><Download v-else :size="16" />{{ analyzing ? '正在解析' : '解析视频' }}</button>
          </form>
          <div class="link-card-footer"><span v-if="detectedSource" class="recognized-source"><CircleCheck :size="13" />已识别{{ platformLabels[detectedSource.platform] }}<small>{{ sourceSummary(detectedSource) }}</small></span><span v-else><Check :size="13" />自动识别平台 · 支持分享短链接</span><button v-if="detectedSource && detectedSource.platform !== 'bilibili'" class="text-button" @click="openSourceLogin">登录{{ platformLabels[detectedSource.platform] }}</button><span class="shortcut-tip"><kbd>Ctrl Shift V</kbd> 粘贴并解析 · <kbd>Enter</kbd> 解析</span></div>
          <div v-if="clipboardSuggestion" class="clipboard-suggestion"><LoaderCircle v-if="pendingClipboard?.phase === 'parsing'" :size="15" class="spin" /><ClipboardPaste v-else :size="15" /><span>剪贴板识别到{{ platformLabels[clipboardSuggestion.platform] }}链接 · {{ clipboardMessage }}</span><button class="text-button" :disabled="pendingClipboard?.phase === 'parsing'" @click="parseClipboardSuggestion">{{ pendingClipboard?.phase === 'ready' ? '查看结果' : pendingClipboard?.phase === 'error' ? '重试解析' : '立即解析' }}<ArrowUpRight :size="13" /></button><button class="icon-button" aria-label="忽略剪贴板链接" @click="discardClipboard"><X :size="14" /></button></div>
        </section>

        <section class="task-section">
          <div class="section-heading">
            <div class="section-title"><h2>{{ view === 'active' ? '下载列表' : viewTitle }}</h2><span class="count-pill">{{ currentTasks.length }}</span></div>
            <div class="list-tools">
              <label v-if="tasks.length" class="search-box"><Search :size="14" /><input v-model="query" aria-label="搜索任务" placeholder="搜索任务" /></label>
              <div v-if="view === 'active'" class="filter-select"><SlidersHorizontal :size="14" /><UiSelect :model-value="filter" :options="filterOptions" label="筛选下载状态" compact @update:model-value="setFilter" /></div>
              <button class="text-button" @click="openFolder"><FolderOpen :size="15" />打开文件夹</button>
            </div>
          </div>

          <div class="task-panel">
            <div class="table-toolbar">
              <label class="checkbox-label"><input type="checkbox" :checked="allSelected" :disabled="!currentTasks.length" @change="selectedTasks = allSelected ? [] : currentTasks.map(t => t.id)" /><span>{{ selectedTasks.length ? `已选 ${selectedTasks.length} 项` : '全选' }}</span></label>
              <div class="bulk-actions"><button :disabled="!selectedTasks.length" @click="bulk('resume')"><Play :size="13" />开始</button><button :disabled="!selectedTasks.length" @click="bulk('pause')"><Pause :size="13" />暂停</button></div>
              <span class="table-right-label">{{ runningTasks.length ? `${runningTasks.length} 个任务正在进行` : '准备好，开始收藏' }}</span>
            </div>

            <div v-if="!currentTasks.length" class="empty-state">
              <div class="empty-illustration" aria-hidden="true">
                <span class="floating-flake flake-one"><Snowflake :size="17" /></span><span class="floating-flake flake-two"><Plus :size="11" /></span><span class="floating-flake flake-three"><Plus :size="14" /></span>
                <svg viewBox="0 0 180 140"><defs><linearGradient id="empty-bg" x2="0" y2="1"><stop stop-color="#edf6ff"/><stop offset="1" stop-color="#f8fbff"/></linearGradient></defs><ellipse cx="90" cy="121" rx="57" ry="8" fill="#eaf0f7"/><rect x="44" y="22" width="80" height="90" rx="10" fill="#e4effb" transform="rotate(-10 84 67)"/><rect x="55" y="17" width="80" height="94" rx="10" fill="url(#empty-bg)" stroke="#c2d8f1" stroke-width="1.5"/><path d="m65 85 18-24 17 19 10-12 15 17" fill="#d9e9fa"/><circle cx="110" cy="43" r="8" fill="#d5e7fb"/><path d="M35 92h36l8 10h23l8-10h35v25a7 7 0 0 1-7 7H42a7 7 0 0 1-7-7z" fill="#fff" stroke="#b9d0eb" stroke-width="1.5"/><path d="M89 44v31m-9-9 9 10 9-10" stroke="#5b94da" fill="none" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </div>
              <h3>{{ query || filter !== 'all' ? '没有找到匹配的任务' : view === 'completed' ? '收藏正在路上' : view === 'history' ? '你的收藏故事，从这里开始' : '还没有下载任务' }}</h3>
              <p>{{ query || filter !== 'all' ? '试试其他关键词，或调整筛选条件。' : view === 'completed' ? '视频下载完成后，会自动出现在这里。' : '粘贴视频链接，把喜欢的内容留在本地。' }}</p>
              <button v-if="!query && filter === 'all'" class="button empty-button" @click="openBatch"><Plus :size="15" />添加批量下载</button>
            </div>

            <div v-else class="task-list">
              <article v-for="task in currentTasks" :key="task.id" class="task-row">
                <input type="checkbox" :aria-label="`选择 ${task.title}`" :checked="selectedTasks.includes(task.id)" @change="toggleTask(task.id)" />
                <div class="task-cover"><img v-if="task.cover" :src="task.cover" alt="" loading="lazy" referrerpolicy="no-referrer" /><Film v-else :size="24" /><span>{{ task.format === 'audio' ? '音频' : task.platform === 'bilibili' ? `P${task.page}` : '视频' }}</span></div>
                <div class="task-info">
                  <div class="task-title-line"><h3 :title="task.title">{{ task.title }}</h3><span :class="['status-tag', task.status]">{{ statusLabels[task.status] }}</span></div>
                  <div class="task-meta"><span :class="['task-platform', task.platform]">{{ platformLabels[task.platform ?? 'bilibili'] }}</span><i></i><span v-if="task.format === 'audio'"><Music2 :size="11" />M4A</span><span v-else>MP4 · {{ task.qualityLabel }}</span><i></i><span class="page-title" :title="task.pageTitle">{{ task.platform === 'bilibili' || !task.platform ? `P${task.page} · ` : '' }}{{ task.pageTitle }}</span></div>
                  <div v-if="['downloading', 'merging', 'paused', 'queued'].includes(task.status)" class="progress-track"><div :class="{ paused: task.status === 'paused' }" :style="{ width: `${Math.min(task.progress, 100)}%` }"></div></div>
                  <div class="task-detail"><span v-if="task.error" class="task-error" :title="task.error">{{ task.error }}</span><span v-else-if="task.warning" class="task-warning" :title="task.warning">{{ task.warning }}</span><span v-else-if="task.status === 'completed'">{{ bytes(task.totalBytes) }}<span class="detail-separator">·</span>已保存到下载目录</span><span v-else-if="task.status === 'queued' && task.startIntervalSecs">等待下载<span class="detail-separator">·</span>同批启动间隔 {{ task.startIntervalSecs }} 秒</span><span v-else>{{ bytes(task.downloadedBytes) }}<template v-if="task.totalBytes"> / {{ bytes(task.totalBytes) }}</template></span><span v-if="task.status === 'downloading'" class="task-speed">{{ bytes(task.speed) }}/s<span class="detail-separator">·</span>{{ task.progress.toFixed(1) }}%</span><span v-else-if="task.status === 'merging'">正在生成完整文件…</span></div>
                </div>
                <div class="task-actions">
                  <button v-if="['queued', 'downloading'].includes(task.status)" class="icon-button" title="暂停下载" @click="action(task, 'pause')"><Pause :size="17" /></button>
                  <button v-if="['paused', 'failed', 'cancelled'].includes(task.status)" class="icon-button" :title="task.status === 'paused' ? '继续下载' : '重新下载'" @click="action(task, 'resume')"><Play v-if="task.status === 'paused'" :size="17" /><RotateCcw v-else :size="17" /></button>
                  <button v-if="task.status === 'completed'" class="icon-button" title="显示已下载文件" @click="action(task, 'reveal')"><FolderOpen :size="18" /></button>
                  <button v-if="['queued', 'downloading', 'merging', 'paused'].includes(task.status)" class="icon-button" title="取消下载" @click="action(task, 'cancel')"><X :size="17" /></button>
                  <button v-else class="icon-button delete-button" title="移除记录（保留本地文件）" @click="action(task, 'remove')"><Trash2 :size="16" /></button>
                </div>
              </article>
            </div>
            <div class="table-footer"><span><HardDrive :size="13" />{{ snapshot?.settings.downloadDir || '下载到你选择的本地文件夹' }}</span><button class="text-button" @click="openSettings()">更改位置<ArrowUpRight :size="12" /></button></div>
          </div>
        </section>

        <div class="workspace-note"><span class="note-icon"><ShieldCheck :size="16" /></span><div><strong>安心收藏，文件始终在你手中</strong><p>视频直接保存到本地。关闭窗口后，SnowDown 会在系统托盘继续下载。</p></div><span class="note-decoration"><Snowflake :size="33" :stroke-width="1" /></span></div>
      </main>

      <footer class="statusbar"><span><span :class="['status-dot', { warning: !snapshot?.mediaReady }]"></span>{{ !api.desktop ? '浏览器界面预览 · 下载请使用桌面客户端' : snapshot?.mediaReady ? '下载引擎已就绪' : '正在检查音视频组件' }}</span><div><span><ArrowDown :size="12" />{{ bytes(totalSpeed) }}/s</span><span>{{ runningTasks.length }} 个进行中</span><button @click="updateCenter?.open()">v{{ snapshot?.version ?? '0.1.8' }}</button></div></footer>
    </div>

    <Transition name="toast"><div v-if="toast" :class="['toast-message', toast.type]" role="status"><CircleAlert v-if="toast.type === 'error'" :size="18" /><CircleCheck v-else-if="toast.type === 'success'" :size="18" /><Info v-else :size="18" /><span>{{ toast.text }}</span><button aria-label="关闭提示" @click="toast = null"><X :size="15" /></button></div></Transition>

    <BatchDownload v-if="batchOpen && snapshot" :settings="snapshot.settings" @close="closeBatch" @queued="batchQueued" />

    <div v-if="video" class="modal-overlay" @mousedown.self="!submitting && (video = null)">
      <section class="modal video-modal" role="dialog" aria-modal="true" aria-labelledby="video-dialog-title">
        <div class="modal-header"><div><span class="eyebrow">NEW COLLECTION</span><h2 id="video-dialog-title">准备好收藏了吗？</h2></div><button class="icon-button" aria-label="关闭视频选择" :disabled="submitting" @click="video = null"><X :size="20" /></button></div>
        <div class="video-summary"><img :src="video.cover" alt="视频封面" referrerpolicy="no-referrer" /><div><h3>{{ video.title }}</h3><p>{{ video.author }}<span>·</span>{{ duration(video.duration) }}<span>·</span>{{ video.pages.length }} {{ video.platform === 'bilibili' ? '个分P' : '个视频' }}</p><small>{{ platformLabels[video.platform] }} · {{ video.bvid }}</small></div></div>
        <div class="format-grid"><label :class="['format-option', { chosen: format === 'video' }]"><input v-model="format" type="radio" value="video" /><Film :size="21" /><span><strong>视频</strong><small>MP4 · 自动合并音视频</small></span><CircleCheck v-if="format === 'video'" :size="17" /></label><label :class="['format-option', { chosen: format === 'audio' }]"><input v-model="format" type="radio" value="audio" /><Music2 :size="21" /><span><strong>仅音频</strong><small>{{ video.platform === 'bilibili' ? 'M4A · 保留原始音质' : 'M4A · 提取音轨' }}</small></span><CircleCheck v-if="format === 'audio'" :size="17" /></label></div>
        <div v-if="format === 'video'" class="field-row"><span>视频画质</span><UiSelect :model-value="quality" :options="video.qualities.map(item => ({ value: item.id, label: item.label + (item.width ? ` · ${item.width} × ${item.height}` : '') }))" label="视频画质" @update:model-value="value => quality = Number(value)" /></div>
        <p class="quality-hint"><Info :size="13" />{{ platformLabels[video.platform] }} · {{ video.platform === 'bilibili' ? '仅展示当前可用画质。扫码登录后重新解析可刷新画质。' : '仅展示平台实际返回的视频流，访问限制可在平台账号中处理。' }}</p>
        <div class="pages-heading"><strong>{{ video.platform === 'bilibili' ? '选择分P' : '选择视频' }} <span>{{ selectedPages.length }} / {{ video.pages.length }}</span></strong><button v-if="video.pages.length > 1" class="text-button" @click="selectedPages = allPagesSelected ? [] : video.pages.map(p => p.cid)"><CheckCheck :size="14" />{{ allPagesSelected ? '取消全选' : '全选分P' }}</button></div>
        <div class="pages-list"><label v-for="page in video.pages" :key="page.cid" :class="['page-option', { checked: selectedPages.includes(page.cid) }]"><input type="checkbox" :checked="selectedPages.includes(page.cid)" @change="togglePage(page.cid)" /><span class="page-number">{{ video.platform === 'bilibili' ? `P${page.page}` : page.page }}</span><span class="part-title">{{ page.part }}</span><small>{{ duration(page.duration) }}</small></label></div>
        <div class="modal-footer"><span><FolderOpen :size="14" />保存到本地下载目录</span><button class="button primary" :disabled="!selectedPages.length || submitting" @click="submit"><LoaderCircle v-if="submitting" :size="16" class="spin" /><Download v-else :size="16" />下载 {{ selectedPages.length }} 项</button></div>
      </section>
    </div>

    <div v-if="dialog === 'settings'" class="modal-overlay" @mousedown.self="dialog = null">
      <section class="modal settings-modal" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <div class="modal-header"><div><span class="eyebrow">MAKE IT YOURS</span><h2 id="settings-title">偏好设置</h2></div><button class="icon-button" aria-label="关闭设置" @click="dialog = null"><X :size="20" /></button></div>
        <div class="settings-tabs"><button :class="{ active: settingsTab === 'download' }" @click="settingsTab = 'download'"><SlidersHorizontal :size="15" />下载设置</button><button :class="{ active: settingsTab === 'account' }" @click="settingsTab = 'account'"><UserRound :size="15" />平台账号</button></div>
        <div v-if="settingsTab === 'download'" class="settings-content">
          <label class="field-label">下载位置</label><div class="folder-field"><FolderOpen :size="17" /><input v-model="settingsDraft.downloadDir" placeholder="选择本地文件夹" aria-label="下载位置" /><button @click="chooseFolder">浏览</button></div>
          <div class="settings-two-col"><div><span class="field-label">同时下载任务数</span><UiSelect :model-value="settingsDraft.concurrency" :options="concurrencyOptions" label="同时下载任务数" @update:model-value="value => settingsDraft.concurrency = Number(value)" /></div><label><span class="field-label">单任务限速（MB/s）</span><input v-model.number="settingsDraft.maxSpeedMbps" type="number" min="0" max="1024" /><small>设为 0 表示不限速</small></label></div>
          <div class="settings-divider"></div><h3 class="settings-section-title">剪贴板与快速解析</h3>
          <label class="toggle-row"><span><ClipboardPaste :size="17" /><span>识别剪贴板链接<small>后台持续识别支持的平台链接，关闭到托盘后仍生效</small></span></span><input v-model="settingsDraft.clipboardEnabled" type="checkbox" role="switch" /></label>
          <label class="toggle-row"><span><Link2 :size="17" /><span>识别后自动解析<small>后台准备最新链接，回到窗口查看；不会自动下载</small></span></span><input v-model="settingsDraft.clipboardAutoParse" :disabled="!settingsDraft.clipboardEnabled" type="checkbox" role="switch" /></label>
          <div class="settings-divider"></div><h3 class="settings-section-title">顺便一起收藏</h3><p class="settings-description">可用时，与视频保存在同一文件夹。弹幕与字幕目前适用于B站。</p>
          <label class="toggle-row"><span><Film :size="17" /><span>保存视频封面<small>原始封面图片</small></span></span><input v-model="settingsDraft.saveCover" type="checkbox" role="switch" /></label>
          <label class="toggle-row"><span><FileText :size="17" /><span>保存弹幕<small>XML 格式</small></span></span><input v-model="settingsDraft.saveDanmaku" type="checkbox" role="switch" /></label>
          <label class="toggle-row"><span><FileText :size="17" /><span>保存字幕<small>SRT 格式，取决于视频是否提供字幕</small></span></span><input v-model="settingsDraft.saveSubtitles" type="checkbox" role="switch" /></label>
          <div :class="['component-status', { missing: !snapshot?.mediaReady }]"><CircleCheck v-if="snapshot?.mediaReady" :size="16" /><CircleAlert v-else :size="16" /><span>{{ snapshot?.mediaReady ? 'FFmpeg 音视频组件已就绪' : api.desktop ? '未找到 FFmpeg，请重新安装完整客户端。' : '界面预览模式，桌面端会检查 FFmpeg 组件。' }}</span></div>
        </div>
        <div v-else class="settings-content account-content">
          <div class="account-platform-tabs"><button v-for="platform in platformList" :key="platform" :class="{ active: accountPlatform === platform }" @click="accountPlatform = platform">{{ platformLabels[platform] }}</button></div>
          <template v-if="accountPlatform === 'bilibili' && snapshot?.authenticated && !loginQr && loginState === 'idle'">
            <div class="account-illustration account-avatar"><img v-if="snapshot.auth.account?.avatar" :src="snapshot.auth.account.avatar" alt="B站头像" referrerpolicy="no-referrer" /><UserRound v-else :size="25" /></div>
            <h3>{{ snapshot.auth.account?.name || 'B站账号' }}</h3>
            <span class="login-saved"><ShieldCheck :size="14" />登录状态已加密保存</span>
            <p>下次打开 SnowDown 时自动恢复。画质和字幕以此账号可用的权限为准。</p>
            <p v-if="snapshot.auth.state === 'checking'" class="account-tip">正在验证登录状态…</p>
            <p v-if="snapshot.auth.message" class="account-tip">{{ snapshot.auth.message }}</p>
            <div class="account-buttons"><button class="button secondary" :disabled="savingSettings" @click="logout">退出账号</button><button class="button secondary" :disabled="savingSettings" @click="checkLogin">检查登录</button></div>
          </template>
          <template v-else-if="accountPlatform !== 'bilibili' && siteSessionSaved && !loginQr && loginState === 'idle'">
            <div class="account-illustration"><ShieldCheck :size="25" /></div>
            <h3>{{ platformLabels[accountPlatform] }}账号</h3>
            <span class="login-saved"><ShieldCheck :size="14" />登录状态已加密保存</span>
            <p>已保存此平台登录状态。输入视频链接并点击“解析视频”即可选择下载。</p>
            <div class="account-buttons"><button class="button secondary" :disabled="savingSettings" @click="logout">退出账号</button><button class="button secondary" :disabled="savingSettings" @click="checkLogin">检查登录</button></div>
          </template>
          <template v-else>
            <div class="account-illustration"><QrCode :size="25" /></div>
            <h3>{{ platformLabels[accountPlatform] }}官方扫码登录</h3>
            <p>使用手机{{ platformLabels[accountPlatform] }}扫描二维码，在手机上确认登录。二维码过期后会自动更新。</p>
            <div class="login-qr" :aria-busy="loginState === 'loading'">
              <img v-if="loginQr" :src="loginQr.image" :alt="platformLabels[accountPlatform] + '官方登录二维码'" :class="{ faded: loginState === 'error' }" />
              <LoaderCircle v-else-if="loginState === 'loading'" class="spin" :size="30" />
              <QrCode v-else :size="82" :stroke-width="1" />
              <div v-if="loginState === 'error'" class="qr-expired"><button class="button primary" @click="startLogin"><RotateCcw :size="15" />重新获取</button></div>
            </div>
            <p class="qr-status" role="status">{{ loginState === 'loading' ? '正在获取官方二维码…' : loginState === 'waiting' ? '请用手机' + platformLabels[accountPlatform] + '扫码 · 过期自动更新' : loginState === 'scanned' ? '已扫码，请在手机' + platformLabels[accountPlatform] + '确认登录' : loginState === 'error' ? loginError : '确认登录后，将自动保存登录状态' }}</p>
            <button v-if="loginState === 'idle'" class="button primary login-start" @click="startLogin"><QrCode :size="16" />获取登录二维码</button>
          </template>
          <div class="login-privacy"><ShieldCheck :size="16" /><span>二维码来自平台官方登录流程，过期后自动更新。登录状态由 Windows 加密保存，仅供当前系统用户使用。登录与视频解析相互独立。</span></div>
        </div>
        <div v-if="settingsTab === 'download'" class="modal-footer"><span>设置适用于之后创建的新任务</span><button class="button primary" :disabled="savingSettings" @click="saveSettings"><Check :size="16" />保存设置</button></div>
      </section>
    </div>

    <div v-if="dialog === 'about'" class="modal-overlay" @mousedown.self="dialog = null"><section class="modal about-modal" role="dialog" aria-modal="true" aria-labelledby="about-title"><button class="icon-button about-close" aria-label="关闭关于" @click="dialog = null"><X :size="20" /></button><span class="brand-mark about-mark"><Snowflake :size="33" /></span><h2 id="about-title">SnowDown<span>.</span></h2><p>好内容，慢慢收藏。</p><span class="version-tag">Windows · v{{ snapshot?.version ?? '0.1.8' }}</span><div class="about-description">B站、抖音、小红书视频，本地收藏。<br />视频、音频、字幕与弹幕，一处收藏。</div><div class="about-tech"><ShieldCheck :size="14" />本机解析与下载<span>·</span>Rust + Tauri + Vue</div><button class="button secondary" @click="openUpdateCenter">检查更新<RotateCcw :size="14" /></button></section></div>
  </div>
</template>
