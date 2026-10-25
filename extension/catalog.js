import sites from '../shared/platforms.json';
export {sites};
export function siteOf(input){try{const url=new URL(input);if(!['https:','http:'].includes(url.protocol)||url.username||url.password)return null;return sites.find(site=>site.hosts.some(host=>url.hostname===host||url.hostname.endsWith('.'+host)))||null}catch{return null}}
export function previewUrl(input,base){try{const url=new URL(input,base),site=siteOf(url.href);if(!site||url.username||url.password||url.port)return null;
 if(site.id==='bilibili'&&!/\/video\/(?:BV[\w]{10}|av\d+)/.test(url.pathname))return null;
 if(site.id==='douyin'&&!/\/video\/\d{8,24}/.test(url.pathname)&&!/^\d{8,24}$/.test(url.searchParams.get('modal_id')||''))return null;
 if(site.id==='xiaohongshu'&&!/\/(?:explore|discovery\/item)\/[a-f\d]{24}/i.test(url.pathname))return null;
 if(site.id==='youtube'&&!((url.pathname==='/watch'&&url.searchParams.has('v'))||/^\/(?:shorts|live)\/[\w-]+/.test(url.pathname)||url.hostname==='youtu.be'&&url.pathname.length>2))return null;
 if(site.id==='twitter'&&!/\/status\/\d+/.test(url.pathname))return null;
 if(site.id==='instagram'&&!/^\/(?:p|reel|reels|tv)\/[\w-]+/.test(url.pathname))return null;
 if(site.id==='tiktok'&&!/\/video\/\d+/.test(url.pathname))return null;
 if(url.pathname==='/'||/^\/(?:login|signup|settings|search|explore|feed|home)\/?$/.test(url.pathname)&&!url.search)return null;
 url.hash='';return url.href;
 }catch{return null}}
