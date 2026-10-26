import {createLimiter} from './parse-cache.js';
import {safeUrl} from './core.js';
function attributes(line){const result={};for(const match of line.matchAll(/([A-Z0-9-]+)=("[^"]*"|[^,]*)/g))result[match[1]]=match[2].replace(/^"|"$/g,'');return result}
export function hlsVariants(text,base){
 if(!text.trimStart().startsWith('#EXTM3U'))return [];
 if(/#EXT-X-(?:SESSION-)?KEY:.*(?:KEYFORMAT="(?!identity)|METHOD=SAMPLE-AES)/.test(text))throw Error('此视频使用受保护的播放格式。');
 const lines=text.split(/\r?\n/),audio=new Map(),variants=[];
 for(const line of lines){if(line.startsWith('#EXT-X-MEDIA:')){const a=attributes(line);if(a.TYPE==='AUDIO'&&a.URI)try{audio.set(a['GROUP-ID'],safeUrl(new URL(a.URI,base).href))}catch{}}}
 for(let i=0;i<lines.length&&variants.length<32;i++){if(!lines[i].startsWith('#EXT-X-STREAM-INF:'))continue;const a=attributes(lines[i]);let j=i+1;while(j<lines.length&&(!lines[j].trim()||lines[j].startsWith('#')))j++;if(j>=lines.length)continue;try{const url=safeUrl(new URL(lines[j].trim(),base).href);if(!url)continue;const [width,height]=(a.RESOLUTION||'0x0').split('x').map(Number);variants.push({url,width:width||0,height:height||0,audio:audio.get(a.AUDIO)||null,kind:'manifest',masterUrl:base,label:height?`${height}P`:'原始画质',ext:'mp4'});}catch{}}
 return variants;
}
export async function expandManifests(media,parse=hlsVariants){
 const candidates=[...new Map(media.filter(s=>s.kind==='manifest'||/\.m3u8(?:$|\?)/i.test(s.url)).map(s=>[s.url,s])).values()].slice(0,8);
 if(!candidates.length)return media;
 const limited=createLimiter(2,8);const results=await Promise.allSettled(candidates.map(candidate=>limited(async()=>{const response=await fetch(candidate.url,{credentials:'include',signal:AbortSignal.timeout(4000)});if(!response.ok)throw Error('播放清单暂时无法读取。');const reader=response.body.getReader();let size=0,text='',decoder=new TextDecoder();try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>256*1024){await reader.cancel();throw Error('播放清单过大。');}text+=decoder.decode(value,{stream:true});}text+=decoder.decode();}finally{reader.releaseLock();}return parse(text,response.url||candidate.url)})));
 const masters=results.filter(r=>r.status==='fulfilled'&&r.value.length).map(r=>r.value);
 if(masters.length===1)return [...media.filter(s=>!candidates.some(c=>c.url===s.url)),...masters[0]];
 if(masters.length>1)throw Error('页面包含多个视频清单，请在目标预览卡片中下载，或使用桌面解析。');
 return media;
}
