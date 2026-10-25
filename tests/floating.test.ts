import{describe,it,expect,vi,afterEach}from'vitest'
// @ts-expect-error Extension JS is bundled separately.
import{supportedVideoUrl,contentSender,currentTicket}from'../extension/floating-core.js'
// @ts-expect-error MAIN-world function is intentionally self-contained.
import{capturePage}from'../extension/page.js'
describe('floating video boundaries',()=>{
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
