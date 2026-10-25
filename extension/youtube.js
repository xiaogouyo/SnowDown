import {safeUrl} from './core.js';
export function youtubeId(input){try{const url=new URL(input);if(url.hostname==='youtu.be')return url.pathname.slice(1).match(/^[\w-]{11}$/)?.[0]||null;if(!/(^|\.)youtube\.com$/.test(url.hostname))return null;return (url.searchParams.get('v')||url.pathname.match(/^\/(?:shorts|live)\/([\w-]{11})(?:\/|$)/)?.[1]||'').match(/^[\w-]{11}$/)?.[0]||null}catch{return null}}
export function youtubeMedia(value,id){
 if(!id||value?.videoDetails?.videoId!==id||value.videoDetails.isLiveContent)return [];
 const data=value.streamingData||{},audio=(data.adaptiveFormats||[]).filter(f=>/^audio\/mp4/.test(f.mimeType||'')&&safeUrl(f.url)).sort((a,b)=>b.bitrate-a.bitrate)[0]?.url;
 return [...(data.formats||[]),...(data.adaptiveFormats||[])].slice(0,128).filter(f=>/^video\/mp4/.test(f.mimeType||'')&&safeUrl(f.url)&&(!(data.adaptiveFormats||[]).includes(f)||audio)).map(f=>({url:f.url,audio:(data.adaptiveFormats||[]).includes(f)?audio:null,width:f.width,height:f.height,label:f.qualityLabel||'原始画质',ext:'mp4'}));
}
export function observedYoutubeMedia(rows,id){
 if(!id)return [];
 const groups=new Map();for(const row of rows){if(row.youtubeId!==id)continue;try{const url=new URL(row.url);if(!/(^|\.)googlevideo\.com$/.test(url.hostname)||url.pathname!=='/videoplayback')continue;const type=row.mime||url.searchParams.get('mime')||'';if(!/^(video|audio)\/mp4/.test(type))continue;const signed=(url.searchParams.get('sparams')||'').split(',');if(url.searchParams.has('range')&&signed.includes('range'))continue;url.searchParams.delete('range');const key=url.searchParams.get('id');if(!key)continue;const group=groups.get(key)||{video:[],audio:[]};group[/^audio/.test(type)?'audio':'video'].push({...row,url:url.href});groups.set(key,group);}catch{}}
 // Multiple media IDs may be an advertisement or prefetched next video.
 if(groups.size!==1)return [];
 const group=[...groups.values()][0],audio=group.audio.at(-1)?.url;
 return audio?group.video.map(row=>({url:row.url,audio,width:0,height:0,label:'已播放画质',ext:'mp4'})):[];
}
