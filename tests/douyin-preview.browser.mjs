// Real MV3 UI on controlled profile/search/feed pages, including anchorless cards.
import {pathToFileURL} from 'node:url';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import path from 'node:path';import assert from 'node:assert/strict';
const {chromium}=await import(process.env.SNOWDOWN_PLAYWRIGHT_PATH?pathToFileURL(process.env.SNOWDOWN_PLAYWRIGHT_PATH).href:'playwright');
const out=path.resolve('.tools/v029-preview-qa'),extension=path.resolve('src-tauri/resources/browser-extension');await mkdir(path.join(out,'downloads'),{recursive:true});
const context=await chromium.launchPersistentContext(path.join(out,'profile-'+Date.now()),{channel:'chromium',headless:true,viewport:{width:1400,height:950},args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker'),first='1234567890123456789',second='9876543210987654321',errors=[];
context.on('page',page=>page.on('pageerror',error=>errors.push(error.stack)));
await worker.evaluate(()=>chrome.storage.local.set({preferDesktop:false,syncAccounts:false,floatingEnabled:true}));
const item=(id,title)=>({awemeId:id,desc:title,author:{nickname:'测试作者',private:'not copied'},video:{width:720,height:1280,playAddr:[{src:'https://fixture.douyinvod.com/video.mp4'}],bitRateAudio:[{audioMeta:{bitrate:128000,urlList:{mainUrl:'https://fixture.douyinvod.com/audio.mp4'}}}]}});
const items=[item(first,'预览 A'),item(second,'预览 B')],media=await readFile('.tools/v024-fixture.mp4'),cover=await readFile('extension/icons/128.png');
await context.route('https://fixture.douyinvod.com/**',route=>route.fulfill({contentType:'video/mp4',body:media}));
await context.route('https://fixture.douyinpic.com/**',route=>route.fulfill({contentType:'image/png',body:cover}));
await context.route('https://www.douyin.com/**',route=>{
 const url=new URL(route.request().url());
 if(url.pathname.startsWith('/aweme/'))return route.fulfill({json:{data:items.map(aweme_info=>({aweme_info})),privateAccount:'not copied'}});
 const player=['/','/jingxuan'].includes(url.pathname),search=url.pathname.startsWith('/search');
 const content=player?'<div class=player><video controls src="https://fixture.douyinvod.com/video.mp4"></video></div>':`<div class=scroll><section>${items.map((_,i)=>`<article aria-label="卡片 ${i}">${i?'<div class=cover style="background-image:url(https://fixture.douyinpic.com/cover.png)"></div>':'<img class=cover alt="卡片封面" src="https://fixture.douyinpic.com/cover.png">'}</article>`).join('')}</section></div>`;
 const script=search?`fetch('/aweme/v1/web/general/search/single/').then(r=>r.json()).then(()=>document.body.dataset.ready='true');document.querySelectorAll('.cover').forEach((node,index)=>node.setAttribute('data-aweme-id',${JSON.stringify([first,second])}[index]));`:`const rows=${JSON.stringify(items)};document.querySelectorAll('.cover,video').forEach((node,index)=>node.__reactFiber$test={memoizedProps:{awemeInfo:rows[index]},return:null});document.body.dataset.ready='true';`;
 return route.fulfill({contentType:'text/html;charset=utf-8',body:`<!doctype html><meta charset=utf-8><title>抖音预览回归</title><style>body{margin:0;padding:70px;background:#161722;color:white;font:16px system-ui}.scroll{width:850px;height:550px;overflow:auto;margin-top:30px}section{padding-top:50px}article{width:350px;height:420px;margin:0 0 30px 20px}.cover{display:block;width:350px;height:350px;object-fit:contain;border-radius:16px;background-color:#283244;background-size:cover}.player,video{width:900px;height:650px}</style>${content}<script>${script}</script>`});
});
const content=(tabId,type,data={})=>worker.evaluate(async({tabId,type,data})=>(await chrome.scripting.executeScript({target:{tabId},func:(type,data)=>chrome.runtime.sendMessage({type,...data}),args:[type,data]}))[0].result,{tabId,type,data});
async function openFirst(page,title='预览 A'){
 await page.waitForFunction(()=>document.querySelector('snowdown-preview-helper')?.style.visibility==='visible');
 const host=page.locator('snowdown-preview-helper').first(),r=await host.boundingBox();assert.ok(r);assert.equal(Math.round(r.width),30);assert.equal(Math.round(r.height),30);
 await page.mouse.click(r.x+15,r.y+15);await page.waitForFunction(title=>[...document.querySelectorAll('snowdown-preview-helper')].some(node=>node.title===title),title);await page.waitForTimeout(350);const expanded=await host.boundingBox();assert.equal(Math.round(expanded.width),288);return host;
}
try{
 const page=await context.newPage();await page.goto('https://www.douyin.com/user/fixture');await page.waitForSelector('body[data-ready=true]');
 const tab=(await worker.evaluate(()=>chrome.tabs.query({url:'https://www.douyin.com/*'})))[0];
 const session=await context.newCDPSession(page);await session.send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:path.join(out,'downloads')});
 const popup=await context.newPage();await popup.goto(`chrome-extension://${worker.url().split('/')[2]}/popup.html?tab=${tab.id}`);const status=await popup.evaluate(()=>chrome.runtime.sendMessage({type:'status'}));await worker.evaluate(binding=>chrome.storage.local.set({lastBinding:binding,preferDesktop:false,syncAccounts:false}),status.data?.binding||'');await page.bringToFront();
 const host=await openFirst(page);await page.screenshot({path:path.join(out,'profile-menu.png')});
 const rect=await host.boundingBox(),jobPromise=context.waitForEvent('page');await page.mouse.click(rect.x+130,rect.y+65);const job=await jobPromise;await job.waitForFunction(()=>/下载已开始|下载完成/.test(document.querySelector('#status')?.textContent||''));
 let complete;for(let n=0;n<80;n++){complete=(await worker.evaluate(()=>chrome.downloads.search({limit:10,orderBy:['-startTime']}))).find(d=>d.state==='complete'&&d.filename.endsWith('.mp4'));if(complete)break;await page.waitForTimeout(100)}assert.ok(complete);assert.ok(complete.filename.startsWith(path.join(out,'downloads')));execFileSync(path.resolve('src-tauri/resources/ffmpeg.exe'),['-v','error','-i',complete.filename,'-map','0:v:0','-map','0:a:0','-f','null','-'],{windowsHide:true,stdio:'pipe'});await page.bringToFront();
 const ticket=await content(tab.id,'preview-parse',{url:`https://www.douyin.com/video/${first}`});assert.equal(ticket.ok,true,ticket.error);assert.equal(ticket.data.title,'预览 A');assert.ok(ticket.data.rows.some(row=>row.kind==='video'));
 await page.evaluate(()=>document.querySelector('.scroll').scrollTop=50);
 await page.waitForFunction(()=>{const target=document.querySelector('[data-snowdown-preview-key]'),host=document.querySelector('snowdown-preview-helper');return Math.abs(host.getBoundingClientRect().top-target.getBoundingClientRect().top-6)<1});
 assert.equal(await host.evaluate(node=>getComputedStyle(node).transitionProperty),'none');
 await worker.evaluate(()=>{globalThis.__previewScriptCalls=0;const original=chrome.scripting.executeScript;chrome.scripting.executeScript=function(...args){globalThis.__previewScriptCalls++;return original.apply(this,args)}});
 for(let i=0;i<20;i++){await page.evaluate(i=>document.querySelector('.scroll').scrollTop=50+i,i);await page.waitForTimeout(18)}
 const scrollCalls=await worker.evaluate(()=>globalThis.__previewScriptCalls);assert.ok(scrollCalls<=4,`Excessive discovery while scrolling: ${scrollCalls}`);
 await page.evaluate(()=>document.querySelector('.scroll').scrollTop=150);await page.waitForFunction(()=>document.querySelector('snowdown-preview-helper')?.style.visibility==='hidden');
 await page.evaluate(()=>document.querySelector('.scroll').scrollTop=430);await page.waitForFunction(()=>[...document.querySelectorAll('snowdown-preview-helper')].filter(node=>node.style.visibility==='visible').length===1);
 await page.waitForTimeout(650);const bg=await content(tab.id,'preview-parse',{url:`https://www.douyin.com/video/${second}`});assert.equal(bg.ok,true,bg.error);assert.equal(bg.data.title,'预览 B');
 await page.evaluate(()=>document.querySelector('.cover').__reactFiber$test.memoizedProps.awemeInfo.awemeId='1111111111111111111');const stale=await content(tab.id,'preview-download',{id:ticket.data.id,index:0,kind:'video'});assert.equal(stale.ok,false);
 await page.goto('https://www.douyin.com/search/fixture');await page.waitForFunction(()=>window.__snowdownMediaDetails?.length>0);await openFirst(page);assert.equal(await page.evaluate(()=>JSON.stringify(window.__snowdownMediaDetails).includes('not copied')),false);
 for(const pathname of ['/jingxuan','/']){await page.goto('https://www.douyin.com'+pathname);await openFirst(page);}
 assert.deepEqual(errors,[]);const result={success:true,version:await worker.evaluate(()=>chrome.runtime.getManifest().version),profileWithoutAnchors:true,profileVideoAndAudioDecoded:true,backgroundCover:true,searchMetadata:true,selectedFeedWithoutUrlId:true,rootFeedWithoutUrlId:true,smallTranslucentButton:true,nestedScrollAnchoredWithoutTween:true,discoveryCallsDuringScroll:scrollCalls,staleCardRejected:true,errors};await writeFile(path.join(out,'verification.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result,null,2));
}finally{await context.close()}
