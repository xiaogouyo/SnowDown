// Executes in the current tab's MAIN world. No form values or arbitrary cookies.
export async function capturePage({resources:includeResources=false,backgrounds=true,targetUrl=null,accountOnly=false}={}){
  let sliceStart=performance.now();const yieldMain=async()=>{if(performance.now()-sliceStart>5){await new Promise(resolve=>setTimeout(resolve,0));sliceStart=performance.now();}};
  const url=targetUrl||location.href,host=new URL(url).hostname;
  const matches=base=>host===base||host.endsWith(`.${base}`);
  const platform=matches('bilibili.com')?'bilibili':matches('douyin.com')?'douyin':matches('xiaohongshu.com')?'xiaohongshu':null;
  if(accountOnly){const root=platform==='douyin'?document.querySelector('[data-e2e="user-avatar"],[data-e2e="user-info"],header a[href*="/user/"]'):platform==='xiaohongshu'?[...document.querySelectorAll('a[href*="/user/profile/"]')].find(n=>/^(我|我的|个人主页)$/.test(n.textContent.trim())):null;const image=root?.querySelector('img')||(root?.tagName==='IMG'?root:null);return {url,platform,resources:[],account:image?{name:(root?.title||image.alt||'').slice(0,80),avatar:image.currentSrc||image.src}:null};}
  const resources=[];
  if(includeResources)for(const node of document.querySelectorAll('img,video,audio,source')){
    if(resources.length%64===0)await yieldMain();const src=node.currentSrc||node.src;if(!/^https?:/.test(src||''))continue;
    resources.push({url:src,kind:node.tagName==='IMG'?'image':node.tagName==='AUDIO'||node.tagName==='SOURCE'&&node.parentElement?.tagName==='AUDIO'?'audio':'video',width:node.naturalWidth||node.videoWidth||0,height:node.naturalHeight||node.videoHeight||0,name:node.alt||'',mime:node.type||''});
    if(resources.length>=1000)break;
  }
  if(includeResources)for(const entry of performance.getEntriesByType('resource').slice(-1000))resources.push({url:entry.name,...(entry.initiatorType==='img'?{kind:'image'}:{})});
  if(includeResources&&backgrounds)for(const node of [...document.querySelectorAll('*')].slice(0,5000)){
    await yieldMain();const value=getComputedStyle(node).backgroundImage;
    for(const match of value.matchAll(/url\(["']?(https?:[^"')]+)["']?\)/g))resources.push({url:match[1],kind:'image'});
  }
  if(includeResources)for(const node of [...document.querySelectorAll('img,source,a[href],object[data],embed[src]')].slice(0,3000)){
    await yieldMain();const image=['IMG','SOURCE'].includes(node.tagName);
    const urls=image?[node.getAttribute('data-src'),node.getAttribute('data-original'),node.getAttribute('data-lazy-src'),...(node.getAttribute('srcset')||node.getAttribute('data-srcset')||'').split(',').map(x=>x.trim().split(/\s+/)[0])]:[node.href||node.data||node.src];
    for(const raw of urls){if(!raw)continue;try{const source=new URL(raw,location.href);if(!/^https?:$/.test(source.protocol))continue;const kind=image?'image':/\.(pdf|txt|csv|docx?|xlsx?|pptx?|zip|rar|7z)(?:$|[?#])/i.test(source.href)?'document':null;if(kind)resources.push({url:source.href,kind,name:node.alt||node.download||'',mime:node.type||''})}catch{}}
    if(resources.length>=3000)break;
  }
  let id=platform==='douyin'?(new URL(url).searchParams.get('modal_id')||url.match(/\/video\/(\d+)/)?.[1]):platform==='xiaohongshu'?url.match(/\/(?:explore|item)\/([a-f\d]{24})/)?.[1]:url.match(/BV[\w]{10}/)?.[0];
  const roots=[...(Array.isArray(window.__snowdownMediaDetails)?window.__snowdownMediaDetails.slice(-4).reverse():[]),window.__INITIAL_STATE__,window._ROUTER_DATA,window.__UNIVERSAL_DATA_FOR_REHYDRATION__];
  for(const script of document.querySelectorAll('script#RENDER_DATA,script#__NEXT_DATA__')){const raw=script.textContent;if(!raw||raw.length>1024*1024)continue;try{roots.push(JSON.parse(raw))}catch{try{roots.push(JSON.parse(decodeURIComponent(raw)))}catch{}}}
  let detail=null,seen=new Set(),count=0;
  function address(value){
    if(!value)return null;
    const list=typeof value==='string'?[value]:Array.isArray(value)?value.flatMap(item=>address(item)?.url_list||[]):value.url_list||value.urlList||[value.src||value.url];
    const urls=(Array.isArray(list)?list:[]).filter(url=>typeof url==='string'&&/^https?:\/\//.test(url)).slice(0,6);
    return urls.length?{url_list:urls,width:Number(value.width)||0,height:Number(value.height)||0}:null;
  }
  function project(value){
    const video=value.video;
    const variants=video.bit_rate||video.bitRateList;
    const rates=(Array.isArray(variants)?variants:[]).slice(0,32).map(rate=>({play_addr:address(rate.play_addr||rate.playAddr),bit_rate:Number(rate.bit_rate||rate.bitRate)||0})).filter(rate=>rate.play_addr);
    const direct=address(video.play_addr||video.playAddr),h264=address(video.play_addr_h264||video.playAddrH264),hevc=address(video.play_addr_bytevc1||video.playAddrBytevc1);
    if(!direct&&!h264&&!hevc&&!rates.length)return null;
    const audioRates=video.bit_rate_audio||video.bitRateAudio;
    const audio=(Array.isArray(audioRates)?audioRates:[]).slice(0,16).map(rate=>{const meta=rate.audio_meta||rate.audioMeta||{},urls=meta.url_list||meta.urlList||{};return {audio_meta:{bitrate:Number(meta.bitrate)||0,url_list:{main_url:urls.main_url||urls.mainUrl,backup_url:urls.backup_url||urls.backupUrl}}}});
    return {aweme_detail:{aweme_id:id,desc:String(value.desc||'').slice(0,1000),author:{nickname:String(value.author?.nickname||'').slice(0,100)},video:{width:Number(video.width)||0,height:Number(video.height)||0,duration:Number(video.duration)||0,cover:address(video.cover),play_addr:direct,play_addr_h264:h264,play_addr_bytevc1:hevc,bit_rate:rates,bit_rate_audio:audio}}};
  }
  async function find(value,depth=0){if(!value||typeof value!=='object'||seen.has(value)||depth>18||++count>18000)return;seen.add(value);if(count%128===0)await yieldMain();
    if(value===window||value instanceof Node)return;
    if(platform==='douyin'&&String(value.aweme_id||value.awemeId)===id&&value.video){try{detail=project(value)}catch{}}
    if(platform==='xiaohongshu'&&(value.noteId===id||value.id===id)&&value.video)detail={note:{noteDetailMap:{[id]:{note:value}}}};
    if(!detail)for(const key of Object.keys(value).slice(0,300)){try{await find(value[key],depth+1)}catch{}if(detail)break;}
  }
  if(platform==='douyin'||platform==='xiaohongshu')for(const root of roots){await find(root);if(detail)break;}
  // A tab may already be playing when the extension is installed/reloaded. Inspect
  // bounded React props adjacent to the player; return only the exact video's fields.
  if(platform==='douyin'&&!detail&&id){
    seen=new Set();count=0;
    const candidates=[...[...document.querySelectorAll('video')].slice(0,12),...[...document.querySelectorAll('img,[data-e2e="video-card"],[data-aweme-id],[data-video-id],[style*="background-image"]')].slice(0,1200).filter(node=>{const r=node.getBoundingClientRect();return r.width>=100&&r.height>=70&&r.bottom>0&&r.top<innerHeight&&r.right>0&&r.left<innerWidth}).slice(0,64)];
    for(const video of candidates){
      for(let node=video,level=0;node&&level<12&&!detail;node=node.parentElement,level++){
        for(const key of Object.keys(node).filter(key=>/^__react(?:Props|Fiber)\$/.test(key))){
          let value;try{value=node[key]}catch{continue}
          if(key.startsWith('__reactProps'))await find(value);
          else for(let fiber=value,n=0;fiber&&n<12&&!detail;fiber=fiber.return,n++){await find(fiber.memoizedProps);if(!detail)await find(fiber.pendingProps)}
          if(detail)break;
        }
      }
      if(detail)break;
    }
  }
  const root=platform==='douyin'?document.querySelector('[data-e2e="user-avatar"],[data-e2e="user-info"],header a[href*="/user/"]'):platform==='xiaohongshu'?[...document.querySelectorAll('a[href*="/user/profile/"]')].find(n=>/^(我|我的|个人主页)$/.test(n.textContent.trim())):null;
  const image=root?.querySelector('img')||(root?.tagName==='IMG'?root:null);
  const account=image?{name:(root?.title||image.alt||'').slice(0,80),avatar:image.currentSrc||image.src}:null;
  const verificationRequired=platform==='douyin'&&/验证码中间页|安全验证|security verification/i.test(document.title);
  const preview=url!==location.href,media=[];
  const videos=[...document.querySelectorAll('video')].filter(video=>{const rect=video.getBoundingClientRect();return rect.width>100&&rect.height>70&&rect.bottom>0&&rect.top<innerHeight&&(!preview||video.closest('a[href]')?.href===url)});
  for(const video of videos.slice(0,16)){const src=video.currentSrc||video.src;if(/^https?:/.test(src))media.push({url:src,width:video.videoWidth,height:video.videoHeight,cover:video.poster,label:video.videoHeight?`${video.videoHeight}P`:'原始画质'});}
  if(!preview){for(const meta of document.querySelectorAll('meta[property="og:video"],meta[property="og:video:url"],meta[property="og:video:secure_url"],meta[name="twitter:player:stream"]'))if(/^https?:/.test(meta.content))media.push({url:meta.content,label:'原始画质'});
   for(const script of [...document.querySelectorAll('script[type="application/ld+json"]')].slice(0,15)){if(script.textContent.length>512*1024)continue;try{const root=JSON.parse(script.textContent),queue=Array.isArray(root)?root:[root];let n=0;while(queue.length&&n++<200){const item=queue.shift();if(!item||typeof item!=='object')continue;if(item['@type']==='VideoObject'&&typeof item.contentUrl==='string')media.push({url:item.contentUrl,label:'原始画质',cover:Array.isArray(item.thumbnailUrl)?item.thumbnailUrl[0]:item.thumbnailUrl});for(const value of Object.values(item))if(value&&typeof value==='object')queue.push(...(Array.isArray(value)?value:[value]));}}catch{}}
  }
  let youtube=window.ytInitialPlayerResponse;const targetId=new URL(url).searchParams.get('v')||new URL(url).pathname.match(/\/(?:shorts|live)\/([\w-]+)/)?.[1];
  if((host==='youtube.com'||host.endsWith('.youtube.com'))&&targetId&&youtube?.videoDetails?.videoId!==targetId)try{const current=document.querySelector('#movie_player')?.getPlayerResponse?.();if(current?.videoDetails?.videoId===targetId)youtube=current;}catch{}
  if((host==='youtube.com'||host.endsWith('.youtube.com'))&&targetId&&youtube?.videoDetails?.videoId===targetId){const data=youtube.streamingData||{},audio=(data.adaptiveFormats||[]).filter(f=>/^audio\//.test(f.mimeType||'')&&/^https?:/.test(f.url||'')).sort((a,b)=>b.bitrate-a.bitrate)[0]?.url;for(const f of [...(data.formats||[]),...(data.adaptiveFormats||[])].slice(0,128))if(/^video\//.test(f.mimeType||'')&&/^https?:/.test(f.url||''))media.push({url:f.url,width:f.width,height:f.height,label:f.qualityLabel||'原始画质',audio:(data.adaptiveFormats||[]).includes(f)?audio:null,ext:/webm/.test(f.mimeType)?'webm':'mp4'});}
  const cover=document.querySelector('meta[property="og:image"]')?.content;
  return {url,platform,id,title:document.title.slice(0,200),detail,account,resources,verificationRequired,media,singleVisibleVideo:videos.length===1,cover,youtubeResponse:(host==='youtube.com'||host.endsWith('.youtube.com'))&&targetId&&youtube?.videoDetails?.videoId===targetId?{videoDetails:youtube.videoDetails,streamingData:youtube.streamingData}:undefined};
}
