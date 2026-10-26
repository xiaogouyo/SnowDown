let creating;
async function ensure(){
 if(creating)return creating;
 creating=(async()=>{const url=chrome.runtime.getURL('parser.html');const contexts=await chrome.runtime.getContexts({contextTypes:['OFFSCREEN_DOCUMENT'],documentUrls:[url]});if(!contexts.length)await chrome.offscreen.createDocument({url:'parser.html',reasons:['WORKERS'],justification:'Run two bounded video parser workers outside web pages and the popup UI.'})})().finally(()=>{creating=null});return creating;
}
export async function parseInWorker(kind,page){await ensure();let timer;try{const result=await Promise.race([chrome.runtime.sendMessage({target:'snowdown-parser',kind,page}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('后台解析超时，请重新读取。')),30000)})]);if(!result?.ok)throw Error(result?.error||'后台解析暂不可用，请刷新插件后重试。');return result.value;}finally{clearTimeout(timer)}}
