<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { CircleAlert, CircleCheck, Download, FolderOpen, Layers, Link2, LoaderCircle, X } from '@lucide/vue'
import * as api from '../api'
import { batchItem, batchLimit, qualityOptions } from '../batch'
import { errorMessage } from '../format'
import { identifySources, platformLabels, sourceSummary } from '../sources'
import type { BatchResult, CreateTaskRequest, OutputFormat, Settings, SourceLink } from '../types'
import UiSelect from './UiSelect.vue'

const props = defineProps<{ settings: Settings }>()
const emit = defineEmits<{ close: []; queued: [result: BatchResult] }>()
const root = ref<HTMLElement>()
const input = ref<HTMLTextAreaElement>()
const text = ref('')
const format = ref<OutputFormat>('video')
const maxResolution = ref(0)
const allPages = ref(false)
const options = reactive({ downloadDir: props.settings.downloadDir, intervalSecs: 3, saveCover: props.settings.saveCover, saveDanmaku: props.settings.saveDanmaku, saveSubtitles: props.settings.saveSubtitles })
const detected = computed(() => identifySources(text.value))
const phase = ref<'idle' | 'parsing' | 'queueing'>('idle')
const busy = computed(() => phase.value !== 'idle')
const error = ref('')
type Row = { source: SourceLink; state: 'waiting' | 'parsing' | 'ready' | 'failed' | 'queued'; title: string; message: string; item?: CreateTaskRequest }
const rows = ref<Row[]>([])
const progress = ref(0)
let generation = 0

async function chooseFolder() {
  try { const directory = await api.chooseDirectory(); if (directory) options.downloadDir = directory }
  catch (cause) { error.value = errorMessage(cause) }
}

function close() {
  if (phase.value === 'queueing') return
  generation++
  emit('close')
}

async function download() {
  if (busy.value) return
  error.value = ''
  const sources = detected.value.sources
  if (!sources.length) { error.value = '请粘贴至少一个支持的视频链接。'; input.value?.focus(); return }
  if (sources.length > batchLimit) { error.value = `每批最多 ${batchLimit} 个链接，请分批添加。`; return }
  if (!options.downloadDir.trim()) { error.value = '请选择保存目录。'; return }
  if (!Number.isInteger(options.intervalSecs) || options.intervalSecs < 0 || options.intervalSecs > 3600) { error.value = '任务启动间隔应为 0 至 3600 秒的整数。'; return }
  const current = ++generation
  phase.value = 'parsing'; progress.value = 0
  rows.value = sources.map(source => ({ source, state: 'waiting', title: '', message: '' }))
  for (const row of rows.value) {
    if (current !== generation) return
    row.state = 'parsing'
    try {
      const video = await api.inspectVideo(row.source.url)
      if (current !== generation) return
      row.item = batchItem(video, format.value, maxResolution.value, allPages.value)
      row.title = video.title
      row.message = `${row.item.pageCids.length} 项 · ${format.value === 'audio' ? 'M4A 音频' : video.qualities.find(quality => quality.id === row.item!.quality)?.label ?? '可用画质'}`
      row.state = 'ready'
    } catch (cause) {
      if (current !== generation) return
      row.state = 'failed'; row.message = errorMessage(cause)
    }
    progress.value++
  }
  const ready = rows.value.filter(row => row.state === 'ready' && row.item)
  if (!ready.length) { phase.value = 'idle'; error.value = '本批链接均未能解析，请检查提示后重试。'; return }
  phase.value = 'queueing'
  try {
    const result = await api.createBatch({ items: ready.map(row => row.item!), options: { ...options } })
    if (current !== generation) return
    ready.forEach(row => { row.state = 'queued' })
    emit('queued', result)
    const failed = rows.value.filter(row => row.state === 'failed')
    if (!failed.length) emit('close')
    else {
      text.value = failed.map(row => row.source.url).join('\n')
      error.value = `${failed.length} 个链接解析失败，已保留在输入框中，可修改或再次点击下载。`
    }
  } catch (cause) { error.value = errorMessage(cause) }
  finally { if (current === generation) phase.value = 'idle' }
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); close() }
  if (event.key !== 'Tab') return
  const controls = [...(root.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled)') ?? [])].filter(element => element.getClientRects().length)
  const first = controls[0], last = controls.at(-1)
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
}
onMounted(async () => { await nextTick(); input.value?.focus() })
onBeforeUnmount(() => { generation++ })
</script>

<template>
  <div class="modal-overlay" @mousedown.self="close">
    <section ref="root" class="modal batch-modal" role="dialog" aria-modal="true" aria-labelledby="batch-title" @keydown="onKey">
      <div class="modal-header"><div><span class="eyebrow">COLLECT TOGETHER</span><h2 id="batch-title"><Layers :size="22" />批量下载</h2><p>多个链接，一次设置。把喜欢的内容一起收藏。</p></div><button class="icon-button" aria-label="关闭批量下载" :disabled="phase === 'queueing'" @click="close"><X :size="20" /></button></div>
      <div class="batch-body">
        <div class="batch-input-heading"><label for="batch-links"><Link2 :size="15" />视频链接</label><span>每批最多 {{ batchLimit }} 个</span></div>
        <textarea id="batch-links" ref="input" v-model="text" aria-label="批量视频链接" :disabled="busy" maxlength="65536" placeholder="粘贴B站、抖音、小红书链接或完整分享文字&#10;用换行、空格或逗号分隔，也可以混合不同平台链接。" spellcheck="false"></textarea>
        <div class="batch-detection" aria-live="polite"><span><CircleCheck :size="13" />已识别 {{ detected.sources.length }} 个链接<template v-if="detected.duplicates"> · 已去重 {{ detected.duplicates }} 个</template><template v-if="detected.unsupported"> · 忽略 {{ detected.unsupported }} 个不支持的链接</template></span><button class="text-button" :disabled="busy || !text" @click="text = ''; rows = []; error = ''">清空</button></div>

        <fieldset :disabled="busy" class="batch-options"><legend>统一下载设置</legend>
          <div class="batch-grid">
            <div class="batch-field"><span>输出格式</span><UiSelect :model-value="format" :disabled="busy" :options="[{ value: 'video', label: 'MP4 视频' }, { value: 'audio', label: 'M4A 仅音频' }]" label="批量输出格式" @update:model-value="value => format = value as OutputFormat" /></div>
            <div class="batch-field"><span>视频画质</span><UiSelect :model-value="maxResolution" :disabled="busy || format === 'audio'" :options="qualityOptions" label="批量视频画质" @update:model-value="value => maxResolution = Number(value)" /></div>
            <label class="batch-field"><span>同批任务启动间隔</span><div class="batch-number"><input v-model.number="options.intervalSecs" aria-label="任务启动间隔秒数" type="number" min="0" max="3600" step="1" /><span>秒</span></div></label>
          </div>
          <label class="batch-folder-label" for="batch-directory">保存目录</label><div class="batch-folder"><FolderOpen :size="16" /><input id="batch-directory" v-model="options.downloadDir" aria-label="批量保存目录" placeholder="选择本批任务的保存目录" /><button type="button" class="text-button" @click="chooseFolder">选择目录</button></div>
          <div class="batch-checks"><label class="checkbox-label"><input v-model="options.saveCover" type="checkbox" />保存封面</label><label class="checkbox-label"><input v-model="options.saveDanmaku" type="checkbox" />保存弹幕<span>B站</span></label><label class="checkbox-label"><input v-model="options.saveSubtitles" type="checkbox" />保存字幕<span>B站</span></label><label class="checkbox-label"><input v-model="allPages" type="checkbox" />下载全部分P<span>B站</span></label></div>
          <p class="batch-hint">画质按各视频实际可用资源选择；指定上限不可用时选最接近的较低画质，无较低画质时选最低可用画质。默认只下载链接指定的分P。以上设置仅用于本批任务。</p>
        </fieldset>

        <div v-if="rows.length" class="batch-results" aria-live="polite" aria-label="批量解析结果"><div v-for="(row, index) in rows" :key="row.source.url" :class="['batch-result', row.state]"><span class="batch-index">{{ index + 1 }}</span><LoaderCircle v-if="row.state === 'parsing'" :size="15" class="spin" /><CircleAlert v-else-if="row.state === 'failed'" :size="15" /><CircleCheck v-else :size="15" /><div><strong>{{ row.title || sourceSummary(row.source) }}</strong><small>{{ platformLabels[row.source.platform] }} · {{ row.state === 'queued' ? '已处理，重复任务自动跳过' : row.state === 'waiting' ? '等待解析' : row.state === 'parsing' ? '正在解析…' : row.message }}</small></div></div></div>
        <p v-if="error" class="batch-error" role="alert"><CircleAlert :size="15" />{{ error }}</p>
      </div>
      <div class="modal-footer"><span>{{ phase === 'parsing' ? `正在解析 ${progress} / ${rows.length}，关闭可取消本次操作` : `全局并发 ${settings.concurrency} 个 · 间隔 0 秒即不等待` }}</span><button class="button primary batch-submit" :disabled="busy || !detected.sources.length || detected.sources.length > batchLimit" @click="download"><LoaderCircle v-if="busy" :size="16" class="spin" /><Download v-else :size="16" />{{ phase === 'parsing' ? '正在解析' : phase === 'queueing' ? '正在加入队列' : `一键下载${detected.sources.length ? ` ${detected.sources.length} 个链接` : ''}` }}</button></div>
    </section>
  </div>
</template>

<style scoped>
.batch-modal{width:min(790px,calc(100vw - 48px));max-height:calc(100vh - 64px);display:flex;flex-direction:column;padding:26px 28px 0;overflow:hidden}.modal-header{flex-shrink:0;margin-bottom:20px}.modal-header h2{display:flex;gap:9px;align-items:center}.modal-header h2 svg{color:var(--blue)}.modal-header p{font-size:12px;color:#8b99ad;margin:9px 0 0}.batch-body{overflow-y:auto;padding:0 3px 6px}.batch-input-heading{display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;font-size:12px}.batch-input-heading label{display:flex;gap:7px;align-items:center;font-weight:600}.batch-input-heading>span{color:#9aa7b9;font-size:11px}textarea{display:block;width:100%;min-height:135px;resize:vertical;max-height:240px;padding:13px 15px;border:1px solid #dce5f1;border-radius:9px;font-family:inherit;line-height:1.8;font-size:12px;color:#405470;background:#fbfcfe;overflow-wrap:anywhere}textarea::placeholder{color:#99a7ba}textarea:focus-visible{outline:2px solid #a7c8fa;outline-offset:1px}.batch-detection{display:flex;align-items:center;justify-content:space-between;gap:10px;margin:9px 0 18px;color:#8c9db1;font-size:11px}.batch-detection>span{display:flex;gap:5px;align-items:center;flex-wrap:wrap}.batch-detection svg{color:#7daf9b}.batch-options{margin:0;padding:14px 16px 12px;border:1px solid #e6edf6;border-radius:10px;background:#f8faff;min-width:0}.batch-options legend{font-size:12px;font-weight:600;padding:0 7px;color:#586d8c}.batch-grid{display:grid;grid-template-columns:1fr 1.1fr 1fr;gap:14px}.batch-field{display:flex;flex-direction:column;gap:9px;min-width:0}.batch-field>span,.batch-folder-label{font-size:11px;color:#7e8fa6}.batch-field :deep(.ui-select){width:100%;min-width:0;background:white}.batch-number,.batch-folder{display:flex;align-items:center;gap:8px;border:1px solid #dfe7f2;border-radius:7px;background:#fff;padding:9px 11px}.batch-number input,.batch-folder input{border:0;outline:none;min-width:0;width:100%;font:inherit;background:transparent;color:#405470;font-size:12px}.batch-number>span{color:#95a3b7;font-size:11px}.batch-folder-label{display:block;margin:15px 0 9px}.batch-folder{color:#97abc4}.batch-folder .text-button{flex-shrink:0}.batch-checks{display:flex;align-items:center;gap:16px;flex-wrap:wrap;margin-top:17px}.batch-checks .checkbox-label{color:#6e819b;font-size:11px;gap:6px}.batch-checks label>span{font-size:9px;color:#9daec3;background:#edf2fa;padding:1px 4px;border-radius:3px}.batch-hint{color:#9aa9bd;font-size:10px;line-height:1.8;margin:13px 0 0}.batch-results{margin-top:16px;max-height:190px;overflow-y:auto;border:1px solid #e7edf5;border-radius:9px}.batch-result{display:flex;gap:9px;padding:11px 12px;align-items:center;color:#99abc1}.batch-result+.batch-result{border-top:1px solid #edf1f7}.batch-index{font-size:10px;min-width:15px}.batch-result>div{display:flex;flex-direction:column;gap:4px;min-width:0}.batch-result strong{font-size:11px;font-weight:500;color:#526886;overflow-wrap:anywhere}.batch-result small{font-size:10px;line-height:1.6;overflow-wrap:anywhere}.batch-result.failed,.batch-result.failed small{color:#c57878}.batch-result.ready>svg,.batch-result.queued>svg{color:#76a994}.batch-error{display:flex;align-items:flex-start;gap:7px;color:#bc6e6e;font-size:11px;line-height:1.7;margin:13px 0 6px}.batch-error svg{margin-top:2px}.modal-footer{flex-shrink:0;padding:18px 0;margin-top:16px;gap:15px}.modal-footer>span{line-height:1.7;white-space:normal}.batch-submit{min-width:157px}@media(max-width:800px){.batch-grid{grid-template-columns:1fr}.batch-modal{padding:20px 20px 0}.batch-checks{gap:12px}}
.batch-modal .modal-header{padding:0}.batch-modal .modal-footer{background:#fff;border-radius:0}
</style>
