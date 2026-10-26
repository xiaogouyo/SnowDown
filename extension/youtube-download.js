// YouTube can stall an open-ended media response. Feed the muxer bounded,
// validated ranges instead of letting one indefinitely open request block it.
export async function youtubeRangeFetch(input,init={}){
 const url=new URL(input instanceof Request?input.url:input),headers=new Headers(init.headers),range=/^bytes=(\d+)-(\d*)$/.exec(headers.get('Range')||'');
 const total=Number(url.searchParams.get('clen'));
 if(!/(^|\.)googlevideo\.com$/.test(url.hostname)||url.pathname!=='/videoplayback'||!range||!Number.isSafeInteger(total)||total<=0||total>512*1024**3)return fetch(input,init);
 const first=Number(range[1]),last=Math.min(range[2]?Number(range[2]):total-1,total-1);
 if(!Number.isSafeInteger(first)||!Number.isSafeInteger(last)||first<0||first>last)throw Error('YouTube 下载范围无效，请重新解析。');
 const abort=new AbortController();let offset=first;
 const signal=init.signal?AbortSignal.any([init.signal,abort.signal]):abort.signal;
 const body=new ReadableStream({
  async pull(controller){
   const end=Math.min(offset+128*1024-1,last),requestHeaders=new Headers(headers);requestHeaders.set('Range',`bytes=${offset}-${end}`);
   try{
    const response=await fetch(url.href,{...init,headers:requestHeaders,credentials:'omit',signal:AbortSignal.any([signal,AbortSignal.timeout(20000)])});
    if(response.status!==206||response.headers.get('Content-Range')!==`bytes ${offset}-${end}/${total}`)throw Error('YouTube 未返回完整下载分块，请重新解析或改用桌面下载。');
    const reader=response.body.getReader(),chunks=[];let size=0;
    try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>end-offset+1){await reader.cancel();throw Error('YouTube 下载分块长度无效。')}chunks.push(value)}}finally{reader.releaseLock()}
    if(size!==end-offset+1)throw Error('YouTube 下载分块不完整，请重试。');
    const bytes=new Uint8Array(size);let copied=0;for(const chunk of chunks){bytes.set(chunk,copied);copied+=chunk.length}
    controller.enqueue(bytes);offset=end+1;if(offset>last)controller.close();
   }catch(error){abort.abort();controller.error(error)}
  },cancel(){abort.abort()}
 },{highWaterMark:0});
 return new Response(body,{status:206,headers:{'Content-Range':`bytes ${first}-${last}/${total}`,'Content-Length':String(last-first+1),'Content-Type':url.searchParams.get('mime')||'video/mp4'}});
}
export async function youtubeDownloadBuffer(raw){
 const url=new URL(raw),total=Number(url.searchParams.get('clen'));
 if(!/(^|\.)googlevideo\.com$/.test(url.hostname)||url.pathname!=='/videoplayback'||!Number.isSafeInteger(total)||total<=0||total>128*1024**2)return null;
 const response=await youtubeRangeFetch(raw,{headers:{Range:'bytes=0-'}});
 return new Uint8Array(await response.arrayBuffer());
}
