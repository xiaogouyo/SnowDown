import {platformOf,safeUrl} from './core.js';
export function supportedVideoUrl(input){
 try{const u=new URL(input),site=platformOf(input);if(!site)return false;
 return site==='bilibili'?/\/video\/(BV[\w]{10}|av\d+)/.test(u.pathname):site==='douyin'?/\/video\/\d{8,24}/.test(u.pathname)||/^\d{8,24}$/.test(u.searchParams.get('modal_id')||''):/\/(explore|discovery\/item)\/[a-f\d]{24}/i.test(u.pathname);
 }catch{return false}
}
export function contentSender(sender,runtimeId){return sender.id===runtimeId&&Number.isInteger(sender.tab?.id)&&sender.frameId===0&&safeUrl(sender.url)&&sender.origin!=='null'}
export function currentTicket(ticket,sender){return !!ticket&&ticket.tabId===sender.tab.id&&ticket.url===sender.url&&Date.now()-ticket.createdAt<120000}
export function fitFloatingRect(position,size,viewport,margin=8){
 const width=Math.min(size.width,Math.max(1,viewport.width-2*margin)),height=Math.min(size.height,Math.max(1,viewport.height-2*margin));
 return {left:Math.max(margin,Math.min(position.left,viewport.width-width-margin)),top:Math.max(margin,Math.min(position.top,viewport.height-height-margin)),width,height};
}
export function floatingOrigin(saved,viewport,expanded=false){
 const valid=saved&&Number.isFinite(saved.x)&&Number.isFinite(saved.y)&&saved.x>=0&&saved.x<=1&&saved.y>=0&&saved.y<=1;
 const center=valid?{x:saved.x*viewport.width,y:saved.y*viewport.height}:{x:viewport.width-(viewport.width<=600?12:24)-27,y:viewport.height-(viewport.width<=600?80:112)-27};
 const size=expanded?{width:Math.max(54,Math.min(340,viewport.width-24)),height:Math.max(54,Math.min(480,viewport.height-138))}:{width:54,height:54};
 return fitFloatingRect({left:center.x+27-size.width,top:center.y+27-size.height},size,viewport);
}
export function rememberFloatingRect(rect,viewport){return {x:Math.max(0,Math.min(1,(rect.left+rect.width-27)/viewport.width)),y:Math.max(0,Math.min(1,(rect.top+rect.height-27)/viewport.height))}}
