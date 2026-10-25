// Real MV3 regression on isolated, controlled Douyin pages. No user browser profile.
import {pathToFileURL} from 'node:url';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import path from 'node:path';import assert from 'node:assert/strict';
const {chromium}=await import(process.env.SNOWDOWN_PLAYWRIGHT_PATH?pathToFileURL(process.env.SNOWDOWN_PLAYWRIGHT_PATH).href:'playwright');
const extension=path.resolve('src-tauri/resources/browser-extension'),out=path.resolve('.tools/v026-douyin-browser');await mkdir(out,{recursive:true});
const context=await chromium.launchPersistentContext(path.join(out,'profile-'+Date.now()),{channel:'chromium',headless:true,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker'),extensionId=worker.url().split('/')[2];
const media=await readFile('.tools/v024-fixture.mp4'),id='1234567890123456789',next='9876543210987654321';
const item=(id,title)=>({aweme_id:id,desc:title,author:{nickname:'测试作者'},video:{width:720,height:1280,play_addr:{url_list:['https://fixture.douyinvod.com/video.mp4']}}});
const errors=[];context.on('page',page=>page.on('pageerror',error=>errors.push(error.message)));
await worker.evaluate(()=>chrome.storage.local.set({syncAccounts:false,preferDesktop:false,floatingEnabled:true}));
await context.route('https://fixture.douyinvod.com/**',route=>route.fulfill({contentType:'video/mp4',body:media}));
await context.route('https://www.douyin.com/**',route=>{
 const url=new URL(route.request().url());
 if(url.pathname.startsWith('/aweme/'))return route.fulfill({json:{aweme_list:[item(next,'相邻视频，不能误选'),item(id,'精选列表捕获成功')]}});
 const xhr=url.searchParams.get('xhr')==='1';
 const request=xhr?`const x=new XMLHttpRequest();x.open('GET','/aweme/v1/web/tab/feed/');x.responseType='json';x.onload=()=>document.body.dataset.ready='true';x.send();`:`fetch(new URL('/aweme/v1/web/tab/feed/',location.href)).then(r=>r.json()).then(()=>document.body.dataset.ready='true');`;
 return route.fulfill({contentType:'text/html; charset=utf-8',body:`<!doctype html><meta charset=utf-8><title>精选页读取测试</title><video style="width:800px;height:450px" controls src="https://fixture.douyinvod.com/video.mp4"></video><script>${url.searchParams.has('react')?'document.body.dataset.ready="true"':request}</script>`});
});
try {
 const page=await context.newPage();await page.goto(`https://www.douyin.com/jingxuan?modal_id=${id}`);await page.waitForSelector('body[data-ready=true]');
 const tab=(await worker.evaluate(()=>chrome.tabs.query({url:'https://www.douyin.com/*'})))[0];
 const popup=await context.newPage();await popup.goto(`chrome-extension://${extensionId}/popup.html?tab=${tab.id}`);
 await popup.locator('#parse').click();await popup.waitForSelector('#video:not([hidden])');assert.equal(await popup.locator('#videoTitle').innerText(),'精选列表捕获成功');await popup.screenshot({path:path.join(out,'feed-parsed.png')});
 await page.goto(`https://www.douyin.com/jingxuan?modal_id=${id}&xhr=1`);await page.waitForSelector('body[data-ready=true]');await page.waitForFunction(()=>window.__snowdownMediaDetails?.length>0);const xhrResult=await popup.evaluate(async tabId=>chrome.runtime.sendMessage({type:'parse',tabId}),tab.id);assert.equal(xhrResult.ok,true);assert.equal(xhrResult.data.video.title,'精选列表捕获成功');
 await page.goto(`https://www.douyin.com/jingxuan?modal_id=${id}&react=1`);
 await page.evaluate(id=>{document.querySelector('video').__reactFiber$test={memoizedProps:{item:{awemeId:id,desc:'已播放播放器恢复成功',video:{width:720,height:1280,playAddr:[{src:'https://fixture.douyinvod.com/react.mp4'}]}}},return:null}},id);
 await popup.locator('#parse').click();await popup.waitForFunction(()=>document.querySelector('#videoTitle').textContent==='已播放播放器恢复成功');
 await page.evaluate(next=>history.pushState({},'',`/jingxuan?modal_id=${next}`),next);
 const stale=await popup.evaluate(async tabId=>chrome.runtime.sendMessage({type:'parse',tabId}),tab.id);assert.equal(stale.ok,false);
 await page.evaluate(next=>{document.querySelector('video').__reactFiber$test.memoizedProps.item.awemeId=next},next);
 const current=await popup.evaluate(async tabId=>chrome.runtime.sendMessage({type:'parse',tabId}),tab.id);assert.equal(current.ok,true);assert.equal(current.data.page.id,next);
 const nativeSource=(await readFile('extension/page.js','utf8')).replace('export function capturePage','function capturePage');
 const nativeScript=(await readFile('src-tauri/src/site_video.js','utf8')).replace('__VIDEO_ID__',JSON.stringify(next)).replace('__CAPTURE_PAGE_SOURCE__',nativeSource);
 const native=JSON.parse(await page.evaluate(nativeScript));assert.equal(native.detail.aweme_id,next);assert.ok(native.detail.video.play_addr.url_list[0].endsWith('react.mp4'));
 assert.deepEqual(errors,[]);const result={success:true,version:await worker.evaluate(()=>chrome.runtime.getManifest().version),feedFetch:true,jsonXHR:true,reactPlayerFallback:true,modalSwitchRejectsStale:true,desktopSharedReader:true,errors};await writeFile(path.join(out,'verification.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result,null,2));
}finally{await context.close()}
