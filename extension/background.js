import {capturePage} from './page.js';
import {linkedPageMedia} from './linked-page.js';
import {genericVideo,downloadRows} from './media.js';
import {expandManifests} from './manifests.js';
import {siteOf,previewUrl} from './catalog.js';
import {parseVideo} from './sites.js';
import {platformOf,platformNames,sessionPlatforms,normalizeResources,safeUrl,safeName,repository,resourceKind} from './core.js';
import {contentSender,currentTicket,supportedVideoUrl} from './floating-core.js';
const browser=/Edg\//.test(navigator.userAgent)?'edge':'chrome';
let port,sequence=0,connected=false,version='',binding='',lastSession='',syncTimer;
const pending=new Map(),network=new Map(),floatTickets=new Map();
const prefs=async()=>({preferDesktop:false,syncAccounts:true,floatingEnabled:true,...await chrome.storage.local.get(['preferDesktop','syncAccounts','lastBinding','floatingEnabled'])});
async function call(method,data={}){
 if(!port){try{port=chrome.runtime.connectNative('app.snowdown.bridge');port.onMessage.addListener(value=>{const waiter=pending.get(value.id);if(waiter){pending.delete(value.id);clearTimeout(waiter.timer);waiter.resolve(value)}});port.onDisconnect.addListener(()=>{void chrome.runtime.lastError;port=null;connected=false;for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.resolve({connected:false})}pending.clear()})}catch{return {connected:false}}}
 return new Promise(resolve=>{const id=++sequence;pending.set(id,{resolve,timer:setTimeout(()=>{pending.delete(id);resolve({connected:false,error:'桌面连接超时，请稍后重试。'})},method==='hello'?2500:method==='inspect'||method==='queue'?65000:45000)});try{port.postMessage({id,browser,method,...data})}catch{const item=pending.get(id);clearTimeout(item?.timer);pending.delete(id);resolve({connected:false})}});
}
let heartbeatPending,lastHeartbeat=0;
async function heartbeat(force=false){if(heartbeatPending)return heartbeatPending;if(!force&&Date.now()-lastHeartbeat<1000)return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};
 heartbeatPending=checkHeartbeat().finally(()=>{heartbeatPending=null;lastHeartbeat=Date.now()});return heartbeatPending;}
async function checkHeartbeat(){const result=await call('hello');const before=connected;connected=!!result.connected;version=result.data?.version||'';binding=result.data?.binding||'';
 const settings=await prefs();if(connected&&binding&&settings.lastBinding!==binding){await chrome.storage.local.set({lastBinding:binding,preferDesktop:true,syncAccounts:true});}
 if(connected&&!before){lastSession='';void syncSessions();}
 await chrome.action.setBadgeText({text:connected?'✓':''});await chrome.action.setBadgeBackgroundColor({color:'#3478ee'});
 return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};}
async function pageFor(tabId,resources=false,targetUrl=null){const results=await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:capturePage,args:[{resources,backgrounds:resources,targetUrl}]});const page=results[0]?.result;if(!page)throw Error('当前页面无法读取。');return {...page,platform:page.platform||platformOf(page.url),site:siteOf(page.url),resources:resources?normalizeResources([...page.resources,...(network.get(tabId)||[])]):[]}}
async function resolvePage(tabId,targetUrl=null,preview=null){
 let page=await pageFor(tabId,false,targetUrl);if(preview){page.title=preview.title||page.title;page.cover=preview.cover||page.cover;}
 if(page.platform!=='web'&&page.platform){try{return {page,video:await parseVideo(page)}}catch(original){const state=await heartbeat();if(state.connected){const result=await call('inspect',{url:page.url});if(result.data?.streams?.length)return {page,video:result.data};}throw original;}}
 if(!preview&&page.singleVisibleVideo&&!page.media?.length){page.media=(network.get(tabId)||[]).filter(r=>['video','manifest'].includes(r.kind)&&!/[.](?:m4s|mpd)(?:$|\?)/.test(r.url)).slice(-8);}
 page.media=await expandManifests(page.media||[]);let video=genericVideo(page);
 if(!video&&targetUrl){try{const result=(await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:linkedPageMedia,args:[targetUrl]}))[0]?.result;if(result){page={...page,...result};video=genericVideo(page);}}catch{}}
 if(video){page.detail=video.detail;return {page,video};}
 const state=await heartbeat();if(state.connected&&page.platform==='web'){const result=await call('inspect',{url:page.url});if(result.error)throw Error(result.error);if(result.data?.streams?.length)return {page,video:result.data};}
 throw Error(page.platform==='web'?'此页面暂未提供直接视频资源。可连接 SnowDown 桌面端解析，或播放目标视频后重试。':'请打开一个视频页面，或切换到网页资源模式。');
}
async function syncSessions(){if(!connected||!(await prefs()).syncAccounts)return;const sessions=[];
 const tabs=await chrome.tabs.query({url:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*']});
 for(const platform of sessionPlatforms){
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
 if(message.stream?.native&&message.browserOnly)throw Error('此画质需要桌面端解析，不能改用浏览器直接下载。');
 const settings=await prefs();if(message.browserOnly)settings.preferDesktop=false;await heartbeat();
 const nativeVersion=version.split('.').map(Number),modern=nativeVersion[0]>0||nativeVersion[1]>2||nativeVersion[1]===2&&nativeVersion[2]>=8;
 if(!modern&&(message.format==='audio'||message.page?.platform==='web'))settings.preferDesktop=false;
 if((settings.preferDesktop||message.stream?.native)&&connected&&message.page?.platform){if(settings.syncAccounts){lastSession='';await syncSessions();}const result=await call('queue',{url:message.page.url,detail:message.page.detail,quality:message.stream?.quality,format:message.format==='audio'?'audio':'video'});if(result.error)throw Error(result.error);if(result.connected)return {message:'已加入 SnowDown 下载队列。'};connected=false;}
 if(message.resource&&settings.preferDesktop&&connected){const r=await call('resource',{url:message.resource.url,name:message.resource.name||new URL(message.resource.url).pathname.split('/').pop(),kind:message.resource.kind});if(r.connected&&!r.error)return {message:'已发送到 SnowDown 下载队列。'};if(r.error)message.fallbackReason=r.error;}
 if(message.resource){const url=safeUrl(message.resource.url);if(!url)throw Error('此资源无法下载。');if(message.resource.kind==='manifest')throw Error('这是分段播放清单，请使用平台视频模式或选择直接视频文件。');await chrome.downloads.download({url,filename:`SnowDown/${safeName(message.resource.name||new URL(url).pathname.split('/').pop())}`,saveAs:false});return {message:message.fallbackReason?'此资源已回退到浏览器下载。':settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已开始浏览器下载。'};}
 const selected=message.stream;
 if(selected?.native)throw Error('此视频需要桌面解析，请先启动 SnowDown，再重新读取。');
 if(!safeUrl(selected?.url)||selected.audio&&!safeUrl(selected.audio))throw Error('媒体地址无效，请重新解析。');
 const job=crypto.randomUUID();await chrome.storage.session.set({[`job:${job}`]:{stream:selected,title:message.title,format:message.format||'video',createdAt:Date.now()}});
 await chrome.tabs.create({url:chrome.runtime.getURL(`job.html?id=${job}`)});
 return {message:settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已打开浏览器下载进度页。'};
}
chrome.runtime.onMessage.addListener((message,sender,sendResponse)=>{
 const ownPage=sender.id===chrome.runtime.id&&sender.url?.startsWith(chrome.runtime.getURL(''));
 // Content scripts only operate on the sender's own top-level tab. They never
 // supply cookies, native methods, arbitrary media URLs or another tab's id.
 if(!ownPage){
  if(!contentSender(sender,chrome.runtime.id)||!['float-state','float-parse','float-download','float-disable','preview-parse','preview-download'].includes(message.type))return;
  floatingRequest(message,sender).then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'无法读取视频'}));return true;
 }
 (async()=>{
  switch(message.type){
   case 'status':return {...await heartbeat(),settings:await prefs(),...await chrome.storage.local.get(['syncedAt','syncedPlatforms'])};
   case 'page':return pageFor(message.tabId,true);
   case 'parse':return resolvePage(message.tabId);
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
 if(message.type==='preview-parse'){
  const target=previewUrl(message.url,sender.url);if(!target||siteOf(target)?.id!==siteOf(sender.url)?.id)throw Error('预览视频链接无效。');
  const check=(await chrome.scripting.executeScript({target:{tabId:sender.tab.id},world:'MAIN',func:url=>{const a=[...document.querySelectorAll('a[href]')].find(a=>a.href.split('#')[0]===url);if(!a)return null;const r=a.getBoundingClientRect();if(r.width<60||r.height<30||r.bottom<0||r.top>innerHeight)return null;const image=a.querySelector('img');return {title:a.title||image?.alt||a.textContent.trim().slice(0,200),cover:image?.currentSrc||image?.src||''}},args:[target]}))[0]?.result;
  if(!check&&target!==sender.url)throw Error('预览卡片已变化，请重新读取。');
  const {page,video}=await resolvePage(sender.tab.id,target,check);const id=crypto.randomUUID();
  for(const [key,value]of floatTickets)if(Date.now()-value.createdAt>120000)floatTickets.delete(key);if(floatTickets.size>64)floatTickets.delete(floatTickets.keys().next().value);
  floatTickets.set(id,{tabId:sender.tab.id,url:sender.url,sourceUrl:target,createdAt:Date.now(),page,video,preview:true});
  return {id,title:video.title,rows:downloadRows(video),connected,desktop:(await prefs()).preferDesktop&&connected};
 }
 if(message.type==='float-parse'){
  const page=await pageFor(sender.tab.id,!platformOf(sender.url)||platformOf(sender.url)==='web');if(page.url!==sender.url)throw Error('页面已切换，请重新读取。');
  let video,resource;
  if(page.platform==='web'){const result=await resolvePage(sender.tab.id);video=result.video;Object.assign(page,result.page);}
  else if(page.platform)video=await parseVideo(page);
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
 if(message.type==='preview-download'&&!ticket.preview)throw Error('预览下载已失效。');
 if(ticket.preview&&ticket.sourceUrl!==sender.url){const present=(await chrome.scripting.executeScript({target:{tabId:sender.tab.id},world:'MAIN',func:url=>[...document.querySelectorAll('a[href]')].some(a=>a.href.split('#')[0]===url),args:[ticket.sourceUrl]}))[0]?.result;if(!present)throw Error('预览卡片已切换，请重新读取。');}
 if(message.type==='preview-download'&&message.kind==='image'){
  const cover=safeUrl(ticket.video.cover);if(!cover)throw Error('没有可下载封面。');const result=await download({resource:{url:cover,kind:'image',name:safeName(ticket.video.title)+'.'+(cover.split(/[?#]/)[0].match(/\.(jpg|jpeg|png|webp|avif)$/i)?.[1]||'jpg')},browserOnly:true});floatTickets.delete(message.id);return result;
 }
 const result=await download(ticket.resource?{resource:ticket.resource,browserOnly:message.route==='browser'}:{page:ticket.page,stream:ticket.video.streams[message.index],title:ticket.video.title,format:message.kind==='audio'?'audio':'video',browserOnly:message.route==='browser'});
 floatTickets.delete(message.id);return result;
}
async function registerResourceFloat(){
 const id='snowdown-floating-resources';const allowed=await chrome.permissions.contains({origins:['http://*/*','https://*/*']});
 const registered=(await chrome.scripting.getRegisteredContentScripts()).find(x=>x.id===id);
 if(allowed&&!registered)await chrome.scripting.registerContentScripts([{id,matches:['http://*/*','https://*/*'],excludeMatches:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*'],js:['floating.js','preview.js'],runAt:'document_idle',allFrames:false}]);
 if(allowed&&registered&&!registered.js?.includes('preview.js'))await chrome.scripting.updateContentScripts([{id,js:['floating.js','preview.js']}]);
 if(!allowed&&registered)await chrome.scripting.unregisterContentScripts({ids:[id]});
}
chrome.permissions.onAdded.addListener(()=>void registerResourceFloat().catch(()=>{}));chrome.permissions.onRemoved.addListener(()=>void registerResourceFloat().catch(()=>{}));void registerResourceFloat().catch(()=>{});
chrome.alarms.create('connection',{periodInMinutes:.5});chrome.alarms.onAlarm.addListener(()=>void heartbeat());
setInterval(()=>void heartbeat(),5000);void refererRules();void heartbeat();
