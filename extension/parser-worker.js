import {parseVideo} from './sites.js';
import {hlsVariants} from './manifests.js';
import {genericVideo} from './media.js';
const threadId=crypto.randomUUID();
self.onmessage=async({data:{id,kind,page}})=>{try{let value;if(kind==='site')value=await parseVideo(page);else if(kind==='hls')value=hlsVariants(page.text,page.base);else value=genericVideo(page);self.postMessage({id,value,threadId})}catch(error){self.postMessage({id,error:error.message||'解析失败，请重试。'})}};
