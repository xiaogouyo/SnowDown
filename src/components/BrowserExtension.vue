<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { ArrowUpRight, Check, Puzzle, X } from '@lucide/vue'
import * as api from '../api'
import { errorMessage } from '../format'
const emit=defineEmits<{visibility:[visible:boolean]}>()
const browsers=ref<{id:string;name:string;installed:boolean;connected:boolean}[]>([])
const visible=ref(false),busy=ref(false),error=ref(''),folder=ref('')
let timer:ReturnType<typeof setInterval>|undefined,disposed=false
async function refresh(){if(!api.desktop)return;try{const value=await api.browserStatus();if(!disposed)browsers.value=value}catch{}}
function open(){visible.value=true;emit('visibility',true);void refresh()}
function close(){visible.value=false;emit('visibility',false)}
async function install(id:string){busy.value=true;error.value='';try{folder.value=await api.installBrowserExtension(id)}catch(e){error.value=errorMessage(e)}finally{busy.value=false;await refresh()}}
onMounted(()=>{void refresh();timer=setInterval(()=>void refresh(),5000)})
onUnmounted(()=>{disposed=true;clearInterval(timer)})
</script>
<template>
  <button class="extension-tile" @click="open"><span class="extension-symbol"><Puzzle :size="19" /></span><strong>浏览器插件</strong><small>{{ browsers.some(browser=>browser.connected)?'已连接 · 浏览器账号同步':'网页发现，轻松收藏' }}</small><span class="extension-link">{{ browsers.some(browser=>browser.connected)?'管理插件':'安装并连接' }}<ArrowUpRight :size="12" /></span></button>
  <Teleport to="body"><div v-if="visible" class="modal-overlay" @mousedown.self="close" @keydown.esc="close"><section class="modal extension-modal" role="dialog" aria-modal="true" aria-labelledby="extension-title"><div class="modal-header"><div><span class="eyebrow">SNOWDOWN FOR YOUR BROWSER</span><h2 id="extension-title">连接你的浏览器</h2></div><button class="icon-button" aria-label="关闭插件设置" @click="close"><X :size="19" /></button></div><div class="extension-body"><p>视频、图像一处收藏。插件 v0.1.0 可独立使用，也可将下载交给桌面端。</p><div v-for="browser in browsers" :key="browser.id" class="browser-choice"><div><strong>{{ browser.name }}</strong><small><Check v-if="browser.connected" :size="12" />{{ browser.connected?'已连接 SnowDown':browser.installed?'已检测到浏览器':'未安装' }}</small></div><button class="button" :class="browser.connected?'secondary':'primary'" :disabled="busy" @click="install(browser.id)">{{ browser.installed?browser.connected?'管理插件':'安装插件':'下载浏览器' }}</button></div><div v-if="folder" class="extension-steps"><strong>插件文件已准备，联动已注册</strong><ol><li>在已打开的扩展管理页开启“开发者模式”。</li><li>选择“加载已解压的扩展”，选择下方文件夹。</li><li>固定 SnowDown 图标。首次连接会自动绑定，并启用桌面下载和账号同步。</li></ol><code>{{ folder }}</code></div><p class="extension-note">普通 Windows 浏览器要求用户确认加载未上架扩展。此处自动准备文件和绑定配置，并打开管理页；不会修改浏览器策略。</p><p class="extension-note">浏览器账号仅在本机同步并加密保存。软件离线时，插件自动使用浏览器下载；重新打开软件后恢复连接。</p><p v-if="error" class="task-error" role="alert">{{ error }}</p></div><div class="modal-footer"><span>支持 Google Chrome 与 Microsoft Edge</span><button class="button secondary" @click="close">完成</button></div></section></div></Teleport>
</template>
<style scoped>
.extension-tile{margin:17px 10px 7px;padding:13px 15px;text-align:left;border:1px solid #dfebfb;border-radius:13px;background:linear-gradient(135deg,#f7fbff,#edf4ff);color:#6887b3;display:grid;grid-template-columns:26px 1fr;gap:5px 7px;flex-shrink:0}.extension-symbol{grid-row:span 2;color:#76a5e6;align-self:center}.extension-tile strong{font-size:12px;font-weight:550}.extension-tile small{font-size:9px;color:#98adca}.extension-link{grid-column:2;display:flex;gap:5px;align-items:center;font-size:10px;margin-top:4px;color:#4d8ce7}.extension-modal{width:530px}.extension-body{padding:0 28px 20px}.extension-body>p{font-size:11px;line-height:1.9;color:#90a4bf}.browser-choice{display:flex;justify-content:space-between;align-items:center;padding:15px 0;border-bottom:1px solid #eaf0f8}.browser-choice strong{font-size:13px;color:#607d9f}.browser-choice small{display:flex;gap:4px;font-size:10px;color:#98acc6;margin-top:7px}.extension-steps{background:#f5f9ff;border:1px solid #e2ecfa;border-radius:9px;padding:13px 15px;margin-top:17px;font-size:11px;color:#7992b2;line-height:1.9}.extension-steps strong{font-size:12px;color:#5b81b4}.extension-steps ol{padding-left:17px}.extension-steps code{display:block;overflow-wrap:anywhere;font-size:10px}.extension-body .extension-note{font-size:10px;color:#9bacc3}
</style>
