import { safeUrl,platformOf } from './core.js';
const text=v=>typeof v==='string'?v:'';
const first=v=>Array.isArray(v)?v.find(x=>safeUrl(x)):null;
export function detailVideo(page){
 const source=page.platform==='douyin'?page.detail?.aweme_detail:page.detail?.note?.noteDetailMap?.[page.id]?.note;
 if(!source)return null;
 if(page.platform==='douyin'){
   const video=source.video;const variants=(video.bit_rate||[]).map(x=>({url:first(x.play_addr?.url_list),width:x.play_addr?.width||video.width,height:x.play_addr?.height||video.height,bitrate:x.bit_rate||0})).filter(x=>x.url).sort((a,b)=>b.bitrate-a.bitrate);
   const direct=variants[0]||{url:first(video.play_addr?.url_list),width:video.width,height:video.height};
   if(!direct.url)return null;const audio=(video.bit_rate_audio||[]).sort((a,b)=>(b.audio_meta?.bitrate||0)-(a.audio_meta?.bitrate||0))[0]?.audio_meta?.url_list;
   return {title:text(source.desc)||page.title,cover:first(video.cover?.url_list)||'',author:text(source.author?.nickname),duration:(video.duration||0)/1000,streams:[{...direct,quality:Math.min(direct.width||0,direct.height||0),label:'原始画质',audio:audio?.main_url||null}],detail:page.detail};
 }
 const video=source.video;const streams=[...(video.media?.stream?.h264||[]),...(video.media?.stream?.h265||[]),...(video.media?.stream?.av1||[])].map(s=>({url:s.masterUrl||first(s.backupUrls),width:s.width,height:s.height,bitrate:s.avgBitrate||0,quality:Math.min(s.width||0,s.height||0),label:`${Math.min(s.width||0,s.height||0)}P`})).filter(s=>safeUrl(s.url)).sort((a,b)=>b.bitrate-a.bitrate);
 if(!streams.length&&video.consumer?.originVideoKey)streams.push({url:`https://sns-video-bd.xhscdn.com/${video.consumer.originVideoKey}`,label:'原始画质'});
 return streams.length?{title:source.title||source.desc||page.title,cover:source.imageList?.[0]?.urlDefault||'',author:source.user?.nickname||'',streams,detail:page.detail}:null;
}
export async function biliVideo(page){
 const query=new URL(page.url);let id=page.id;
 if(!id)throw Error('请打开一个B站视频页面。');
 const get=async url=>{const response=await fetch(url,{credentials:'include'});if(!response.ok)throw Error(`B站返回 ${response.status}`);const data=await response.json();if(data.code!==0)throw Error(data.message||'B站解析失败');return data.data};
 const data=await get(`https://api.bilibili.com/x/web-interface/view?bvid=${id}`);
 const current=data.pages?.find(p=>p.page===Number(query.searchParams.get('p')||1))||data.pages?.[0];
 const play=await get(`https://api.bilibili.com/x/player/playurl?bvid=${id}&cid=${current?.cid||data.cid}&qn=64&fnval=4048&fourk=1`);
 const audio=play.dash?.audio?.sort((a,b)=>b.bandwidth-a.bandwidth)[0];
 const streams=play.dash?.video?.filter(s=>s.codecid===7).sort((a,b)=>b.id-a.id).map(s=>({url:s.baseUrl||s.base_url,audio:audio?.baseUrl||audio?.base_url,width:s.width,height:s.height,quality:s.id,label:play.accept_description?.[play.accept_quality?.indexOf(s.id)]||`${s.height}P`}))||play.durl?.map(s=>({url:s.url,label:'原始画质'}))||[];
 if(!streams.length)throw Error('当前账号没有可下载画质。');
 return {title:data.title,author:data.owner?.name,cover:data.pic,streams,duration:data.duration};
}
export async function parseVideo(page){if(!platformOf(page.url))throw Error('当前页不属于支持的平台。');if(page.platform==='bilibili')return biliVideo(page);const result=detailVideo(page);if(result)return result;throw Error('尚未取得当前视频详情，请播放视频后重试；首次安装可刷新页面。图集、直播或受限内容暂不支持。')}
