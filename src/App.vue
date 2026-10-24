<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import {
  ArrowDown, ArrowDownToLine, ArrowUpRight, Check, CheckCheck, ChevronDown, CircleAlert,
  CircleCheck, Clock3, ClipboardPaste, Download, ExternalLink, FileText, Film, FolderOpen,
  HardDrive, Info, Link2, LoaderCircle, Music2, Pause, Play, Plus, RotateCcw,
  Search, Settings2, ShieldCheck, SlidersHorizontal, Snowflake, Trash2, UserRound, X, Minus, Square, Copy, QrCode,
} from '@lucide/vue'
import * as api from './api'
import { bytes, duration, errorMessage, statusLabels } from './format'
import type { AppSnapshot, DownloadTask, OutputFormat, Settings, VideoInfo, LoginQr } from './types'

const snapshot = ref<AppSnapshot | null>(null)
const view = ref<'active' | 'completed' | 'history'>('active')
const link = ref('')
const linkInput = ref<HTMLInputElement>()
const analyzing = ref(false)
const submitting = ref(false)
const video = ref<VideoInfo | null>(null)
const selectedPages = ref<number[]>([])
const quality = ref(32)
const format = ref<OutputFormat>('video')
const dialog = ref<'settings' | 'about' | null>(null)
const settingsTab = ref<'download' | 'account'>('download')
const settingsDraft = reactive<Settings>({ downloadDir: '', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false })
const loginQr = ref<LoginQr | null>(null)
const loginState = ref<'idle' | 'loading' | 'waiting' | 'scanned' | 'expired' | 'error'>('idle')
const loginError = ref('')
const maximized = ref(false)
let loginTimer: ReturnType<typeof setTimeout> | undefined
let loginGeneration = 0
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

async function analyze() {
  if (analyzing.value) return
  if (!link.value.trim()) { focusLink(); notify('先粘贴一个B站视频链接或BV号。'); return }
  analyzing.value = true
  try {
    const result = await api.inspectVideo(link.value)
    video.value = result
    selectedPages.value = [result.selectedCid]
    quality.value = result.qualities[0]?.id ?? 32
    format.value = 'video'
  } catch (error) { notify(errorMessage(error), 'error') }
  finally { analyzing.value = false }
}

async function paste() {
  try { link.value = await navigator.clipboard.readText(); linkInput.value?.focus() }
  catch { focusLink(); notify('请在输入框中按 Ctrl+V 粘贴链接。') }
}

function togglePage(cid: number) {
  selectedPages.value = selectedPages.value.includes(cid) ? selectedPages.value.filter(id => id !== cid) : [...selectedPages.value, cid]
}

async function submit() {
  if (!video.value || !selectedPages.value.length || submitting.value) return
  submitting.value = true
  try {
    await api.createTasks({ video: video.value, pageCids: selectedPages.value, quality: quality.value, format: format.value })
    notify(`已将 ${selectedPages.value.length} 个分P加入下载队列。`, 'success')
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
  void api.cancelLogin().catch(() => {})
}

async function startLogin() {
  const generation = ++loginGeneration
  clearTimeout(loginTimer)
  loginQr.value = null; loginError.value = ''; loginState.value = 'loading'
  try {
    await api.cancelLogin()
    if (generation !== loginGeneration) return
    const qr = await api.beginLogin()
    if (generation !== loginGeneration) return
    loginQr.value = qr; loginState.value = 'waiting'
    const poll = async () => {
      if (generation !== loginGeneration) return
      if (Date.now() >= qr.expiresAt) { loginState.value = 'expired'; return }
      try {
        const result = await api.pollLogin(qr.id)
        if (generation !== loginGeneration) return
        if (result.state === 'confirmed') {
          loginQr.value = null; loginState.value = 'idle'
          snapshot.value = await api.getSnapshot()
          notify('B站登录成功，已安全保存。重启后将自动恢复。', 'success')
          return
        }
        loginState.value = result.state
        if (result.state !== 'expired') loginTimer = setTimeout(poll, 2000)
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
  try { await api.logout(); snapshot.value = await api.getSnapshot(); notify('已退出账号，并删除本机保存的登录状态。', 'success') }
  catch (error) { notify(errorMessage(error), 'error') }
  finally { savingSettings.value = false }
}

async function checkLogin() {
  savingSettings.value = true
  try { await api.checkLogin(); snapshot.value = await api.getSnapshot() }
  catch (error) { notify(errorMessage(error), 'error') }
  finally { savingSettings.value = false }
}

async function controlWindow(action: 'minimize' | 'maximize' | 'close') {
  try { await api.windowAction(action) }
  catch (error) { notify(errorMessage(error), 'error') }
}

watch(() => dialog.value === 'settings' && settingsTab.value === 'account', (visible) => { if (!visible) stopLogin() })

async function openFolder() {
  try { await api.openDirectory() } catch (error) { notify(errorMessage(error), 'error') }
}

function onKey(event: KeyboardEvent) {
  if (event.ctrlKey && event.key.toLowerCase() === 'n') { event.preventDefault(); focusLink() }
  if (event.key === 'Escape' && !submitting.value) { video.value = null; dialog.value = null }
}

onMounted(async () => {
  document.addEventListener('keydown', onKey)
  try {
    unsubscribe = await api.subscribe((state) => { snapshot.value = state })
    snapshot.value = await api.getSnapshot()
    unwatchWindow = await api.watchMaximized(value => { maximized.value = value })
  } catch (error) { notify(`初始化失败：${errorMessage(error)}`, 'error') }
})

onUnmounted(() => { unsubscribe?.(); unwatchWindow?.(); stopLogin(); clearTimeout(toastTimer); document.removeEventListener('keydown', onKey) })
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
          <span class="profile-text"><strong>{{ snapshot?.auth.account?.name || '本地收藏家' }}</strong><small>{{ snapshot?.authenticated ? 'B站账号 · 登录已保存' : '连接B站账号 · 解锁可用画质' }}</small></span>
          <ChevronDown :size="13" />
        </button>
      </div>
    </aside>

    <div class="main-shell">
      <header class="topbar" data-tauri-drag-region>
        <div class="breadcrumb" data-tauri-drag-region>工作空间 <span>/</span> <strong>{{ viewTitle }}</strong></div>
        <div class="topbar-right"><span class="platform-badge"><span class="tiny-dot"></span> Windows 桌面版</span><button class="icon-button" aria-label="关于 SnowDown" @click="dialog = 'about'"><Info :size="18" /></button><div class="window-controls"><button aria-label="最小化窗口" title="最小化" @click="controlWindow('minimize')"><Minus :size="15" /></button><button :aria-label="maximized ? '还原窗口' : '最大化窗口'" :title="maximized ? '还原' : '最大化'" @click="controlWindow('maximize')"><Copy v-if="maximized" :size="13" /><Square v-else :size="13" /></button><button class="window-close" aria-label="关闭到托盘" title="关闭到托盘，继续下载；完全退出请使用托盘菜单" @click="controlWindow('close')"><X :size="17" /></button></div></div>
      </header>

      <main class="workspace">
        <section class="page-heading">
          <div><div class="eyebrow">YOUR CONTENT, YOUR SPACE</div><h1>{{ viewTitle }}<span class="title-dot">.</span></h1><p>{{ view === 'active' ? '留住每一次灵感，让喜欢的内容随时相伴。' : view === 'completed' ? '下载完成的内容，都在这里等你。' : '每一次收藏，都有迹可循。' }}</p></div>
          <button class="button primary new-task" @click="focusLink"><Plus :size="17" />新建下载<kbd>Ctrl N</kbd></button>
        </section>

        <section class="link-card" aria-label="添加下载">
          <div class="link-card-label"><span class="bilibili-mark">bilibili</span><span>从一个链接开始收藏</span><span class="local-badge"><ShieldCheck :size="12" />本地下载</span></div>
          <form class="link-form" @submit.prevent="analyze">
            <Link2 :size="19" class="link-icon" />
            <input ref="linkInput" v-model="link" aria-label="B站视频链接" placeholder="粘贴 B站视频链接、BV号，或 b23.tv 分享链接…" :disabled="analyzing" spellcheck="false" />
            <button class="paste-button" type="button" title="粘贴剪贴板链接" @click="paste"><ClipboardPaste :size="15" /><span>粘贴</span></button>
            <button class="button primary analyze-button" type="submit" :disabled="analyzing"><LoaderCircle v-if="analyzing" :size="16" class="spin" /><Download v-else :size="16" />{{ analyzing ? '正在解析' : '解析视频' }}</button>
          </form>
          <div class="link-card-footer"><span><Check :size="13" />单视频 / 多P批量</span><span><Check :size="13" />视频 / 音频 / 字幕 / 弹幕</span><span class="shortcut-tip">粘贴链接后按 <kbd>Enter</kbd> 解析</span></div>
        </section>

        <section class="task-section">
          <div class="section-heading">
            <div class="section-title"><h2>{{ view === 'active' ? '下载列表' : viewTitle }}</h2><span class="count-pill">{{ currentTasks.length }}</span></div>
            <div class="list-tools">
              <label v-if="tasks.length" class="search-box"><Search :size="14" /><input v-model="query" aria-label="搜索任务" placeholder="搜索任务" /></label>
              <label v-if="view === 'active'" class="filter-select"><SlidersHorizontal :size="14" /><select v-model="filter" aria-label="筛选下载状态"><option value="all">全部状态</option><option value="downloading">进行中</option><option value="paused">已暂停</option><option value="failed">失败</option></select><ChevronDown :size="12" /></label>
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
              <p>{{ query || filter !== 'all' ? '试试其他关键词，或调整筛选条件。' : view === 'completed' ? '视频下载完成后，会自动出现在这里。' : '粘贴一个B站链接，把喜欢的内容留在本地。' }}</p>
              <button v-if="!query && filter === 'all'" class="button empty-button" @click="focusLink"><Plus :size="15" />添加第一个下载</button>
            </div>

            <div v-else class="task-list">
              <article v-for="task in currentTasks" :key="task.id" class="task-row">
                <input type="checkbox" :aria-label="`选择 ${task.title}`" :checked="selectedTasks.includes(task.id)" @change="toggleTask(task.id)" />
                <div class="task-cover"><img v-if="task.cover" :src="task.cover" alt="" loading="lazy" referrerpolicy="no-referrer" /><Film v-else :size="24" /><span>{{ task.format === 'audio' ? '音频' : `P${task.page}` }}</span></div>
                <div class="task-info">
                  <div class="task-title-line"><h3 :title="task.title">{{ task.title }}</h3><span :class="['status-tag', task.status]">{{ statusLabels[task.status] }}</span></div>
                  <div class="task-meta"><span v-if="task.format === 'audio'"><Music2 :size="11" />M4A</span><span v-else>MP4 · {{ task.qualityLabel }}</span><i></i><span class="page-title" :title="task.pageTitle">P{{ task.page }} · {{ task.pageTitle }}</span></div>
                  <div v-if="['downloading', 'merging', 'paused', 'queued'].includes(task.status)" class="progress-track"><div :class="{ paused: task.status === 'paused' }" :style="{ width: `${Math.min(task.progress, 100)}%` }"></div></div>
                  <div class="task-detail"><span v-if="task.error" class="task-error" :title="task.error">{{ task.error }}</span><span v-else-if="task.warning" class="task-warning" :title="task.warning">{{ task.warning }}</span><span v-else-if="task.status === 'completed'">{{ bytes(task.totalBytes) }}<span class="detail-separator">·</span>已保存到下载目录</span><span v-else>{{ bytes(task.downloadedBytes) }}<template v-if="task.totalBytes"> / {{ bytes(task.totalBytes) }}</template></span><span v-if="task.status === 'downloading'" class="task-speed">{{ bytes(task.speed) }}/s<span class="detail-separator">·</span>{{ task.progress.toFixed(1) }}%</span><span v-else-if="task.status === 'merging'">正在生成完整文件…</span></div>
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

      <footer class="statusbar"><span><span :class="['status-dot', { warning: !snapshot?.mediaReady }]"></span>{{ !api.desktop ? '浏览器界面预览 · 下载请使用桌面客户端' : snapshot?.mediaReady ? '下载引擎已就绪' : '正在检查音视频组件' }}</span><div><span><ArrowDown :size="12" />{{ bytes(totalSpeed) }}/s</span><span>{{ runningTasks.length }} 个进行中</span><button @click="dialog = 'about'">v{{ snapshot?.version ?? '0.1.1' }}</button></div></footer>
    </div>

    <Transition name="toast"><div v-if="toast" :class="['toast-message', toast.type]" role="status"><CircleAlert v-if="toast.type === 'error'" :size="18" /><CircleCheck v-else-if="toast.type === 'success'" :size="18" /><Info v-else :size="18" /><span>{{ toast.text }}</span><button aria-label="关闭提示" @click="toast = null"><X :size="15" /></button></div></Transition>

    <div v-if="video" class="modal-overlay" @mousedown.self="!submitting && (video = null)">
      <section class="modal video-modal" role="dialog" aria-modal="true" aria-labelledby="video-dialog-title">
        <div class="modal-header"><div><span class="eyebrow">NEW COLLECTION</span><h2 id="video-dialog-title">准备好收藏了吗？</h2></div><button class="icon-button" aria-label="关闭视频选择" :disabled="submitting" @click="video = null"><X :size="20" /></button></div>
        <div class="video-summary"><img :src="video.cover" alt="视频封面" referrerpolicy="no-referrer" /><div><h3>{{ video.title }}</h3><p>{{ video.author }}<span>·</span>{{ duration(video.duration) }}<span>·</span>{{ video.pages.length }} 个分P</p><small>{{ video.bvid }}</small></div></div>
        <div class="format-grid"><label :class="['format-option', { chosen: format === 'video' }]"><input v-model="format" type="radio" value="video" /><Film :size="21" /><span><strong>视频</strong><small>MP4 · 自动合并音视频</small></span><CircleCheck v-if="format === 'video'" :size="17" /></label><label :class="['format-option', { chosen: format === 'audio' }]"><input v-model="format" type="radio" value="audio" /><Music2 :size="21" /><span><strong>仅音频</strong><small>M4A · 保留原始音质</small></span><CircleCheck v-if="format === 'audio'" :size="17" /></label></div>
        <label v-if="format === 'video'" class="field-row"><span>视频画质</span><select v-model="quality"><option v-for="item in video.qualities" :key="item.id" :value="item.id">{{ item.label }}{{ item.width ? ` · ${item.width} × ${item.height}` : '' }}</option></select></label>
        <p class="quality-hint"><Info :size="13" />仅展示当前可用画质。可在设置中扫码登录B站后重新解析。</p>
        <div class="pages-heading"><strong>选择分P <span>{{ selectedPages.length }} / {{ video.pages.length }}</span></strong><button class="text-button" @click="selectedPages = allPagesSelected ? [] : video.pages.map(p => p.cid)"><CheckCheck :size="14" />{{ allPagesSelected ? '取消全选' : '全选分P' }}</button></div>
        <div class="pages-list"><label v-for="page in video.pages" :key="page.cid" :class="['page-option', { checked: selectedPages.includes(page.cid) }]"><input type="checkbox" :checked="selectedPages.includes(page.cid)" @change="togglePage(page.cid)" /><span class="page-number">P{{ page.page }}</span><span class="part-title">{{ page.part }}</span><small>{{ duration(page.duration) }}</small></label></div>
        <div class="modal-footer"><span><FolderOpen :size="14" />保存到本地下载目录</span><button class="button primary" :disabled="!selectedPages.length || submitting" @click="submit"><LoaderCircle v-if="submitting" :size="16" class="spin" /><Download v-else :size="16" />下载 {{ selectedPages.length }} 项</button></div>
      </section>
    </div>

    <div v-if="dialog === 'settings'" class="modal-overlay" @mousedown.self="dialog = null">
      <section class="modal settings-modal" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <div class="modal-header"><div><span class="eyebrow">MAKE IT YOURS</span><h2 id="settings-title">偏好设置</h2></div><button class="icon-button" aria-label="关闭设置" @click="dialog = null"><X :size="20" /></button></div>
        <div class="settings-tabs"><button :class="{ active: settingsTab === 'download' }" @click="settingsTab = 'download'"><SlidersHorizontal :size="15" />下载设置</button><button :class="{ active: settingsTab === 'account' }" @click="settingsTab = 'account'"><UserRound :size="15" />B站账号</button></div>
        <div v-if="settingsTab === 'download'" class="settings-content">
          <label class="field-label">下载位置</label><div class="folder-field"><FolderOpen :size="17" /><input v-model="settingsDraft.downloadDir" placeholder="选择本地文件夹" aria-label="下载位置" /><button @click="chooseFolder">浏览</button></div>
          <div class="settings-two-col"><label><span class="field-label">同时下载任务数</span><select v-model.number="settingsDraft.concurrency"><option v-for="n in 4" :key="n" :value="n">{{ n }} 个任务</option></select></label><label><span class="field-label">单任务限速（MB/s）</span><input v-model.number="settingsDraft.maxSpeedMbps" type="number" min="0" max="1024" /><small>设为 0 表示不限速</small></label></div>
          <div class="settings-divider"></div><h3 class="settings-section-title">顺便一起收藏</h3><p class="settings-description">可用时，与视频保存在同一文件夹。</p>
          <label class="toggle-row"><span><Film :size="17" /><span>保存视频封面<small>原始封面图片</small></span></span><input v-model="settingsDraft.saveCover" type="checkbox" role="switch" /></label>
          <label class="toggle-row"><span><FileText :size="17" /><span>保存弹幕<small>XML 格式</small></span></span><input v-model="settingsDraft.saveDanmaku" type="checkbox" role="switch" /></label>
          <label class="toggle-row"><span><FileText :size="17" /><span>保存字幕<small>SRT 格式，取决于视频是否提供字幕</small></span></span><input v-model="settingsDraft.saveSubtitles" type="checkbox" role="switch" /></label>
          <div :class="['component-status', { missing: !snapshot?.mediaReady }]"><CircleCheck v-if="snapshot?.mediaReady" :size="16" /><CircleAlert v-else :size="16" /><span>{{ snapshot?.mediaReady ? 'FFmpeg 音视频组件已就绪' : api.desktop ? '未找到 FFmpeg，请重新安装完整客户端。' : '界面预览模式，桌面端会检查 FFmpeg 组件。' }}</span></div>
        </div>
        <div v-else class="settings-content account-content">
          <template v-if="snapshot?.authenticated && !loginQr && loginState === 'idle'">
            <div class="account-illustration account-avatar"><img v-if="snapshot.auth.account?.avatar" :src="snapshot.auth.account.avatar" alt="B站头像" referrerpolicy="no-referrer" /><UserRound v-else :size="25" /></div>
            <h3>{{ snapshot.auth.account?.name || 'B站账号' }}</h3>
            <span class="login-saved"><ShieldCheck :size="14" />登录状态已加密保存</span>
            <p>下次打开 SnowDown 时自动恢复。画质和字幕以此账号可用的权限为准。</p>
            <p v-if="snapshot.auth.state === 'checking'" class="account-tip">正在验证登录状态…</p>
            <p v-if="snapshot.auth.message" class="account-tip">{{ snapshot.auth.message }}</p>
            <div class="account-buttons"><button class="button secondary" :disabled="savingSettings" @click="logout">退出账号</button><button class="button secondary" :disabled="savingSettings" @click="checkLogin">检查登录</button></div>
          </template>
          <template v-else>
            <div class="account-illustration"><QrCode :size="25" /></div>
            <h3>B站官方扫码登录</h3>
            <p>使用手机 B站扫描二维码，在 B站内确认登录。无需填写密码或复制 Cookie。</p>
            <div class="login-qr" :aria-busy="loginState === 'loading'">
              <img v-if="loginQr" :src="loginQr.image" alt="B站官方登录二维码" :class="{ faded: ['expired', 'error'].includes(loginState) }" />
              <LoaderCircle v-else-if="loginState === 'loading'" class="spin" :size="30" />
              <QrCode v-else :size="82" :stroke-width="1" />
              <div v-if="['expired', 'error'].includes(loginState)" class="qr-expired"><button class="button primary" @click="startLogin"><RotateCcw :size="15" />刷新二维码</button></div>
            </div>
            <p class="qr-status" role="status">{{ loginState === 'loading' ? '正在获取官方二维码…' : loginState === 'waiting' ? '请用手机 B站扫码' : loginState === 'scanned' ? '已扫码，请在手机 B站确认登录' : loginState === 'expired' ? '二维码已过期，请刷新后重试' : loginState === 'error' ? loginError : snapshot?.auth.message || '确认登录后，将自动记住此账号' }}</p>
            <button v-if="loginState === 'idle'" class="button primary login-start" @click="startLogin"><QrCode :size="16" />获取登录二维码</button>
            <button v-if="snapshot?.authenticated" class="text-button login-back" @click="stopLogin">返回当前账号</button>
          </template>
          <div class="login-privacy"><ShieldCheck :size="16" /><span>登录状态由 Windows 加密，仅供当前系统用户使用。B站会话失效后需重新扫码。</span></div>
        </div>
        <div v-if="settingsTab === 'download'" class="modal-footer"><span>设置适用于之后创建的新任务</span><button class="button primary" :disabled="savingSettings" @click="saveSettings"><Check :size="16" />保存设置</button></div>
      </section>
    </div>

    <div v-if="dialog === 'about'" class="modal-overlay" @mousedown.self="dialog = null"><section class="modal about-modal" role="dialog" aria-modal="true" aria-labelledby="about-title"><button class="icon-button about-close" aria-label="关闭关于" @click="dialog = null"><X :size="20" /></button><span class="brand-mark about-mark"><Snowflake :size="33" /></span><h2 id="about-title">SnowDown<span>.</span></h2><p>好内容，慢慢收藏。</p><span class="version-tag">Windows · v{{ snapshot?.version ?? '0.1.1' }}</span><div class="about-description">专注B站视频的本地下载工具。<br />视频、音频、字幕与弹幕，一处收藏。</div><div class="about-tech"><ShieldCheck :size="14" />本机解析与下载<span>·</span>Rust + Tauri + Vue</div><button class="button secondary" @click="dialog = null">开始收藏<ExternalLink :size="14" /></button></section></div>
  </div>
</template>
