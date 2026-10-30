import {safeName,safeUrl,resourceKind} from './core.js';
export function resourceFormat(item){
 const types={'image/jpeg':'jpg','image/png':'png','image/webp':'webp','image/gif':'gif','image/svg+xml':'svg','video/mp4':'mp4','audio/mpeg':'mp3','application/pdf':'pdf','application/zip':'zip'};
 const mime=(item.mime||'').split(';')[0].toLowerCase();
 if(types[mime])return types[mime];
 try{const u=new URL(item.url),ext=u.pathname.match(/\.([a-z\d]{1,8})$/i)?.[1]||u.searchParams.get('format')||u.searchParams.get('fm');if(ext)return ext.toLowerCase()==='jpeg'?'jpg':ext.toLowerCase()}catch{}
 return mime.split('/')[1]?.replace(/^x-/,'')||'未知';
}
export function resourceFilename(item,index,template='{NAME}_{N}.{EXT}'){
 const format=resourceFormat(item),ext=/^[a-z\d]{1,8}$/.test(format)?format:'bin';
 let name=item.name;try{name=name||decodeURIComponent(new URL(item.url).pathname.split('/').pop()||'资源')}catch{}
 name=String(name||'资源').replace(/\.[a-z\d]{1,8}$/i,'');
 const date=new Date().toISOString().slice(0,10);
 let result=safeName(template.replaceAll('{NAME}',name).replaceAll('{DATE}',date).replaceAll('{N}',String(index+1).padStart(4,'0')).replaceAll('{EXT}',ext));
 while(new TextEncoder().encode(result).length>200)result=[...result].slice(0,-1).join('');
 if(/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(result))result='_'+result;
 return result.toLowerCase().endsWith('.'+ext)?result:result+'.'+ext;
}
export function resourceView(items,filter={}){
 const {kind='all',format='',minWidth=0,maxWidth=Infinity,minHeight=0,maxHeight=Infinity,query='',sort='auto'}=filter;
 const rows=items.filter(item=>(kind==='all'||item.kind===kind)&&(!format||resourceFormat(item)===format)&&(item.width||0)>=minWidth&&(item.width||0)<=maxWidth&&(item.height||0)>=minHeight&&(item.height||0)<=maxHeight&&(!query||`${item.url} ${item.name||''}`.toLowerCase().includes(query.toLowerCase())));
 if(sort!=='auto')rows.sort((a,b)=>((a.width||0)*(a.height||0)-(b.width||0)*(b.height||0))*(sort==='large'?-1:1));
 return rows;
}
export function archiveResources(items,template){
 if(!Array.isArray(items)||!items.length||items.length>200)throw Error('每个 ZIP 请选择 1 至 200 个资源。');
 const seen=new Set();return items.map((item,index)=>{
  const url=safeUrl(item.url),kind=resourceKind(url||'',item.mime)||item.kind;
  if(!url||kind==='manifest'||/\.(m3u8|mpd|m4s|ts)(?:$|[?#])/i.test(url))throw Error('播放清单和视频分片不能直接打包，请选择完整文件。');
  const base=resourceFilename(item,index,template);let name=base,n=1;while(seen.has(name.toLowerCase()))name=String(index+1).padStart(4,'0')+'_'+n+++'_'+base;seen.add(name.toLowerCase());
  return {url,name,kind,mime:item.mime||''};
 });
}
