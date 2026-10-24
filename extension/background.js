import {capturePage} from './page.js';
import {parseVideo} from './sites.js';
import {platformNames,normalizeResources,safeUrl,safeName,repository,resourceKind} from './core.js';
const browser=/Edg\//.test(navigator.userAgent)?'edge':'chrome';
let port,sequence=0,connected=false,version='',binding='',lastSession='',syncTimer;
const pending=new Map(),network=new Map();
const prefs=async()=>({preferDesktop:false,syncAccounts:true,...await chrome.storage.local.get(['preferDesktop','syncAccounts','lastBinding'])});
async function call(method,data={}){
 if(!port){try{port=chrome.runtime.connectNative('app.snowdown.bridge');port.onMessage.addListener(value=>{const waiter=pending.get(value.id);if(waiter){pending.delete(value.id);clearTimeout(waiter.timer);waiter.resolve(value)}});port.onDisconnect.addListener(()=>{void chrome.runtime.lastError;port=null;connected=false;for(const waiter of pending.values()){clearTimeout(waiter.timer);waiter.resolve({connected:false})}pending.clear()})}catch{return {connected:false}}}
 return new Promise(resolve=>{const id=++sequence;pending.set(id,{resolve,timer:setTimeout(()=>{pending.delete(id);resolve({connected:false,error:'桌面连接超时，请稍后重试。'})},45000)});try{port.postMessage({id,browser,method,...data})}catch{pending.delete(id);resolve({connected:false})}});
}
async function heartbeat(){const result=await call('hello');const before=connected;connected=!!result.connected;version=result.data?.version||'';binding=result.data?.binding||'';
 const settings=await prefs();if(connected&&binding&&settings.lastBinding!==binding){await chrome.storage.local.set({lastBinding:binding,preferDesktop:true,syncAccounts:true});}
 if(connected&&!before){lastSession='';void syncSessions();}
 await chrome.action.setBadgeText({text:connected?'✓':''});await chrome.action.setBadgeBackgroundColor({color:'#3478ee'});
 return {connected,version,binding,effectiveDesktop:connected&&(await prefs()).preferDesktop};}
async function pageFor(tabId){const results=await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:capturePage});const page=results[0]?.result;if(!page)throw Error('当前页面无法读取。');return {...page,resources:normalizeResources([...page.resources,...(network.get(tabId)||[])])}}
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
chrome.tabs.onRemoved.addListener(id=>network.delete(id));
chrome.tabs.onUpdated.addListener((id,change)=>{if(change.status==='loading')network.delete(id)});
chrome.webRequest.onHeadersReceived.addListener(details=>{if(details.tabId<0)return;const mime=details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-type')?.value||'';const kind=resourceKind(details.url,mime);if(!kind)return;const list=network.get(details.tabId)||[];list.push({url:details.url,mime,kind,size:Number(details.responseHeaders?.find(h=>h.name.toLowerCase()==='content-length')?.value)||0});network.set(details.tabId,list.slice(-1000))},{urls:['http://*/*','https://*/*']},['responseHeaders']);
async function refererRules(){await chrome.declarativeNetRequest.updateDynamicRules({removeRuleIds:[1,2,3],addRules:[['bilibili.com',['bilivideo.com','bilivideo.cn']],['douyin.com',['douyinvod.com','douyin.com']],['xiaohongshu.com',['xhscdn.com','xiaohongshu.com']]].map(([host,domains],index)=>({id:index+1,priority:1,action:{type:'modifyHeaders',requestHeaders:[{header:'Referer',operation:'set',value:`https://www.${host}/`}]},condition:{initiatorDomains:[chrome.runtime.id],requestDomains:domains}}))})}
async function download(message){
 const settings=await prefs();await heartbeat();
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
 // Only our extension pages can issue privileged requests; no content script bridge.
 if(sender.id!==chrome.runtime.id||!sender.url?.startsWith(chrome.runtime.getURL('')))return;
 (async()=>{
  switch(message.type){
   case 'status':return {...await heartbeat(),settings:await prefs(),...await chrome.storage.local.get(['syncedAt','syncedPlatforms'])};
   case 'page':return pageFor(message.tabId);
   case 'parse':{const page=await pageFor(message.tabId);return {page,video:await parseVideo(page)}};
   case 'settings':await chrome.storage.local.set({preferDesktop:!!message.preferDesktop,syncAccounts:!!message.syncAccounts});lastSession='';await syncSessions();return heartbeat();
   case 'sync':lastSession='';await syncSessions();return {ok:connected};
   case 'download':return download(message);
   case 'desktop-download':await chrome.tabs.create({url:repository});return {};
   default:throw Error('不支持的操作。');
  }
 })().then(data=>sendResponse({ok:true,data}),error=>sendResponse({ok:false,error:error.message||'操作未完成，请重试。'}));return true;
});
chrome.alarms.create('connection',{periodInMinutes:.5});chrome.alarms.onAlarm.addListener(()=>void heartbeat());
setInterval(()=>void heartbeat(),5000);void refererRules();void heartbeat();
