import {Input,ALL_FORMATS,UrlSource,BufferSource,Output,Mp4OutputFormat,BufferTarget,Conversion} from 'mediabunny';
import {safeName,safeUrl} from './core.js';
import {youtubeRangeFetch,youtubeDownloadBuffer} from './youtube-download.js';
const text=document.querySelector('#status'),title=document.querySelector('h1');
const jobId=new URL(location.href).searchParams.get('id'),key=`job:${jobId}`;
let objectUrl;
try{
 const job=(await chrome.storage.session.get(key))[key];if(!job||Date.now()-job.createdAt>60*60*1000)throw Error('下载任务已失效，请重新解析。');
 title.textContent=job.title||'视频下载';
 if(!safeUrl(job.stream.url)||job.stream.audio&&!safeUrl(job.stream.audio))throw Error('媒体地址无效。');
 let download;
 const audioOnly=job.format==='audio',manifest=job.stream.kind==='manifest'||/\.m3u8(?:$|\?)/i.test(job.stream.url),extension=audioOnly?'m4a':job.stream.ext==='webm'&&!job.stream.audio&&!manifest?'webm':'mp4';
 if(!job.stream.audio&&!audioOnly&&!manifest){text.textContent='正在交给浏览器下载…';download=await chrome.downloads.download({url:job.stream.url,filename:`SnowDown/${safeName(job.title)}.${extension}`,saveAs:false});}
 else{
  text.textContent='正在读取并合并音视频，请保留此页面…';
  const options={requestInit:{credentials:'include'},fetchFn:youtubeRangeFetch,getRetryDelay:attempt=>attempt<=2?attempt:null};
  const videoUrl=audioOnly?(job.stream.audio||job.stream.url):job.stream.url;
  const audioUrl=!audioOnly?job.stream.audio:null;
  const [videoBytes,audioBytes]=await Promise.all([youtubeDownloadBuffer(videoUrl),audioUrl?youtubeDownloadBuffer(audioUrl):null]);
  const video=new Input({formats:ALL_FORMATS,source:videoBytes?new BufferSource(videoBytes):new UrlSource(videoUrl,options)});
  const audio=audioUrl?new Input({formats:ALL_FORMATS,source:audioBytes?new BufferSource(audioBytes):new UrlSource(audioUrl,options)}):null;
  const target=new BufferTarget(),output=new Output({format:new Mp4OutputFormat(),target});
  const v=await Conversion.init({input:video,output,...(audioOnly?{video:{discard:true}}:audio?{audio:{discard:true}}:{}),composable:true});
  const a=audio?await Conversion.init({input:audio,output,video:{discard:true},composable:true}):null;
  if(!v.isValid||a&&!a.isValid)throw Error('浏览器不能处理此编码，请改用桌面下载。');
  v.onProgress=p=>{text.textContent=`正在合并音视频 ${Math.round(p*100)}% · 关闭此页会中断合并`;document.querySelector('progress').value=p*100};
  await output.start();await Promise.all([v.execute(),...(a?[a.execute()]:[])]);await output.finalize();
  video.dispose();audio?.dispose();objectUrl=URL.createObjectURL(new Blob([target.buffer],{type:audioOnly?'audio/mp4':'video/mp4'}));
  download=await chrome.downloads.download({url:objectUrl,filename:`SnowDown/${safeName(job.title)}.${extension}`,saveAs:false});
 }
 await chrome.storage.session.remove(key);text.textContent='浏览器下载已开始，可在浏览器下载列表查看。';document.querySelector('progress').value=100;
 chrome.downloads.onChanged.addListener(delta=>{if(delta.id===download&&['complete','interrupted'].includes(delta.state?.current)){if(objectUrl)URL.revokeObjectURL(objectUrl);text.textContent=delta.state.current==='complete'?'下载完成。':'下载已中断，请重新解析后重试。'}});
}catch(error){text.textContent=error.message;document.querySelector('progress').hidden=true;await chrome.storage.session.remove(key)}
