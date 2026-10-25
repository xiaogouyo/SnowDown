import {safeUrl,resourceKind} from './core.js';
export function genericVideo(page){
 const media=page.media||[];const seen=new Set();const streams=[];
 for(const item of media){const url=safeUrl(item.url);if(!url||seen.has(url)||resourceKind(url,item.mime)==='image')continue;seen.add(url);if(/\.mpd(?:$|\?)/i.test(url))continue;
  const height=Number(item.height)||0,width=Number(item.width)||0;streams.push({...item,url,audio:safeUrl(item.audio),quality:height,kind:resourceKind(url,item.mime)==='manifest'?'manifest':'video',label:item.label|| (height?`${Math.min(width||height,height)}P`:'原始画质'),ext:item.ext||(/\.webm(?:$|\?)/i.test(url)?'webm':'mp4')});
 }
 if(!streams.length)return null;streams.sort((a,b)=>(b.quality||0)-(a.quality||0));
 const result={title:page.title||'网页视频',author:page.author||'',cover:safeUrl(page.cover)||'',streams};
 result.detail={webMedia:{sourceUrl:page.url,title:result.title,author:result.author,cover:result.cover,streams}};
 return result;
}
export function downloadRows(video){
 const rows=[];for(let index=0;index<(video.streams||[]).length;index++){const stream=video.streams[index],resolution=Math.min(stream.width||stream.height,stream.height);rows.push({kind:'video',index,label:`${stream.ext||'mp4'} ${resolution?`(${resolution}p)`:`(${stream.label||'原始画质'})`}`,native:!!stream.native});}
 if(video.streams?.length)rows.push({kind:'audio',index:0,label:'m4a',native:!!video.streams[0].native});
 if(safeUrl(video.cover))rows.push({kind:'image',index:0,label:video.cover.split(/[?#]/)[0].match(/\.([a-z\d]+)$/i)?.[1]?.toLowerCase()||'封面'});
 return rows;
}
