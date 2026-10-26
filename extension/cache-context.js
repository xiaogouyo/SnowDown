import {siteOf} from './catalog.js';
export const cacheKey=(tabId,pageUrl,targetUrl)=>`${tabId}:${targetUrl?'preview':'current'}:${encodeURIComponent(pageUrl)}:${encodeURIComponent(targetUrl||pageUrl)}`;
export function cacheSite(key){try{return siteOf(decodeURIComponent(key.split(':')[3]))?.id}catch{return null}}
// Exact authentication names: b_lsid and csrf/device/analytics token refreshes
// do not mean that the account used for read-only playback has changed.
export function authCookieSite(cookie){
 const site=siteOf('https://'+cookie.domain.replace(/^\./,''));if(!site)return null;
 const names={bilibili:['SESSDATA','DedeUserID'],douyin:['sessionid','sessionid_ss','sid_guard','uid_tt'],xiaohongshu:['web_session']};
 const relevant=names[site.id]||['session','sessionid','session_id','auth_token','access_token','SID','SSID','LOGIN_INFO'];
 return relevant.includes(cookie.name)?site.id:null;
}
export async function retryStableParse(work,stable){
 try{return await work()}catch(error){if(error.code!=='PARSE_CONTEXT_CHANGED'||!error.retryable||!await stable())throw error;return work()}
}
