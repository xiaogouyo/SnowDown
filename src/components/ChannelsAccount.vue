<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ExternalLink, RotateCcw, LogOut, ShieldCheck, Check, LoaderCircle } from '@lucide/vue'
import * as api from '../api'
const props=defineProps<{input?:string}>()
const status=ref<api.ChannelsStatus>({saved:false,state:'loggedout',message:'请在腾讯元宝官方页面完成登录。'})
const busy=ref(''),error=ref('')
async function action(name:'open'|'save'|'check'|'logout'){
 if(busy.value)return
 busy.value=name;error.value=''
 try{status.value=await api.channelsAccount(name,props.input);if(name==='save'&&props.input)status.value=await api.channelsAccount('check',props.input)}catch(e){error.value=e instanceof Error?e.message:String(e)}finally{busy.value=''}
}
onMounted(async()=>{try{status.value=await api.channelsAccount('status')}catch(e){error.value=String(e)}})
</script>
<template>
 <section class="channels-account" aria-label="视频号解析会话">
  <div class="account-illustration"><ShieldCheck :size="28"/></div>
  <h3>视频号解析会话</h3>
  <span class="session-saved" v-if="status.saved"><ShieldCheck :size="15"/>会话已加密保存在本机</span>
  <p>通过腾讯元宝官方服务解析你提交的视频号链接。<br/>请在官方页面完成登录，再返回保存会话。</p>
  <div class="channels-account-actions">
   <button class="button primary" :disabled="!!busy" @click="action('open')"><ExternalLink :size="16"/>打开腾讯元宝官方登录</button>
   <button class="button secondary" :disabled="!!busy" @click="action('save')"><LoaderCircle v-if="busy==='save'" :size="16" class="spin"/><Check v-else :size="16"/>保存会话</button>
  </div>
  <div class="channels-account-actions" v-if="status.saved">
   <button class="button secondary" :disabled="!!busy" @click="action('check')"><RotateCcw :size="15"/>检查解析会话</button>
   <button class="button secondary" :disabled="!!busy" @click="action('logout')"><LogOut :size="15"/>退出登录</button>
  </div>
  <p class="channels-account-message" :class="{error:!!error}" role="status">{{error||status.message}}</p>
  <div class="account-note"><ShieldCheck :size="18"/><span>会话仅供本机解析使用，不读取现有浏览器登录信息。退出登录会清除软件内此解析会话。登录完成后，仍从主界面输入视频号链接并解析。</span></div>
 </section>
</template>
<style scoped>
.channels-account{text-align:center;padding-top:24px}.channels-account h3{margin:18px 0;color:#526f98}.channels-account>p{color:#8da3c0;line-height:1.8;margin:18px auto;max-width:560px}.channels-account-actions{display:flex;justify-content:center;gap:12px;flex-wrap:wrap;margin:14px 0}.channels-account-message{font-size:12px;overflow-wrap:anywhere}.channels-account-message.error{color:#d05f70}.channels-account .account-note{display:flex;align-items:flex-start;gap:10px;text-align:left;margin-top:20px;padding:14px 16px;border:1px solid #e0eafa;background:#f5f9ff;border-radius:10px;color:#8aa0bf;font-size:11px;line-height:1.8}.channels-account .account-note>svg{flex-shrink:0;color:#7da9e3;margin-top:2px}.channels-account .session-saved{display:inline-flex;gap:6px;align-items:center;background:#ebf7f2;color:#5baf91;padding:8px 12px;border-radius:8px;font-size:12px}
</style>
