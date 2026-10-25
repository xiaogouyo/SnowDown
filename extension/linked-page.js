// Read a preview target's public page without executing its scripts or navigating
// the user's tab. This runs in MAIN and uses that page's existing session.
export async function linkedPageMedia(target){
 const url=new URL(target,location.href);if(url.origin!==location.origin)throw Error('请打开目标视频后读取。');
 const response=await fetch(url.href,{credentials:'include',signal:AbortSignal.timeout(12000)});if(!response.ok||new URL(response.url).origin!==location.origin)throw Error('目标页面暂时无法读取。');
 const reader=response.body.getReader();let bytes=0,text='',decoder=new TextDecoder();try{for(;;){const {done,value}=await reader.read();if(done)break;bytes+=value.length;if(bytes>3*1024*1024){await reader.cancel();throw Error('目标页面数据过大。');}text+=decoder.decode(value,{stream:true});}text+=decoder.decode();}finally{reader.releaseLock();}
 const doc=new DOMParser().parseFromString(text,'text/html'),media=[];
 for(const node of doc.querySelectorAll('video[src],video source[src],meta[property="og:video"],meta[property="og:video:url"],meta[property="og:video:secure_url"],meta[name="twitter:player:stream"]')){const raw=node.getAttribute('src')||node.getAttribute('content');if(raw)try{const source=new URL(raw,url.href);if(/^https?:$/.test(source.protocol))media.push({url:source.href,label:'原始画质'});}catch{}}
 return {url:url.href,title:doc.querySelector('meta[property="og:title"]')?.content||doc.title,cover:doc.querySelector('meta[property="og:image"]')?.content||'',media};
}
