import {createLimiter} from './parse-cache.js';
const schedule=createLimiter(2,12),slots=[];
let sequence=0,idle;
function retire(){clearTimeout(idle);idle=setTimeout(()=>{if(slots.some(s=>s.busy))return;for(const slot of slots)slot.worker.terminate();slots.length=0;},30000)}
function run(kind,page){return schedule(()=>new Promise((resolve,reject)=>{
 clearTimeout(idle);let slot=slots.find(s=>!s.busy);
 if(!slot){slot={worker:new Worker('parser-worker.js',{type:'module'}),busy:false};slots.push(slot)}
 slot.busy=true;const id=++sequence,worker=slot.worker;
 const finish=(error,value)=>{clearTimeout(timer);worker.onmessage=null;worker.onerror=null;slot.busy=false;if(error){worker.terminate();const i=slots.indexOf(slot);if(i>=0)slots.splice(i,1);reject(error)}else resolve(value);retire()};
 const timer=setTimeout(()=>finish(Error('解析超时，请稍后重试。')),22000);
 worker.onmessage=event=>{if(event.data?.id===id)finish(event.data.error?Error(event.data.error):null,{value:event.data.value,threadId:event.data.threadId})};
 worker.onerror=()=>finish(Error('解析线程已重启，请重试。'));
 try{worker.postMessage({id,kind,page})}catch{finish(Error('视频数据无法传递给解析线程。'))}
}))}
chrome.runtime.onMessage.addListener((message,sender,respond)=>{
 if(message?.target!=='snowdown-parser'||sender.id!==chrome.runtime.id||sender.tab||sender.url&&sender.url!==chrome.runtime.getURL('background.js'))return;
 if(!['site','generic','hls'].includes(message.kind))return;
 run(message.kind,message.page).then(result=>respond({ok:true,...result}),error=>respond({ok:false,error:error.message}));return true;
});
