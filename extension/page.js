// Executes in the current tab's MAIN world. No form values or arbitrary cookies.
export async function capturePage(){
  const url=location.href,host=location.hostname;
  const platform=host.endsWith('bilibili.com')?'bilibili':host.endsWith('douyin.com')?'douyin':host.endsWith('xiaohongshu.com')?'xiaohongshu':null;
  const resources=[];
  for(const node of document.querySelectorAll('img,video,audio,source')){
    const src=node.currentSrc||node.src;if(!/^https?:/.test(src||''))continue;
    resources.push({url:src,kind:node.tagName==='IMG'?'image':node.tagName==='AUDIO'?'audio':'video',width:node.naturalWidth||node.videoWidth||0,height:node.naturalHeight||node.videoHeight||0,name:node.alt||'',mime:node.type||''});
    if(resources.length>=1000)break;
  }
  for(const entry of performance.getEntriesByType('resource').slice(-1000))resources.push({url:entry.name,...(entry.initiatorType==='img'?{kind:'image'}:{})});
  for(const node of [...document.querySelectorAll('*')].slice(0,5000)){
    const value=getComputedStyle(node).backgroundImage;
    for(const match of value.matchAll(/url\(["']?(https?:[^"')]+)["']?\)/g))resources.push({url:match[1],kind:'image'});
  }
  let id=platform==='douyin'?(new URL(url).searchParams.get('modal_id')||url.match(/\/video\/(\d+)/)?.[1]):platform==='xiaohongshu'?url.match(/\/(?:explore|item)\/([a-f\d]{24})/)?.[1]:url.match(/BV[\w]{10}/)?.[0];
  const roots=[window.__INITIAL_STATE__,window.__UNIVERSAL_DATA_FOR_REHYDRATION__,window.__snowdownMediaDetails];
  for(const script of document.querySelectorAll('script#RENDER_DATA,script#__NEXT_DATA__')){try{roots.push(JSON.parse(decodeURIComponent(script.textContent)))}catch{}}
  let detail=null,seen=new Set(),count=0;
  function find(value,depth=0){if(!value||typeof value!=='object'||seen.has(value)||depth>18||++count>18000)return;seen.add(value);
    if(platform==='douyin'&&String(value.aweme_id||value.awemeId)===id&&value.video)detail={aweme_detail:{...value,aweme_id:String(value.aweme_id||value.awemeId)}};
    if(platform==='xiaohongshu'&&(value.noteId===id||value.id===id)&&value.video)detail={note:{noteDetailMap:{[id]:{note:value}}}};
    if(!detail)for(const key of Object.keys(value).slice(0,300))find(value[key],depth+1);
  }
  if(platform==='douyin'||platform==='xiaohongshu')for(const root of roots){find(root);if(detail)break;}
  const root=platform==='douyin'?document.querySelector('[data-e2e="user-avatar"],[data-e2e="user-info"],header a[href*="/user/"]'):platform==='xiaohongshu'?[...document.querySelectorAll('a[href*="/user/profile/"]')].find(n=>/^(我|我的|个人主页)$/.test(n.textContent.trim())):null;
  const image=root?.querySelector('img')||(root?.tagName==='IMG'?root:null);
  const account=image?{name:(root?.title||image.alt||'').slice(0,80),avatar:image.currentSrc||image.src}:null;
  return {url,platform,id,title:document.title.slice(0,200),detail,account,resources};
}
