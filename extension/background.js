import {capturePage} from './page.js';
import {parseVideo} from './sites.js';
import {platformOf,platformNames,normalizeResources,safeUrl,safeName,repository,resourceKind} from './core.js';
import {contentSender,currentTicket,supportedVideoUrl} from './floating-core.js';
const browser=/Edg\//.test(navigator.userAgent)?'edge':'chrome';
let port,sequence=0,connected=false,version='',binding='',lastSession='',syncTimer;
const pending=new Map(),network=new Map(),floatTickets=new Map();
const prefs=async()=>({preferDesktop:false,syncAccounts:true,floatingEnabled:true,...await chrome.storage.local.get(['preferDesktop','syncAccounts','lastBinding','floatingEnabled'])});
async function call(method,data={}){
 if(!port){try{port=chrome.runtime.connectNative('app.snowdown.bridge');port.onMessage.addListener(value=>{const waiter=pending.get(value.id);if(waiter){pending.delete(value.id);clearTimeout(waiter.timer);waiter.resolve(value)}});port.onDisconnect.addListener(()=>{void chrome.runtime.lastError;port=null;connected=false;for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.resolve({connected:false})}pending.clear()})}catch{return {connected:false}}}
 return new Promise(resolve=>{const id=++sequence;pending.set(id,{resolve,timer:setTimeout(()=>{pending.delete(id);resolve({connected:false,error:'桌面连接超时，请稍后重试。'})},method==='hello'?2500:45000)});try{port.postMessage({id,browser,method,...data})}catch{const item=pending.get(id);clearTimeout(item?.timer);pending.delete(id);resolve({connected:false})}});
}
let heartbeatPending,lastHeartbeat=0;
async function heartbeat(force=false){if(heartbeatPending)return heartbeatPending;if(!force&&Date.now()-lastHeartbeat<1000)return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};
 heartbeatPending=checkHeartbeat().finally(()=>{heartbeatPending=null;lastHeartbeat=Date.now()});return heartbeatPending;}
async function checkHeartbeat(){const result=await call('hello');const before=connected;connected=!!result.connected;version=result.data?.version||'';binding=result.data?.binding||'';
 const settings=await prefs();if(connected&&binding&&settings.lastBinding!==binding){await chrome.storage.local.set({lastBinding:binding,preferDesktop:true,syncAccounts:true});}
 if(connected&&!before){lastSession='';void syncSessions();}
 await chrome.action.setBadgeText({text:connected?'✓':''});await chrome.action.setBadgeBackgroundColor({color:'#3478ee'});
 return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};}
async function pageFor(tabId,resources=false){const results=await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:capturePage,args:[{resources,backgrounds:resources}]});const page=results[0]?.result;if(!page)throw Error('当前页面无法读取。');return {...page,resources:resources?normalizeResources([...page.resources,...(network.get(tabId)||[])]):[]}}
async function syncSessions(){if(!connected||!(await prefs()).syncAccounts)return;const sessions=[];
 const tabs=await chrome.tabs.query({url:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*']});
 for(const platform of Object.keys(platformNames)){
  const cookies=await chrome.cookies.getAll({url:`https://www.${platform}.com/`});
  const cookie=cookies.filter(c=>!c.partitionKey).map(c=>`${c.name}=${c.value}`).join('; ');
  const required=platform==='bilibili'?'SESSDATA':platform==='douyin'?'sessionid':'web_session';if(!cookies.some(c=>c.name===required&&c.value))continue;
  let account=null;const tab=tabs.find(t=>new URL(t.url).hostname.endsWith(`${platform}.com`));if(tab)try{account=(await pageFor(tab.id)).account}catch{}
  sessions.push({platform,cookie,account});
 }
 const signature=JSON.stringify(sessions);if(signature===lastSession)return;
 const result=await call('sessions',{sessions});if(result.connected&&!result.error){lastSession=signature;await chrome.storage.local.set({syncedAt:Date.now(),syncedPlatforms:result.data?.filter(x=>x.saved).map(x=>x.platform)||[]})}
}
chrome.cookies.onChanged.addListener(change=>{if(!['bilibili.com','douyin.com','xiaohongshu.com'].some(domain=>change.cookie.domain.replace(/^\./,'').endsWith(domain)))return;clearTimeout(syncTimer);syncTimer=setTimeout(()=>void syncSessions(),1500)});
chrome.tabs.onRemoved.addListener(id=>{network.delete(id);for(const [key,value]of floatTickets)if(value.tabId===id)floatTickets.delete(key)});
chrome.tabs.onUpdated.addListener((id,change)=>{if(change.status==='loading'){network.delete(id);for(const [key,value]of floatTickets)if(value.tabId===id)floatTickets.delete(key)}});
chrome.webRequest.onHeadersReceived.addListener(details=>{if(details.tabId<0)return;const mime=details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-type')?.value||'';const kind=resourceKind(details.url,mime);if(!kind)return;const list=network.get(details.tabId)||[];list.push({url:details.url,mime,kind,size:Number(details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-length')?.value)||0});network.set(details.tabId,list.slice(-1000))},{urls:['http://*/*','https://*/*']},['responseHeaders']);
async function refererRules(){await chrome.declarativeNetRequest.updateDynamicRules({removeRuleIds:[1,2,3],addRules:[['bilibili.com',['bilivideo.com','bilivideo.cn']],['douyin.com',['douyinvod.com','douyin.com']],['xiaohongshu.com',['xhscdn.com','xiaohongshu.com']]].map(([host,domains],index)=>({id:index+1,priority:1,action:{type:'modifyHeaders',requestHeaders:[{header:'Referer',operation:'set',value:`https://www.${host}/`}]},condition:{initiatorDomains:[chrome.runtime.id],requestDomains:domains}}))})}
async function download(message){
 const settings=await prefs();if(message.browserOnly)settings.preferDesktop=false;await heartbeat();
 if(settings.preferDesktop&&connected&&message.page?.platform){if(settings.syncAccounts){lastSession='';await syncSessions();}const result=await call('queue',{url:message.page.url,detail:message.page.detail,quality:message.stream?.quality});if(result.error)throw Error(result.error);if(result.connected)return {message:'已加入 SnowDown 下载队列。'};connected=false;}
 if(message.resource&&settings.preferDesktop&&connected){const r=await call('resource',{url:message.resource.url,name:message.resource.name||new URL(message.resource.url).pathname.split('/').pop(),kind:message.resource.kind});if(r.connected&&!r.error)return {message:'已发送到 SnowDown 下载队列。'};if(r.error)message.fallbackReason=r.error;}
 if(message.resource){const url=safeUrl(message.resource.url);if(!url)throw Error('此资源无法下载。');if(message.resource.kind==='manifest')throw Error('这是分段播放清单，请使用平台视频模式或选择直接视频文件。');await chrome.downloads.download({url,filename:`SnowDown/${safeName(message.resource.name||new URL(url).pathname.split('/').pop())}`,saveAs:false});return {message:message.fallbackReason?'此资源已回退到浏览器下载。':settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已开始浏览器下载。'};}
 const selected=message.stream;
 if(!safeUrl(selected?.url)||selected.audio&&!safeUrl(selected.audio))throw Error('媒体地址无效，请重新解析。');
 const job=crypto.randomUUID();await chrome.storage.session.set({[`job:${job}`]:{stream:selected,title:message.title,createdAt:Date.now()}});
 await chrome.tabs.create({url:chrome.runtime.getURL(`job.html?id=${job}`)});
 return {message:settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已打开浏览器下载进度页。'};
}
chrome.runtime.onMessage.addListener((message,sender,sendResponse)=>{
 const ownPage=sender.id===chrome.runtime.id&&sender.url?.startsWith(chrome.runtime.getURL(''));
 // Content scripts only operate on the sender's own top-level tab. They never
 // supply cookies, native methods, arbitrary media URLs or another tab's id.
 if(!ownPage){
  if(!contentSender(sender,chrome.runtime.id)||!['float-state','float-parse','float-download','float-disable'].includes(message.type))return;
  floatingRequest(message,sender).then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'无法读取视频'}));return true;
 }
 (async()=>{
  switch(message.type){
   case 'status':return {...await heartbeat(),settings:await prefs(),...await chrome.storage.local.get(['syncedAt','syncedPlatforms'])};
   case 'page':return pageFor(message.tabId,true);
   case 'parse':{const page=await pageFor(message.tabId);return {page,video:await parseVideo(page)}};
   case 'settings':{const update={};for(const key of ['preferDesktop','syncAccounts','floatingEnabled'])if(typeof message[key]==='boolean')update[key]=message[key];await chrome.storage.local.set(update);lastSession='';void syncSessions();return heartbeat();}
   case 'sync':lastSession='';await syncSessions();return {ok:connected};
   case 'download':return download(message);
   case 'desktop-download':await chrome.tabs.create({url:repository});return {};
   default:throw Error('不支持的操作。');
  }
 })().then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'操作未完成，请重试。'}));return true;
});
async function floatingRequest(message,sender){
 if(message.type==='float-disable'){await chrome.storage.local.set({floatingEnabled:false});return {enabled:false}}
 const settings=await prefs();if(!settings.floatingEnabled)throw Error('悬浮球提醒已关闭，可在插件设置重新开启。');
 if(message.type==='float-state')return {enabled:true,platform:platformNames[platformOf(sender.url)]||null,supported:supportedVideoUrl(sender.url)};
 if(message.type==='float-parse'){
  const page=await pageFor(sender.tab.id,!platformOf(sender.url));if(page.url!==sender.url)throw Error('页面已切换，请重新读取。');
  let video,resource;
  if(page.platform)video=await parseVideo(page);
  else {resource=page.resources.find(r=>r.kind==='video'&&!/\.m4s(?:$|\?)/i.test(r.url));if(!resource)throw Error('未找到可直接下载的视频；分段或加密播放暂不支持。');video={title:page.title,author:'网页视频',streams:[{label:'原始视频',url:resource.url}]};}
  for(const [key,ticket]of floatTickets)if(Date.now()-ticket.createdAt>120000||ticket.tabId===sender.tab.id)floatTickets.delete(key);
  if(floatTickets.size>64)floatTickets.delete(floatTickets.keys().next().value);
  const id=crypto.randomUUID();floatTickets.set(id,{tabId:sender.tab.id,url:sender.url,createdAt:Date.now(),page,video,resource});
  const state=await heartbeat();
  return {id,title:video.title,author:video.author||'',cover:safeUrl(video.cover),streams:video.streams.map((s,index)=>({index,label:s.label||'原始画质'})),desktop:state.effectiveDesktop,connected:state.connected};
 }
 const ticket=floatTickets.get(message.id);if(!currentTicket(ticket,sender))throw Error('视频页面或下载设置已变化，请重新读取。');
 if(!Number.isInteger(message.index)||!ticket.video.streams[message.index])throw Error('请选择有效画质。');
 const current=await chrome.tabs.get(sender.tab.id);if(current.url!==ticket.url)throw Error('页面已切换，请重新读取。');
 const result=await download(ticket.resource?{resource:ticket.resource,browserOnly:message.route==='browser'}:{page:ticket.page,stream:ticket.video.streams[message.index],title:ticket.video.title,browserOnly:message.route==='browser'});
 floatTickets.delete(message.id);return result;
}
async function registerResourceFloat(){
 const id='snowdown-floating-resources';const allowed=await chrome.permissions.contains({origins:['http://*/*','https://*/*']});
 const registered=(await chrome.scripting.getRegisteredContentScripts()).some(x=>x.id===id);
 if(allowed&&!registered)await chrome.scripting.registerContentScripts([{id,matches:['http://*/*','https://*/*'],excludeMatches:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*'],js:['floating.js'],runAt:'document_idle',allFrames:false}]);
 if(!allowed&&registered)await chrome.scripting.unregisterContentScripts({ids:[id]});
}
chrome.permissions.onAdded.addListener(()=>void registerResourceFloat().catch(()=>{}));chrome.permissions.onRemoved.addListener(()=>void registerResourceFloat().catch(()=>{}));void registerResourceFloat().catch(()=>{});
chrome.alarms.create('connection',{periodInMinutes:.5});chrome.alarms.onAlarm.addListener(()=>void heartbeat());
setInterval(()=>void heartbeat(),5000);void refererRules();void heartbeat();
