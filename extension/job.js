import {Input,ALL_FORMATS,UrlSource,Output,Mp4OutputFormat,BufferTarget,Conversion} from 'mediabunny';
import {safeName,safeUrl} from './core.js';
const text=document.querySelector('#status'),title=document.querySelector('h1');
const jobId=new URL(location.href).searchParams.get('id'),key=`job:${jobId}`;
let objectUrl;
try{
 const job=(await chrome.storage.session.get(key))[key];if(!job||Date.now()-job.createdAt>60*60*1000)throw Error('下载任务已失效，请重新解析。');
 title.textContent=job.title||'视频下载';
 if(!safeUrl(job.stream.url)||job.stream.audio&&!safeUrl(job.stream.audio))throw Error('媒体地址无效。');
 let download;
 if(!job.stream.audio){text.textContent='正在交给浏览器下载…';download=await chrome.downloads.download({url:job.stream.url,filename:`SnowDown/${safeName(job.title)}.mp4`,saveAs:false});}
 else{
  text.textContent='正在读取并合并音视频，请保留此页面…';
  const video=new Input({formats:ALL_FORMATS,source:new UrlSource(job.stream.url,{requestInit:{credentials:'include'}})});
  const audio=new Input({formats:ALL_FORMATS,source:new UrlSource(job.stream.audio,{requestInit:{credentials:'include'}})});
  const target=new BufferTarget(),output=new Output({format:new Mp4OutputFormat(),target});
  const v=await Conversion.init({input:video,output,audio:{discard:true},composable:true});
  const a=await Conversion.init({input:audio,output,video:{discard:true},composable:true});
  if(!v.isValid||!a.isValid)throw Error('浏览器不能合并此编码，请改用桌面下载。');
  v.onProgress=p=>{text.textContent=`正在合并音视频 ${Math.round(p*100)}% · 关闭此页会中断合并`;document.querySelector('progress').value=p*100};
  await output.start();await Promise.all([v.execute(),a.execute()]);await output.finalize();
  video.dispose();audio.dispose();objectUrl=URL.createObjectURL(new Blob([target.buffer],{type:'video/mp4'}));
  download=await chrome.downloads.download({url:objectUrl,filename:`SnowDown/${safeName(job.title)}.mp4`,saveAs:false});
 }
 await chrome.storage.session.remove(key);text.textContent='浏览器下载已开始，可在浏览器下载列表查看。';document.querySelector('progress').value=100;
 if(objectUrl)chrome.downloads.onChanged.addListener(delta=>{if(delta.id===download&&['complete','interrupted'].includes(delta.state?.current)){URL.revokeObjectURL(objectUrl);text.textContent=delta.state.current==='complete'?'下载完成。':'下载已中断，请在插件中重试。'}});
}catch(error){text.textContent=error.message;document.querySelector('progress').hidden=true;await chrome.storage.session.remove(key)}
