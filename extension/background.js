import {cacheKey,cacheSite,authCookieSite,retryStableParse} from './cache-context.js';
import {capturePage} from './page.js';
import {linkedPageMedia} from './linked-page.js';
import {downloadRows} from './media.js';
import {expandManifests} from './manifests.js';
import {siteOf,previewUrl} from './catalog.js';
import {douyinPreviewTargets} from './douyin-preview.js';
import {youtubeId,youtubeMedia,observedYoutubeMedia} from './youtube.js';
import {biliInputs} from './sites.js';
import {ParseCache,createLimiter} from './parse-cache.js';
import {parseInWorker} from './parser-client.js';
const parseCache=new ParseCache(),parseQueue=createLimiter(2,12),captureQueue=createLimiter(1,16);
const parseVideo=async page=>parseInWorker('site',page.platform==='bilibili'?{...page,biliData:await biliInputs(page)}:page);
import {platformOf,platformNames,sessionPlatforms,normalizeResources,safeUrl,safeName,repository,resourceKind} from './core.js';
import {contentSender,currentTicket,supportedVideoUrl} from './floating-core.js';
const browser=/Edg\//.test(navigator.userAgent)?'edge':'chrome';
let port,sequence=0,connected=false,version='',binding='',lastSession='',syncTimer;
const pending=new Map(),network=new Map(),floatTickets=new Map();
const prefs=async()=>({preferDesktop:false,syncAccounts:true,floatingEnabled:true,...await chrome.storage.local.get(['preferDesktop','syncAccounts','lastBinding','floatingEnabled'])});
async function call(method,data={},deadline){
 if(!port){try{port=chrome.runtime.connectNative('app.snowdown.bridge');port.onMessage.addListener(value=>{const waiter=pending.get(value.id);if(waiter){pending.delete(value.id);clearTimeout(waiter.timer);waiter.resolve(value)}});port.onDisconnect.addListener(()=>{void chrome.runtime.lastError;port=null;if(connected)parseCache.dropWhere(value=>value.video?.streams?.some(stream=>stream.native));connected=false;for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.resolve({connected:false})}pending.clear()})}catch{return {connected:false}}}
 return new Promise(resolve=>{const id=++sequence;pending.set(id,{resolve,timer:setTimeout(()=>{pending.delete(id);resolve({connected:false,error:'解析超时，请在浏览器打开目标视频并播放后重试。'})},deadline||(method==='hello'?2500:method==='inspect'||method==='queue'?65000:45000))});try{port.postMessage({id,browser,method,...data})}catch{const item=pending.get(id);clearTimeout(item?.timer);pending.delete(id);resolve({connected:false})}});
}
let heartbeatPending,lastHeartbeat=0;
async function heartbeat(force=false){if(heartbeatPending)return heartbeatPending;if(!force&&Date.now()-lastHeartbeat<1000)return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};
 heartbeatPending=checkHeartbeat().finally(()=>{heartbeatPending=null;lastHeartbeat=Date.now()});return heartbeatPending;}
async function checkHeartbeat(){const result=await call('hello',{extensionVersion:chrome.runtime.getManifest().version,extensionRevision:'desktop-0.2.13'});const before=connected;connected=!!result.connected;version=result.data?.version||'';binding=result.data?.binding||'';
 const settings=await prefs();if(connected&&binding&&settings.lastBinding!==binding){await chrome.storage.local.set({lastBinding:binding,preferDesktop:true,syncAccounts:true});}
 if(connected&&!before){lastSession='';void syncSessions();}
 await chrome.action.setBadgeText({text:connected?'✓':''});await chrome.action.setBadgeBackgroundColor({color:'#3478ee'});
 return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};}
async function pageFor(tabId,resources=false,targetUrl=null,accountOnly=false){const results=await captureQueue(()=>chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:capturePage,args:[{resources,backgrounds:resources,targetUrl,accountOnly}]}));const page=results[0]?.result;if(!page)throw Error('当前页面无法读取。');return {...page,platform:page.platform||platformOf(page.url),site:siteOf(page.url),resources:resources?normalizeResources([...page.resources,...(network.get(tabId)||[])]):[]}}
async function resolvePage(tabId,targetUrl=null,preview=null){const tab=await chrome.tabs.get(tabId),key=cacheKey(tabId,tab.url,targetUrl);return retryStableParse(()=>parseCache.resolve(key,()=>parseQueue(async()=>{const result=await resolvePageWork(tabId,targetUrl,preview);if((await chrome.tabs.get(tabId)).url!==tab.url)throw Error('页面已切换，请重新读取。');return result})),async()=>(await chrome.tabs.get(tabId)).url===tab.url);}
async function resolvePageWork(tabId,targetUrl=null,preview=null){
 let page=await pageFor(tabId,false,targetUrl);if(preview){page.title=preview.title||page.title;page.cover=preview.cover||page.cover;}
 if(page.platform!=='web'&&page.platform){try{return {page,video:await parseVideo(page)}}catch(original){const state=await heartbeat();if(state.connected){const result=await call('inspect',{url:page.url},18000);if(result.data?.streams?.length)return {page,video:result.data};}throw original;}}
 const youtube=page.site?.id==='youtube',videoId=youtubeId(page.url);
 if(youtube){page.media=(page.media||[]).filter(item=>{try{return /(^|\.)googlevideo\.com$/.test(new URL(item.url).hostname)}catch{return false}});if(!page.media.length)page.media=observedYoutubeMedia(network.get(tabId)||[],videoId);}
 if(!youtube&&!preview&&page.singleVisibleVideo&&!page.media?.length){page.media=(network.get(tabId)||[]).filter(r=>['video','manifest'].includes(r.kind)&&!/[.](?:m4s|mpd)(?:$|\?)/.test(r.url)).slice(-8);}
 page.media=await expandManifests(page.media||[],(text,base)=>parseInWorker('hls',{text,base}));let video=await parseInWorker('generic',page);
 if(!video&&(targetUrl||youtube)){try{const result=(await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:linkedPageMedia,args:[targetUrl||page.url]}))[0]?.result;if(result){page={...page,...result};if(youtube)page.media=youtubeMedia(result.youtubeResponse,videoId);video=await parseInWorker('generic',page);}}catch{}}
 if(video){page.detail=video.detail;return {page,video};}
 const state=await heartbeat();if(state.connected&&page.platform==='web'){const result=await call('inspect',{url:page.url},youtube?22000:18000);if(result.error)throw Error(youtube?`YouTube：${result.error} 若提示登录或验证，请在浏览器打开并播放目标视频后重新读取。`:result.error);if(result.data?.streams?.length)return {page,video:result.data};}
 if(youtube)throw Error('此 YouTube 视频尚未提供可直接下载的资源。请打开并播放目标视频后重试；签名资源可启动 SnowDown 桌面解析，登录验证需在官方页面完成。');
 throw Error(page.platform==='web'?'此页面暂未提供直接视频资源。可连接 SnowDown 桌面端解析，或播放目标视频后重试。':'请打开一个视频页面，或切换到网页资源模式。');
}
async function syncSessions(){if(!connected||!(await prefs()).syncAccounts)return;const sessions=[];
 const tabs=await chrome.tabs.query({url:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*']});
 for(const platform of sessionPlatforms){
  const cookies=await chrome.cookies.getAll({url:`https://www.${platform}.com/`});
  const cookie=cookies.filter(c=>!c.partitionKey).map(c=>`${c.name}=${c.value}`).join('; ');
  const required=platform==='bilibili'?'SESSDATA':platform==='douyin'?'sessionid':'web_session';if(!cookies.some(c=>c.name===required&&c.value))continue;
  let account=null;const tab=tabs.find(t=>new URL(t.url).hostname.endsWith(`${platform}.com`));if(tab)try{account=(await pageFor(tab.id,false,null,true)).account}catch{}
  sessions.push({platform,cookie,account});
 }
 const signature=JSON.stringify(sessions);if(signature===lastSession)return;
 const result=await call('sessions',{sessions});if(result.connected&&!result.error){lastSession=signature;await chrome.storage.local.set({syncedAt:Date.now(),syncedPlatforms:result.data?.filter(x=>x.saved).map(x=>x.platform)||[]})}
}
chrome.cookies.onChanged.addListener(change=>{const authSite=authCookieSite(change.cookie);if(authSite)parseCache.invalidateWhere(key=>cacheSite(key)===authSite,'auth');if(!['bilibili.com','douyin.com','xiaohongshu.com'].some(domain=>change.cookie.domain.replace(/^\./,'').endsWith(domain)))return;clearTimeout(syncTimer);syncTimer=setTimeout(()=>void syncSessions(),1500)});
chrome.tabs.onRemoved.addListener(id=>{parseCache.invalidate(`${id}:`);network.delete(id);for(const [key,value]of floatTickets)if(value.tabId===id)floatTickets.delete(key)});
chrome.tabs.onUpdated.addListener((id,change)=>{if(change.status==='loading'||change.url){parseCache.invalidate(`${id}:`);network.delete(id);for(const [key,value]of floatTickets)if(value.tabId===id)floatTickets.delete(key)}});
const tabUrls=new Map(),tabLookups=new Map();
chrome.tabs.onUpdated.addListener((id,change)=>{if(change.url)tabUrls.set(id,change.url)});
chrome.tabs.onRemoved.addListener(id=>{tabUrls.delete(id);tabLookups.delete(id)});
chrome.webRequest.onHeadersReceived.addListener(details=>{
 if(details.tabId<0||details.url.length>4096)return;
 const mime=details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-type')?.value||'',kind=resourceKind(details.url,mime);if(!kind)return;
 const list=network.get(details.tabId)||[];if(list.slice(-16).some(item=>item.url===details.url))return;
 const item={url:details.url,mime,kind,size:Number(details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-length')?.value)||0};list.push(item);network.delete(details.tabId);network.set(details.tabId,list.slice(-240));while(network.size>16)network.delete(network.keys().next().value);
 if(/(^|\.)googlevideo\.com$/.test(new URL(details.url).hostname)){
  const known=tabUrls.get(details.tabId);if(known)item.youtubeId=youtubeId(known);
  else if(!tabLookups.has(details.tabId)){const task=chrome.tabs.get(details.tabId).then(tab=>{tabUrls.set(details.tabId,tab.url);for(const row of network.get(details.tabId)||[])if(/(^|\.)googlevideo\.com$/.test(new URL(row.url).hostname))row.youtubeId=youtubeId(tab.url)}).catch(()=>{}).finally(()=>tabLookups.delete(details.tabId));tabLookups.set(details.tabId,task)}
 }
},{urls:['http://*/*','https://*/*']},['responseHeaders']);
async function refererRules(){await chrome.declarativeNetRequest.updateDynamicRules({removeRuleIds:[1,2,3],addRules:[['bilibili.com',['bilivideo.com','bilivideo.cn']],['douyin.com',['douyinvod.com','douyin.com']],['xiaohongshu.com',['xhscdn.com','xiaohongshu.com']]].map(([host,domains],index)=>({id:index+1,priority:1,action:{type:'modifyHeaders',requestHeaders:[{header:'Referer',operation:'set',value:`https://www.${host}/`}]},condition:{initiatorDomains:[chrome.runtime.id],requestDomains:domains}}))})}
async function download(message){
 if(message.stream?.native&&message.browserOnly)throw Error('此画质需要桌面端解析，不能改用浏览器直接下载。');
 const settings=await prefs();if(message.browserOnly)settings.preferDesktop=false;await heartbeat();
 const nativeVersion=version.split('.').map(Number),modern=nativeVersion[0]>0||nativeVersion[1]>2||nativeVersion[1]===2&&nativeVersion[2]>=8;
 if(!modern&&(message.format==='audio'||message.page?.platform==='web'))settings.preferDesktop=false;
 if((settings.preferDesktop||message.stream?.native)&&connected&&message.page?.platform){if(settings.syncAccounts){await syncSessions();}const result=await call('queue',{url:message.page.url,detail:message.page.detail,quality:message.stream?.quality,format:message.format==='audio'?'audio':'video'});if(result.error)throw Error(result.error);if(result.connected)return {message:'已加入 SnowDown 下载队列。'};connected=false;}
 if(message.resource&&settings.preferDesktop&&connected){const r=await call('resource',{url:message.resource.url,name:message.resource.name||new URL(message.resource.url).pathname.split('/').pop(),kind:message.resource.kind});if(r.connected&&!r.error)return {message:'已发送到 SnowDown 下载队列。'};if(r.error)message.fallbackReason=r.error;}
 if(message.resource){const url=safeUrl(message.resource.url);if(!url)throw Error('此资源无法下载。');if(message.resource.kind==='manifest')throw Error('这是分段播放清单，请使用平台视频模式或选择直接视频文件。');await chrome.downloads.download({url,filename:`SnowDown/${safeName(message.resource.name||new URL(url).pathname.split('/').pop())}`,saveAs:false});return {message:message.fallbackReason?'此资源已回退到浏览器下载。':settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已开始浏览器下载。'};}
 const selected=message.stream;if(selected&&!selected.native&&parseCache.expiry({video:{streams:[selected]}})<=Date.now())throw Error('媒体链接已过期，请重新解析。');
 if(selected?.native)throw Error('此视频需要桌面解析，请先启动 SnowDown，再重新读取。');
 if(!safeUrl(selected?.url)||selected.audio&&!safeUrl(selected.audio))throw Error('媒体地址无效，请重新解析。');
 const job=crypto.randomUUID();await chrome.storage.session.set({[`job:${job}`]:{stream:selected,title:message.title,format:message.format||'video',createdAt:Date.now()}});
 await chrome.tabs.create({url:chrome.runtime.getURL(`job.html?id=${job}`),active:false});
 return {message:settings.preferDesktop&&!connected?'桌面端未连接，已使用浏览器下载。':'已在后台打开下载进度页，当前网页可继续浏览。'};
}
chrome.runtime.onMessage.addListener((message,sender,sendResponse)=>{
 if(message?.target==='snowdown-parser')return;
 const ownPage=sender.id===chrome.runtime.id&&sender.url?.startsWith(chrome.runtime.getURL(''));
 // Content scripts only operate on the sender's own top-level tab. They never
 // supply cookies, native methods, arbitrary media URLs or another tab's id.
 if(!ownPage){
  if(!contentSender(sender,chrome.runtime.id)||!['media-changed','float-state','float-parse','float-download','float-disable','preview-targets','preview-parse','preview-download'].includes(message.type))return;
  floatingRequest(message,sender).then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'无法读取视频'}));return true;
 }
 (async()=>{
  switch(message.type){
   case 'status':{void heartbeat().catch(()=>{});const settings=await prefs();return {connected,version,binding,effectiveDesktop:connected&&settings.preferDesktop,settings,...await chrome.storage.local.get(['syncedAt','syncedPlatforms'])};}
   case 'page':return pageFor(message.tabId,true);
   case 'parse':if(message.fresh)parseCache.invalidate(`${message.tabId}:`);return resolvePage(message.tabId);
   case 'settings':{const update={};for(const key of ['preferDesktop','syncAccounts','floatingEnabled'])if(typeof message[key]==='boolean')update[key]=message[key];await chrome.storage.local.set(update);lastSession='';void syncSessions();return heartbeat();}
   case 'sync':lastSession='';await syncSessions();return {ok:connected};
   case 'download':return download(message);
   case 'desktop-download':await chrome.tabs.create({url:repository});return {};
   default:throw Error('不支持的操作。');
  }
 })().then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'操作未完成，请重试。'}));return true;
});
function canonicalDouyin(input){const url=new URL(input);const id=url.pathname.match(/^\/video\/(\d{8,24})(?:\/|$)/)?.[1]||url.searchParams.get('modal_id')||url.searchParams.get('aweme_id')||url.searchParams.get('vid');return /^\d{8,24}$/.test(id||'')?`https://www.douyin.com/video/${id}`:null;}
async function douyinTargets(tabId){return (await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:douyinPreviewTargets}))[0]?.result||[];}
async function floatingRequest(message,sender){
 if(message.type==='media-changed'){parseCache.invalidate(`${sender.tab.id}:current:`);for(const [key,value]of floatTickets)if(value.tabId===sender.tab.id&&!value.preview)floatTickets.delete(key);return {ok:true}}
 if(message.type==='float-disable'){await chrome.storage.local.set({floatingEnabled:false});return {enabled:false}}
 const settings=await prefs();if(!settings.floatingEnabled)throw Error('悬浮球提醒已关闭，可在插件设置重新开启。');
 if(message.type==='float-state')return {enabled:true,platform:platformNames[platformOf(sender.url)]||null,supported:supportedVideoUrl(sender.url)};
 if(message.type==='preview-targets')return siteOf(sender.url)?.id==='douyin'?await douyinTargets(sender.tab.id):[];
 if(message.type==='preview-parse'){
  const target=previewUrl(message.url,sender.url);if(!target||siteOf(target)?.id!==siteOf(sender.url)?.id)throw Error('预览视频链接无效。');
  const check=(await chrome.scripting.executeScript({target:{tabId:sender.tab.id},world:'MAIN',func:url=>{const a=[...document.querySelectorAll('a[href]')].find(a=>a.href.split('#')[0]===url);if(!a)return null;const r=a.getBoundingClientRect();if(r.width<60||r.height<30||r.bottom<0||r.top>innerHeight)return null;const image=a.querySelector('img');return {title:a.title||image?.alt||a.textContent.trim().slice(0,200),cover:image?.currentSrc||image?.src||''}},args:[target]}))[0]?.result;
  const douyin=siteOf(target)?.id==='douyin'?(await douyinTargets(sender.tab.id)).find(item=>item.url===canonicalDouyin(target)):null;
  if(!check&&!douyin&&target!==sender.url)throw Error('预览卡片已变化，请重新读取。');
  const {page,video}=await resolvePage(sender.tab.id,target,check||douyin);const id=crypto.randomUUID();
  for(const [key,value]of floatTickets)if(Date.now()-value.createdAt>120000)floatTickets.delete(key);if(floatTickets.size>64)floatTickets.delete(floatTickets.keys().next().value);
  floatTickets.set(id,{tabId:sender.tab.id,url:sender.url,sourceUrl:target,createdAt:Date.now(),page,video,preview:true});
  return {id,title:video.title,rows:downloadRows(video),connected,desktop:(await prefs()).preferDesktop&&connected};
 }
 if(message.type==='float-parse'){
  const page=await pageFor(sender.tab.id,!platformOf(sender.url)||platformOf(sender.url)==='web');if(page.url!==sender.url)throw Error('页面已切换，请重新读取。');
  let video,resource;
  if(page.platform==='web'){const result=await resolvePage(sender.tab.id);video=result.video;Object.assign(page,result.page);}
  else if(page.platform){const result=await resolvePage(sender.tab.id);video=result.video;Object.assign(page,result.page);}
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
 if(ticket.preview&&ticket.sourceUrl!==sender.url){let present=(await chrome.scripting.executeScript({target:{tabId:sender.tab.id},world:'MAIN',func:url=>[...document.querySelectorAll('a[href]')].some(a=>a.href.split('#')[0]===url),args:[ticket.sourceUrl]}))[0]?.result;if(!present&&siteOf(ticket.sourceUrl)?.id==='douyin')present=(await douyinTargets(sender.tab.id)).some(item=>item.url===canonicalDouyin(ticket.sourceUrl));if(!present)throw Error('预览卡片已切换，请重新读取。');}
 if(message.type==='preview-download'&&message.kind==='image'){
  const cover=safeUrl(ticket.video.cover);if(!cover)throw Error('没有可下载封面。');const result=await download({resource:{url:cover,kind:'image',name:safeName(ticket.video.title)+'.'+(cover.split(/[?#]/)[0].match(/\.(jpg|jpeg|png|webp|avif)$/i)?.[1]||'jpg')},browserOnly:true});floatTickets.delete(message.id);return result;
 }
 const result=await download(ticket.resource?{resource:ticket.resource,browserOnly:message.route==='browser'}:{page:ticket.page,stream:ticket.video.streams[message.index],title:ticket.video.title,format:message.kind==='audio'?'audio':'video',browserOnly:message.route==='browser'});
 floatTickets.delete(message.id);return result;
}
async function registerResourceFloat(){
 const id='snowdown-floating-resources';const allowed=await chrome.permissions.contains({origins:['http://*/*','https://*/*']});
 const registered=(await chrome.scripting.getRegisteredContentScripts()).find(x=>x.id===id);
 if(allowed&&!registered)await chrome.scripting.registerContentScripts([{id,matches:['http://*/*','https://*/*'],excludeMatches:['https://*.bilibili.com/*','https://*.douyin.com/*','https://*.xiaohongshu.com/*'],js:['preview.js'],runAt:'document_idle',allFrames:false}]);
 if(allowed&&registered&&(registered.js?.length!==1||registered.js[0]!=='preview.js'))await chrome.scripting.updateContentScripts([{id,js:['preview.js']}]);
 if(!allowed&&registered)await chrome.scripting.unregisterContentScripts({ids:[id]});
}
chrome.permissions.onAdded.addListener(()=>void registerResourceFloat().catch(()=>{}));chrome.permissions.onRemoved.addListener(()=>{parseCache.clear('permission');network.clear();floatTickets.clear();void registerResourceFloat().catch(()=>{})});void registerResourceFloat().catch(()=>{});
chrome.alarms.create('connection',{periodInMinutes:.5});chrome.alarms.onAlarm.addListener(()=>void heartbeat());
setInterval(()=>{if(connected)void heartbeat().catch(()=>{})},8000);void refererRules();void heartbeat();
