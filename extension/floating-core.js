import {platformOf,safeUrl} from './core.js';
export function supportedVideoUrl(input){
 try{const u=new URL(input),site=platformOf(input);if(!site)return false;
 return site==='bilibili'?/\/video\/(BV[\w]{10}|av\d+)/.test(u.pathname):site==='douyin'?/\/video\/\d{8,24}/.test(u.pathname)||/^\d{8,24}$/.test(u.searchParams.get('modal_id')||''):/\/(explore|discovery\/item)\/[a-f\d]{24}/i.test(u.pathname);
 }catch{return false}
}
export function contentSender(sender,runtimeId){return sender.id===runtimeId&&Number.isInteger(sender.tab?.id)&&sender.frameId===0&&safeUrl(sender.url)&&sender.origin!=='null'}
export function currentTicket(ticket,sender){return !!ticket&&ticket.tabId===sender.tab.id&&ticket.url===sender.url&&Date.now()-ticket.createdAt<120000}
