// Isolated-world reminder: no page message bridge or page-supplied media URLs.
import {fitFloatingRect,floatingOrigin,rememberFloatingRect} from './floating-core.js';
(()=>{
 if(window.__snowdownFloatingInstalled)return;window.__snowdownFloatingInstalled=true;
 let enabled=true,host,surface,expanded=false,busy=false,ticket=null,lastUrl=location.href,scanTimer,position=null,drag=null,suppressClickUntil=0,exitMotion=null;
 const icon='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12m-5-5 5 5 5-5M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"/></svg>';
 const css=`:host{all:initial;position:fixed;right:24px;bottom:112px;z-index:2147483646;display:block;font-family:'Microsoft YaHei UI',system-ui,sans-serif;color:#30435f;font-size:12px;color-scheme:light;transition:left .48s cubic-bezier(.2,.8,.2,1),top .48s cubic-bezier(.2,.8,.2,1)}*{box-sizing:border-box}button,select{font:inherit}button{cursor:pointer;border:0}button:disabled{opacity:.55;cursor:default}button:focus-visible,select:focus-visible{outline:2px solid #8ab7ff;outline-offset:3px}.surface{width:54px;height:54px;background:#fff;border:1px solid #dce8fa;border-radius:27px;box-shadow:0 7px 26px #335c9229;overflow:hidden;position:relative;transform-origin:right center;transition:width .48s cubic-bezier(.2,.8,.2,1),height .48s cubic-bezier(.2,.8,.2,1),border-radius .48s,box-shadow .25s}.surface.arriving{animation:arrive .62s cubic-bezier(.2,.8,.2,1)}.surface:hover{box-shadow:0 9px 30px #335c9238}.orb{width:54px;height:54px;display:flex;align-items:center;justify-content:center;background:linear-gradient(140deg,#f6faff,#eaf2ff);color:#4c8ce8;transition:opacity .18s,transform .35s;position:absolute;right:0;bottom:0}.orb svg{width:24px;height:24px}.dot{position:absolute;right:11px;top:10px;width:6px;height:6px;border:1.5px solid #fff;border-radius:50%;background:#70b89c}.panel{width:338px;height:478px;max-height:calc(100vh - 140px);overflow:auto;padding:20px;opacity:0;visibility:hidden;transform:translateX(15px);transition:opacity .2s,transform .35s,visibility .2s;color:#30435f}.expanded{width:340px;height:480px;max-height:calc(100vh - 138px);border-radius:20px;box-shadow:0 12px 45px #335c922b}.expanded .orb{opacity:0;pointer-events:none;transform:scale(.7)}.expanded .panel{opacity:1;visibility:visible;transform:none;transition-delay:.13s}.heading{display:flex;align-items:center;gap:9px;padding-bottom:17px;border-bottom:1px solid #edf2fa}.heading img{width:27px;height:27px}.heading strong{font-size:17px;color:#28374e}.heading strong span{color:#3478ee}.heading button{margin-left:auto;color:#91a5c1;background:#f5f8fc;width:26px;height:26px;border-radius:8px;font-size:18px}.route{font-size:10px;color:#83a59c;margin:16px 0 8px}h2{font-size:15px;line-height:1.7;max-height:77px;overflow:hidden;margin:8px 0;color:#405673;overflow-wrap:anywhere}.author{font-size:11px;color:#9bacc3;margin:0 0 15px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}label{display:block;color:#91a4bf;font-size:10px;margin:14px 0 7px}select{width:100%;height:37px;border-radius:9px;border:1px solid #dfe9f7;background:#f9fbff;color:#6f89ac;padding:0 9px}.download{margin-top:18px;width:100%;padding:11px;border-radius:10px;background:#3478ee;color:white;box-shadow:0 4px 13px #3478ee1c}.message{height:33px;margin:10px 0 0;font-size:10px;line-height:1.6;color:#8fa5c3;overflow:auto;overflow-wrap:anywhere}.foot{display:flex;justify-content:space-between;margin-top:12px}.foot button{padding:3px 0;background:none;color:#91a5c1;font-size:10px}.foot button:hover{color:#548de0}:host(.dragging){transition:none;user-select:none}.orb,.heading{touch-action:none;user-select:none;cursor:grab}.orb:active,.heading:active{cursor:grabbing}:host(.dragging) .surface{animation:none;transition:none;box-shadow:0 12px 32px #335c9244}:host(.exiting){transition:none}:host(.exiting) .panel{opacity:0;visibility:hidden;transition:opacity .12s}:host(.exiting) .orb{opacity:1;transform:none;pointer-events:none}*,.panel,.message{scrollbar-width:thin;scrollbar-color:#c8d8ed transparent}::-webkit-scrollbar{width:6px;height:6px}::-webkit-scrollbar-track{background:transparent}::-webkit-scrollbar-thumb{background:#c8d8ed;border-radius:10px;border:1px solid #fff}::-webkit-scrollbar-thumb:hover{background:#a5bfdc}@keyframes arrive{0%{transform:translateX(90px) scaleX(.4) scaleY(1.1);opacity:0}48%{transform:translateX(-5px) scaleX(1.14) scaleY(.88);opacity:1}76%{transform:translateX(2px) scaleX(.96) scaleY(1.03)}100%{transform:none;opacity:1}}@media(max-width:600px){:host{right:12px;bottom:80px}.expanded{width:min(340px,calc(100vw - 24px))}.panel{width:min(338px,calc(100vw - 26px))}}@media(prefers-reduced-motion:reduce){:host,.surface,.orb,.panel{transition:none!important;animation:none!important;transform:none!important}}`;
 async function send(type,data={}){const result=await chrome.runtime.sendMessage({type,...data});if(!result?.ok)throw Error(result?.error||'读取失败，请重试');return result.data}
 function node(tag,className,text,parent){const n=document.createElement(tag);n.className=className;if(text)n.textContent=text;parent?.append(n);return n}
 const viewport=()=>({width:innerWidth,height:innerHeight});
 function place(){if(!host||drag||exitMotion)return;const point=floatingOrigin(position,viewport(),expanded);Object.assign(host.style,{left:`${point.left}px`,top:`${point.top}px`,right:'auto',bottom:'auto'})}
 function remove(){exitMotion?.animations.forEach(a=>a.cancel());exitMotion=null;host?.remove();host=surface=null;expanded=false;ticket=null;busy=false;drag=null}
 function leave(){
  if(!host||exitMotion)return;
  if(matchMedia('(prefers-reduced-motion: reduce)').matches){remove();return}
  const node=host,card=surface,rect=node.getBoundingClientRect(),point=floatingOrigin(position,viewport()),direction=point.left<innerWidth/2?-1:1;
  node.style.pointerEvents='none';node.classList.add('exiting');drag=null;
  const animations=[node.animate([{left:`${rect.left}px`,top:`${rect.top}px`},{left:`${point.left}px`,top:`${point.top}px`}],{duration:250,easing:'cubic-bezier(.2,.8,.2,1)',fill:'forwards'}),card.animate([
   {width:`${rect.width}px`,height:`${rect.height}px`,borderRadius:getComputedStyle(card).borderRadius,opacity:1,transform:'none'},
   {width:'54px',height:'54px',borderRadius:'27px',opacity:1,transform:'scale(.98)',offset:.58},
   {width:'54px',height:'54px',borderRadius:'27px',opacity:0,transform:`translateX(${direction*42}px) scale(.4)`}
  ],{duration:390,easing:'cubic-bezier(.3,0,.25,1)',fill:'forwards'})];
  const motion={node,animations};exitMotion=motion;
  void Promise.allSettled(animations.map(a=>a.finished)).then(()=>{if(exitMotion!==motion)return;remove();if(enabled)void inspect()});
 }
 function bindDrag(handle){
  handle.addEventListener('pointerdown',event=>{
   if(!event.isTrusted||event.button!==0||!event.isPrimary||exitMotion||event.composedPath().some(n=>n!==handle&&n instanceof Element&&n.matches('button,select,input,a')))return;
   const rect=host.getBoundingClientRect();drag={id:event.pointerId,x:event.clientX,y:event.clientY,rect,moved:false,handle};handle.setPointerCapture(event.pointerId);
  });
  handle.addEventListener('pointermove',event=>{
   if(!drag||drag.id!==event.pointerId)return;
   const dx=event.clientX-drag.x,dy=event.clientY-drag.y;
   if(!drag.moved&&Math.hypot(dx,dy)<5)return;
   event.preventDefault();drag.moved=true;surface.classList.remove('arriving');host.classList.add('dragging');
   const rect=fitFloatingRect({left:drag.rect.left+dx,top:drag.rect.top+dy},drag.rect,viewport());
   position=rememberFloatingRect(rect,viewport());Object.assign(host.style,{left:`${rect.left}px`,top:`${rect.top}px`,right:'auto',bottom:'auto'});
  });
  function finish(event){
   if(!drag||drag.id!==event.pointerId)return;
   const moved=drag.moved,active=drag.handle;drag=null;
   if(active.hasPointerCapture(event.pointerId))active.releasePointerCapture(event.pointerId);
   host?.classList.remove('dragging');if(moved){suppressClickUntil=performance.now()+400;void chrome.storage.local.set({floatingPosition:position}).catch(()=>{})}place();
  }
  handle.addEventListener('pointerup',finish);handle.addEventListener('pointercancel',finish);handle.addEventListener('lostpointercapture',finish);
 }
 function display(){
  if(host||!enabled)return;
  host=document.createElement('snowdown-video-helper');host.setAttribute('aria-label','SnowDown 视频下载提醒');
  const shadow=host.attachShadow({mode:'closed'}),style=document.createElement('style');style.textContent=css;shadow.append(style);
  surface=node('div','surface arriving','',shadow);surface.addEventListener('animationend',()=>surface?.classList.remove('arriving'),{once:true});const orb=node('button','orb','',surface);orb.setAttribute('aria-label','发现视频，展开下载设置');orb.title='SnowDown · 发现视频';orb.innerHTML=icon+'<span class="dot"></span>';
  const panel=node('section','panel','',surface);panel.setAttribute('aria-label','视频下载设置');
  const heading=node('div','heading','',panel),image=node('img','','',heading);image.src=chrome.runtime.getURL('icons/32.png');const brand=node('strong','','SnowDown',heading);node('span','','.',brand);
  bindDrag(orb);bindDrag(heading);
  const close=node('button','','×',heading);close.title='收起';close.setAttribute('aria-label','收起下载设置');
  const route=node('p','route','当前网页视频',panel),title=node('h2','','正在读取视频…',panel),author=node('p','author','',panel);
  const label=node('label','','下载画质',panel),quality=node('select','','',panel);quality.setAttribute('aria-label','悬浮下载画质');label.htmlFor='quality';quality.id='quality';
  const routeLabel=node('label','','下载方式',panel),destination=node('select','','',panel);routeLabel.htmlFor='destination';destination.id='destination';destination.setAttribute('aria-label','悬浮下载方式');destination.append(new Option('跟随插件设置','auto'),new Option('使用浏览器下载','browser'));
  const download=node('button','download','下载视频',panel),message=node('p','message','',panel);message.setAttribute('role','status');download.disabled=true;
  const foot=node('div','foot','',panel),retry=node('button','','重新读取',foot),disable=node('button','','关闭悬浮提醒',foot);
  const speak=text=>{message.textContent=text};
  async function parse(){
   if(busy)return;busy=true;download.disabled=true;retry.disabled=true;ticket=null;speak('使用当前网页会话读取…');
   const activeHost=host,activeUrl=location.href;
   try{const result=await send('float-parse');if(host!==activeHost||location.href!==activeUrl||!enabled)return;ticket=result;title.textContent=result.title;author.textContent=result.author;quality.replaceChildren(...result.streams.map(s=>new Option(s.label,String(s.index))));route.textContent=result.desktop?'已连接 · 下载交给 SnowDown':'浏览器直接下载';download.textContent=result.desktop?'发送到 SnowDown':'下载视频';download.disabled=false;speak('选择画质即可下载。');}
   catch(error){if(host===activeHost&&enabled){title.textContent='暂未读到视频';speak(error.message)}}finally{if(host===activeHost){busy=false;retry.disabled=false}}
  }
  orb.addEventListener('click',event=>{if(!event.isTrusted||performance.now()<suppressClickUntil||exitMotion)return;expanded=true;surface.classList.add('expanded');place();if(!ticket)void parse()});
  close.addEventListener('click',()=>{expanded=false;surface.classList.remove('expanded');place();orb.focus()});
  retry.addEventListener('click',event=>{if(event.isTrusted)void parse()});
  download.addEventListener('click',async event=>{if(!event.isTrusted||busy||!ticket)return;busy=true;download.disabled=true;try{speak((await send('float-download',{id:ticket.id,index:Number(quality.value),route:destination.value})).message);ticket=null;}catch(error){speak(error.message)}finally{busy=false;retry.disabled=false}});
  disable.addEventListener('click',async event=>{if(event.isTrusted){try{await send('float-disable');enabled=false;leave()}catch(error){speak(error.message)}}});
  panel.addEventListener('keydown',event=>{if(event.key==='Escape'){expanded=false;surface.classList.remove('expanded');place();orb.focus()}});
  document.documentElement.append(host);place();
 }
 async function inspect(){
  if(!enabled||document.hidden)return;
  if(location.href!==lastUrl){lastUrl=location.href;remove()}
  const video=[...document.querySelectorAll('video')].some(n=>{const r=n.getBoundingClientRect();return r.width>100&&r.height>70&&(n.currentSrc||n.querySelector('source')?.src||n.readyState>0)});
  if(video)display();else if(host&&!expanded)remove();
 }
 function schedule(){clearTimeout(scanTimer);scanTimer=setTimeout(()=>void inspect(),450)}
 void chrome.storage.local.get(['floatingEnabled','floatingPosition']).then(value=>{enabled=value.floatingEnabled!==false;position=value.floatingPosition||null;void inspect()});
 chrome.storage.onChanged.addListener((changes,area)=>{if(area!=='local')return;if(changes.floatingPosition){position=changes.floatingPosition.newValue||null;place()}if(changes.floatingEnabled){enabled=changes.floatingEnabled.newValue!==false;if(!enabled)leave();else void inspect()}});
 window.addEventListener('resize',()=>{if(drag){drag=null;host?.classList.remove('dragging')}place()});
 new MutationObserver(schedule).observe(document.documentElement,{childList:true,subtree:true});
 document.addEventListener('loadedmetadata',schedule,true);document.addEventListener('play',schedule,true);document.addEventListener('visibilitychange',schedule);
 setInterval(()=>void inspect(),2500);
})();
