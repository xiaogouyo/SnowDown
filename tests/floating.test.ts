import{describe,it,expect,vi,afterEach}from'vitest'
// @ts-expect-error Extension JS is bundled separately.
import{supportedVideoUrl,contentSender,currentTicket,fitFloatingRect,floatingOrigin,rememberFloatingRect}from'../extension/floating-core.js'
// @ts-expect-error MAIN-world function is intentionally self-contained.
import{capturePage}from'../extension/page.js'
describe('floating video boundaries',()=>{
 it('clamps dragged shapes to each edge and restores their normalized position',()=>{
  const viewport={width:1200,height:800},size={width:54,height:54}
  expect(fitFloatingRect({left:-100,top:-90},size,viewport)).toMatchObject({left:8,top:8})
  expect(fitFloatingRect({left:1500,top:1200},size,viewport)).toMatchObject({left:1138,top:738})
  const rect={left:220,top:150,width:54,height:54}
  const saved=rememberFloatingRect(rect,viewport)
  expect(floatingOrigin(saved,viewport)).toEqual(rect)
 })
 it('keeps expanded cards within the viewport at every sphere position and after resize',()=>{
  for(const viewport of [{width:1440,height:1000},{width:480,height:400},{width:210,height:220}])for(const x of [0,.5,1])for(const y of [0,.5,1]){
   const rect=floatingOrigin({x,y},viewport,true)
   expect(rect.left).toBeGreaterThanOrEqual(8);expect(rect.top).toBeGreaterThanOrEqual(8)
   expect(rect.left+rect.width).toBeLessThanOrEqual(viewport.width-8)
   expect(rect.top+rect.height).toBeLessThanOrEqual(viewport.height-8)
  }
 })
 it('rejects malformed saved positions and keeps the default corner when no position exists',()=>{
  const viewport={width:1200,height:800},fallback=floatingOrigin(null,viewport)
  for(const value of [{x:NaN,y:0},{x:2,y:1},{x:0,y:Infinity},{x:'1',y:0}])expect(floatingOrigin(value,viewport)).toEqual(fallback)
 })
 it('only recognizes actual video routes on official platform domains',()=>{
  expect(supportedVideoUrl('https://www.bilibili.com/video/BV17x411w7KC')).toBe(true)
  expect(supportedVideoUrl('https://www.douyin.com/jingxuan?modal_id=7689371847216893193')).toBe(true)
  expect(supportedVideoUrl('https://www.xiaohongshu.com/explore/1234567890abcdef12345678')).toBe(true)
  expect(supportedVideoUrl('https://douyin.com.evil.test/video/7689371847216893193')).toBe(false)
  expect(supportedVideoUrl('https://www.bilibili.com/')).toBe(false)
 })
 it('binds downloads to the same top-level sender, page, and unexpired ticket',()=>{
  const sender={id:'our-id',tab:{id:7},frameId:0,url:'https://www.bilibili.com/video/BV17x411w7KC'}
  expect(contentSender(sender,'our-id')).toBeTruthy()
  expect(contentSender({...sender,frameId:1},'our-id')).toBeFalsy()
  expect(contentSender({...sender,id:'other'},'our-id')).toBeFalsy()
  const ticket={tabId:7,url:sender.url,createdAt:Date.now()}
  expect(currentTicket(ticket,sender)).toBe(true)
  expect(currentTicket(ticket,{...sender,tab:{id:8}})).toBe(false)
  expect(currentTicket(ticket,{...sender,url:'https://example.com/'})).toBe(false)
  expect(currentTicket({...ticket,createdAt:Date.now()-120001},sender)).toBe(false)
 })
 it('platform metadata capture skips resource style scans until explicitly requested',async()=>{
  document.body.innerHTML='<div style="background-image:url(https://example.com/a.jpg)"></div>'
  const styles=vi.spyOn(window,'getComputedStyle')
  await capturePage();expect(styles).not.toHaveBeenCalled()
  await capturePage({resources:true});expect(styles).toHaveBeenCalled()
 })
})
afterEach(()=>{document.body.innerHTML='';vi.restoreAllMocks()})
