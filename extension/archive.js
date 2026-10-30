import {writeResourceZip} from './zip-store.js';import {safeName} from './core.js';
const status=document.querySelector('#status'),key='archive:'+new URL(location.href).searchParams.get('id'),abort=new AbortController();let directory,file,url,downloadId;
const cleanup=async()=>{if(url)URL.revokeObjectURL(url);if(directory&&file)await directory.removeEntry(file).catch(()=>{})};
document.querySelector('#cancel').onclick=()=>abort.abort();
try{
 const job=(await chrome.storage.session.get(key))[key];if(!job||Date.now()-job.createdAt>3600000)throw Error('打包任务已失效，请重新选择。');document.querySelector('h1').textContent=job.name;
 directory=await navigator.storage.getDirectory();
 // Only reap our own abandoned temporary archives, never browser/site data.
 let removed=0;for await(const [name,entry]of directory.entries()){if(removed>=20)break;if(entry.kind==='file'&&/^snowdown-archive-[a-f\d-]{36}\.zip$/.test(name)){const old=await entry.getFile();if(Date.now()-old.lastModified>86400000){await directory.removeEntry(name).catch(()=>{});removed++}}}
 file='snowdown-archive-'+crypto.randomUUID()+'.zip';const handle=await directory.getFileHandle(file,{create:true}),writer=await handle.createWritable();
 try{await writeResourceZip(job.items,writer,fetch,{signal:abort.signal,onProgress:p=>{status.textContent=`正在打包 ${p.index} / ${p.total} · ${(p.bytes/1048576).toFixed(1)} MB`;document.querySelector('progress').value=p.index/p.total*95}});await writer.close()}catch(error){await writer.abort().catch(()=>{});throw error}
 url=URL.createObjectURL(await handle.getFile());downloadId=await chrome.downloads.download({url,filename:'SnowDown/'+safeName(job.name)+'.zip',saveAs:false});status.textContent='ZIP 已提交浏览器保存，可在下载列表查看。';document.querySelector('progress').value=100;document.querySelector('#cancel').hidden=true;
 chrome.downloads.onChanged.addListener(change=>{if(change.id===downloadId&&['complete','interrupted'].includes(change.state?.current)){status.textContent=change.state.current==='complete'?'ZIP 下载完成。':'ZIP 保存中断，请重新打包。';void cleanup()}});
 const current=(await chrome.downloads.search({id:downloadId}))[0];if(current&&['complete','interrupted'].includes(current.state))await cleanup();
}catch(error){status.textContent=error.message||'打包失败，请重试。';document.querySelector('#cancel').hidden=true;await cleanup()}
finally{await chrome.storage.session.remove(key)}
