<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { ArrowDownToLine, ArrowUpRight, Check, ChevronRight, CircleCheck, FileArchive, FolderInput, FolderOpen, Pin, Puzzle, ToggleRight, X } from '@lucide/vue'
import * as api from '../api'
import { errorMessage } from '../format'
const emit = defineEmits<{ visibility: [visible: boolean] }>()
const browsers = ref<{ id: string; name: string; installed: boolean; connected: boolean }[]>([])
const visible = ref(false), busy = ref(false), exporting = ref(false), error = ref(''), savedZip = ref('')
const steps = [
  { icon: FileArchive, hint: '下载插件 ZIP 到本机' },
  { icon: FolderOpen, hint: '解压 ZIP，保留完整插件文件夹' },
  { icon: ToggleRight, hint: '打开浏览器扩展管理页，开启开发者模式' },
  { icon: FolderInput, hint: '点击加载已解压的扩展，选择包含 manifest.json 的文件夹' },
  { icon: Pin, hint: '在浏览器工具栏固定 SnowDown 图标，首次连接自动绑定' },
]
let timer: ReturnType<typeof setInterval> | undefined, disposed = false
async function refresh() { if (!api.desktop) return; try { const value = await api.browserStatus(); if (!disposed) browsers.value = value } catch { /* The local ZIP remains available if browser detection fails. */ } }
function open() { visible.value = true; emit('visibility', true); void refresh() }
function close() { visible.value = false; emit('visibility', false) }
async function install(id: string) {
  busy.value = true; error.value = ''
  try { await api.installBrowserExtension(id) } catch (e) { error.value = errorMessage(e) }
  finally { busy.value = false; await refresh() }
}
async function downloadZip() {
  if (exporting.value) return
  exporting.value = true; error.value = ''
  try { const path = await api.saveBrowserExtensionZip(); if (path && !disposed) savedZip.value = path }
  catch (e) { if (!disposed) error.value = errorMessage(e) }
  finally { exporting.value = false }
}
onMounted(() => { void refresh(); timer = setInterval(() => void refresh(), 5000) })
onUnmounted(() => { disposed = true; clearInterval(timer) })
</script>

<template>
  <button class="extension-tile" @click="open"><span class="extension-symbol"><Puzzle :size="19" /></span><strong>浏览器插件</strong><small>{{ browsers.some(browser => browser.connected) ? '已连接 · 浏览器账号同步' : '网页发现，轻松收藏' }}</small><span class="extension-link">{{ browsers.some(browser => browser.connected) ? '管理插件' : '安装并连接' }}<ArrowUpRight :size="12" /></span></button>
  <Teleport to="body">
    <div v-if="visible" class="modal-overlay" @mousedown.self="close" @keydown.esc="close">
      <section class="modal extension-modal" role="dialog" aria-modal="true" aria-labelledby="extension-title">
        <div class="modal-header"><div><span class="eyebrow">SNOWDOWN FOR YOUR BROWSER</span><h2 id="extension-title">连接你的浏览器</h2></div><button class="icon-button" aria-label="关闭插件设置" @click="close"><X :size="19" /></button></div>
        <div class="extension-body">
          <p class="extension-intro">视频、图像一处收藏。插件 v0.1.0 可独立使用，也可将下载交给桌面端。</p>
          <div v-for="browser in browsers" :key="browser.id" class="browser-choice">
            <div><strong>{{ browser.name }}</strong><small><Check v-if="browser.connected" :size="12" />{{ browser.connected ? '已连接 SnowDown' : browser.installed ? '已检测到浏览器' : '未安装' }}</small></div>
            <button class="button" :class="browser.connected ? 'secondary' : 'primary'" :disabled="busy" @click="install(browser.id)">{{ browser.installed ? browser.connected ? '管理插件' : '安装插件' : '下载浏览器' }}</button>
          </div>
          <section class="extension-package" aria-label="插件 ZIP 本地安装">
            <span class="package-icon"><FileArchive :size="25" /></span>
            <div class="package-copy"><strong>插件 ZIP</strong><small>{{ savedZip ? '已保存到本机' : '无法联动安装时，使用本地安装包' }}</small></div>
            <button class="button secondary package-download" :disabled="exporting" @click="downloadZip"><CircleCheck v-if="savedZip && !exporting" :size="15" /><ArrowDownToLine v-else :size="15" />{{ exporting ? '正在保存…' : '下载 ZIP' }}</button>
          </section>
          <ol class="extension-flow" aria-label="插件本地安装图解">
            <li v-for="(step, index) in steps" :key="step.hint">
              <span class="flow-stage" tabindex="0" :aria-label="step.hint">
                <span class="flow-window" aria-hidden="true"><span class="flow-window-bar"><i></i><i></i><i></i></span><component :is="step.icon" :size="24" :stroke-width="1.65" /></span>
                <span class="flow-tooltip" role="tooltip">{{ step.hint }}</span>
              </span>
              <ChevronRight v-if="index < steps.length - 1" class="flow-arrow" :size="14" aria-hidden="true" />
            </li>
          </ol>
          <p v-if="error" class="task-error" role="alert">{{ error }}</p>
        </div>
        <div class="modal-footer"><span>支持 Google Chrome 与 Microsoft Edge</span><button class="button secondary" @click="close">完成</button></div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.extension-tile{margin:17px 10px 7px;padding:13px 15px;text-align:left;border:1px solid #dfebfb;border-radius:13px;background:linear-gradient(135deg,#f7fbff,#edf4ff);color:#6887b3;display:grid;grid-template-columns:26px 1fr;gap:5px 7px;flex-shrink:0}.extension-symbol{grid-row:span 2;color:#76a5e6;align-self:center}.extension-tile strong{font-size:12px;font-weight:550}.extension-tile small{font-size:9px;color:#98adca}.extension-link{grid-column:2;display:flex;gap:5px;align-items:center;font-size:10px;margin-top:4px;color:#4d8ce7}
.extension-modal{width:550px}.extension-body{padding:0 28px 20px}.extension-intro{font-size:11px;line-height:1.9;color:#90a4bf}.browser-choice{display:flex;justify-content:space-between;align-items:center;padding:15px 0;border-bottom:1px solid #eaf0f8}.browser-choice strong{font-size:13px;color:#607d9f}.browser-choice small{display:flex;gap:4px;font-size:10px;color:#98acc6;margin-top:7px}
.extension-package{display:flex;align-items:center;gap:13px;background:#f5f9ff;border:1px solid #e2ecfa;border-radius:12px;padding:17px 15px;margin-top:20px}.package-icon{display:flex;align-items:center;justify-content:center;flex-shrink:0;width:44px;height:48px;border-radius:11px;background:#eaf2ff;color:#77a1d9}.package-copy{min-width:0;flex:1;display:flex;flex-direction:column;gap:6px}.package-copy strong{font-size:12px;font-weight:600;color:#5b7fae}.package-copy small{font-size:9px;color:#92a8c5;line-height:1.6}.package-download{font-size:10px;gap:6px;padding:9px 11px;flex-shrink:0}
.extension-flow{list-style:none;margin:22px 0 3px;padding:0;display:flex;align-items:center;justify-content:space-between;gap:4px}.extension-flow li{display:flex;align-items:center;flex:1;min-width:0;gap:5px}.extension-flow li:last-child{flex:0 0 70px}.flow-stage{position:relative;display:flex;justify-content:center;align-items:center;width:70px;height:75px;border-radius:12px;outline-offset:3px}.flow-stage:focus-visible{outline:2px solid #8ab7ff}.flow-window{position:relative;display:flex;justify-content:center;align-items:center;padding-top:8px;width:55px;height:48px;border:1px solid #dce8f7;background:linear-gradient(150deg,#fff,#f3f8ff);border-radius:9px;color:#82a9dc;overflow:hidden}.flow-window-bar{position:absolute;top:0;left:0;right:0;height:10px;border-bottom:1px solid #e6eef9;display:flex;align-items:center;gap:2px;padding-left:5px}.flow-window-bar i{width:2px;height:2px;border-radius:50%;background:#b6cce6}.flow-arrow{margin-left:auto;color:#b4c8e2;flex-shrink:0}.flow-tooltip{position:absolute;bottom:calc(100% + 4px);left:50%;transform:translateX(-50%) translateY(3px);width:128px;padding:8px 10px;background:#fff;border:1px solid #e1eaf7;border-radius:8px;box-shadow:0 4px 14px #456a9610;line-height:1.7;font-size:9px;color:#7891b3;text-align:center;opacity:0;visibility:hidden;pointer-events:none;transition:opacity .15s,transform .15s;z-index:5}.flow-stage:hover .flow-tooltip,.flow-stage:focus .flow-tooltip{opacity:1;visibility:visible;transform:translateX(-50%) translateY(0)}.extension-flow li:first-child .flow-tooltip{left:0;transform:translateY(3px)}.extension-flow li:first-child .flow-stage:hover .flow-tooltip,.extension-flow li:first-child .flow-stage:focus .flow-tooltip{transform:translateY(0)}.extension-flow li:last-child .flow-tooltip{left:auto;right:0;transform:translateY(3px)}.extension-flow li:last-child .flow-stage:hover .flow-tooltip,.extension-flow li:last-child .flow-stage:focus .flow-tooltip{transform:translateY(0)}

.flow-stage{transition:background .22s ease,box-shadow .24s ease}
.flow-window{transition:transform .24s cubic-bezier(.2,.8,.2,1),box-shadow .24s ease,border-color .22s ease,color .22s ease,background .22s ease;will-change:transform}
.flow-stage:hover,.flow-stage:focus-visible{background:#f6faff;box-shadow:0 3px 13px #5b8fce09}
.flow-stage:hover .flow-window,.flow-stage:focus-visible .flow-window{transform:translateY(-4px) scale(1.04);border-color:#bdd4f4;background:#fff;color:#538bd8;box-shadow:0 9px 18px #6b9bdd22}
@media(prefers-reduced-motion:reduce){.flow-stage,.flow-window,.flow-tooltip{transition:none}.flow-stage:hover .flow-window,.flow-stage:focus-visible .flow-window{transform:none}}
</style>
