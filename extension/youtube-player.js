// Serialized into the official site's MAIN world. Use its session and visitor
// context, not a remote parsing service or remotely executed player JavaScript.
export async function youtubePagePlayer(target){
 const source=new URL(target,location.href);
 if(!/(^|\.)youtube\.com$/.test(location.hostname)||source.origin!==location.origin)throw Error('请在 YouTube 官方页面读取视频。');
 const id=source.searchParams.get('v')||source.pathname.match(/^\/(?:shorts|live)\/([\w-]{11})(?:\/|$)/)?.[1];
 if(!/^[\w-]{11}$/.test(id||''))throw Error('请选择单个 YouTube 视频。');
 const config=window.ytcfg,visitor=config?.get?.('INNERTUBE_CONTEXT')?.client?.visitorData;
 const headers={'Content-Type':'application/json','X-Youtube-Client-Name':'101','X-Youtube-Client-Version':'1.02'};
 if(typeof visitor==='string'&&visitor.length<4096)headers['X-Goog-Visitor-Id']=visitor;
 const client={clientName:'VISIONOS',clientVersion:'1.02',deviceMake:'Apple',deviceModel:'RealityDevice17,1',userAgent:'Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15',osName:'visionOS',osVersion:'26.5.23O471',hl:'en',timeZone:'UTC',utcOffsetMinutes:0};
 const timestamp=config?.get?.('STS'),context={html5Preference:'HTML5_PREF_WANTS'};
 if(Number.isInteger(timestamp)&&timestamp>0)context.signatureTimestamp=timestamp;
 const response=await fetch('/youtubei/v1/player?prettyPrint=false',{method:'POST',credentials:'include',headers,body:JSON.stringify({context:{client},videoId:id,playbackContext:{contentPlaybackContext:context}}),signal:AbortSignal.timeout(12000)});
 if(!response.ok)throw Error('YouTube 播放器暂不可用，请重试。');
 const reader=response.body.getReader();let bytes=0,text='';const decoder=new TextDecoder();
 try{for(;;){const {done,value}=await reader.read();if(done)break;bytes+=value.length;if(bytes>4*1024*1024){await reader.cancel();throw Error('YouTube 视频数据过大。')}text+=decoder.decode(value,{stream:true})}text+=decoder.decode()}finally{reader.releaseLock()}
 const value=JSON.parse(text);
 if(value.playabilityStatus?.status!=='OK'||value.videoDetails?.videoId!==id||value.videoDetails?.isLiveContent)throw Error('YouTube 要求登录、验证，或此视频暂不可下载。请在官方页面完成验证后重试。');
 const info=value.videoDetails;
 return {videoDetails:{videoId:id,title:info.title,author:info.author,lengthSeconds:info.lengthSeconds,thumbnail:info.thumbnail},streamingData:value.streamingData};
}
