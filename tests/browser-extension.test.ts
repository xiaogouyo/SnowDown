import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import BrowserExtension from '../src/components/BrowserExtension.vue'
const api=vi.hoisted(()=>({desktop:true,browserStatus:vi.fn(),installBrowserExtension:vi.fn(),saveBrowserExtensionZip:vi.fn(),checkBrowserExtensionUpdate:vi.fn(),saveBrowserExtensionUpdate:vi.fn(),openBrowserExtensionRelease:vi.fn()}))
vi.mock('../src/api',()=>({...api,__v_isRef:false}))
let wrapper:VueWrapper
const modal=()=>document.body.querySelector('.extension-modal')!
const download=()=>modal().querySelector<HTMLButtonElement>('.package-download')!
const checkUpdate=()=>modal().querySelector<HTMLButtonElement>('.update-heading button')!
const release={version:'0.1.9',title:'统一左上角',notes:'更新说明',url:'https://github.com/xiaogouyo/SnowDown/releases/tag/extension-v0.1.9',fileName:'SnowDown_BrowserExtension_0.1.9.zip',size:204164,publishedAt:'2026-10-25'}
beforeEach(async()=>{
  vi.clearAllMocks()
  api.browserStatus.mockResolvedValue([{id:'chrome',name:'Google Chrome',installed:true,connected:false},{id:'edge',name:'Microsoft Edge',installed:true,connected:true}])
  api.installBrowserExtension.mockResolvedValue('C:\\extension')
  api.saveBrowserExtensionZip.mockResolvedValue('D:\\下载\\SnowDown.zip')
  api.checkBrowserExtensionUpdate.mockResolvedValue(release)
  api.saveBrowserExtensionUpdate.mockResolvedValue('D:\\下载\\SnowDown_BrowserExtension_0.1.9.zip')
  api.openBrowserExtensionRelease.mockResolvedValue(undefined)
  wrapper=mount(BrowserExtension,{attachTo:document.body})
  await wrapper.get('.extension-tile').trigger('click');await flushPromises()
})
afterEach(()=>{wrapper.unmount();document.body.innerHTML=''})
describe('browser extension local installation',()=>{
  it('checks GitHub on request and compares the running plugin instead of bundled files',async()=>{
    expect(api.checkBrowserExtensionUpdate).not.toHaveBeenCalled();api.browserStatus.mockResolvedValue([{id:'edge',name:'Microsoft Edge',installed:true,connected:true,extensionVersion:'0.1.8',bundledVersion:'0.1.6'}]);checkUpdate().click();await flushPromises();expect(api.checkBrowserExtensionUpdate).toHaveBeenCalledTimes(1);expect(modal().textContent).toContain('最新插件 v0.1.9');expect(modal().textContent).toContain('发现新版插件');modal().querySelector<HTMLButtonElement>('.update-actions .primary')!.click();await flushPromises();expect(api.saveBrowserExtensionUpdate).toHaveBeenCalledWith(release);expect(modal().textContent).toContain('通过 SHA256 校验');
  })
  it('shows unknown clients honestly and does not overwrite newer plugins with bundled files',async()=>{
    api.browserStatus.mockResolvedValue([{id:'edge',name:'Microsoft Edge',installed:true,connected:true,bundledVersion:'0.1.6'}]);checkUpdate().click();await flushPromises();expect(modal().textContent).toContain('连接插件后可比较');modal().querySelector<HTMLButtonElement>('.browser-choice button')!.click();await flushPromises();expect(api.installBrowserExtension).not.toHaveBeenCalled();expect(api.saveBrowserExtensionUpdate).toHaveBeenCalled();
  })
  it('keeps current versions current and reports GitHub failures without a false latest result',async()=>{
    api.browserStatus.mockResolvedValue([{id:'edge',name:'Microsoft Edge',installed:true,connected:true,extensionVersion:'0.1.9',bundledVersion:'0.1.9'}]);checkUpdate().click();await flushPromises();expect(modal().textContent).toContain('已是最新版本');api.checkBrowserExtensionUpdate.mockRejectedValueOnce(new Error('GitHub 限流'));checkUpdate().click();await flushPromises();expect(modal().textContent).toContain('GitHub 限流');expect(modal().querySelector('.update-result')).toBeNull();
  })
  it('serializes checks and does not report cancelled or failed downloads as saved',async()=>{
    let finish!:(value:typeof release)=>void;api.checkBrowserExtensionUpdate.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));checkUpdate().click();await flushPromises();checkUpdate().click();expect(api.checkBrowserExtensionUpdate).toHaveBeenCalledTimes(1);expect(checkUpdate().disabled).toBe(true);finish(release);await flushPromises();api.saveBrowserExtensionUpdate.mockResolvedValueOnce(null);modal().querySelector<HTMLButtonElement>('.update-actions .primary')!.click();await flushPromises();expect(modal().textContent).not.toContain('已下载并通过');api.saveBrowserExtensionUpdate.mockRejectedValueOnce(new Error('插件安装包校验失败'));modal().querySelector<HTMLButtonElement>('.update-actions .primary')!.click();await flushPromises();expect(modal().textContent).toContain('校验失败');expect(modal().textContent).not.toContain('已下载并通过');
  })
  it('reports running and bundled versions and offers updates for old clients',async()=>{
    api.browserStatus.mockResolvedValue([{id:'edge',name:'Microsoft Edge',installed:true,connected:true,extensionVersion:'0.1.5',bundledVersion:'0.1.6',updateAvailable:true,refreshRequired:false}]);
    wrapper.unmount();wrapper=mount(BrowserExtension,{attachTo:document.body});await wrapper.get('.extension-tile').trigger('click');await flushPromises();
    expect(modal().textContent).toContain('插件 v0.1.5');expect(modal().textContent).toContain('可更新至 v0.1.6');expect(modal().querySelector('.browser-choice button')?.textContent).toBe('更新插件');
  })
  it('does not claim an unreported legacy version is current',async()=>{
    api.browserStatus.mockResolvedValue([{id:'edge',name:'Microsoft Edge',installed:true,connected:true,bundledVersion:'0.1.6',refreshRequired:true}]);wrapper.unmount();wrapper=mount(BrowserExtension,{attachTo:document.body});await wrapper.get('.extension-tile').trigger('click');await flushPromises();expect(modal().textContent).toContain('旧插件未报告版本');expect(modal().textContent).toContain('刷新扩展与网页');
  })
  it('replaces long instructions with a ZIP fallback and five accessible icon steps',()=>{
    expect(modal().querySelectorAll('.browser-choice')).toHaveLength(2)
    expect(modal().querySelectorAll('.extension-flow li')).toHaveLength(5)
    expect(modal().querySelectorAll('.flow-stage[tabindex="0"][aria-label]')).toHaveLength(5)
    expect(modal().querySelector('.extension-steps,.extension-note,code')).toBeNull()
    expect(modal().textContent).not.toContain('普通 Windows 浏览器要求用户确认')
    expect(modal().textContent).not.toContain('浏览器账号仅在本机同步')
    expect(download().textContent).toContain('下载 ZIP')
  })
  it('keeps local ZIP export usable when automatic linking fails',async()=>{
    api.installBrowserExtension.mockRejectedValueOnce(new Error('无法打开扩展管理页'))
    modal().querySelector<HTMLButtonElement>('.browser-choice button')!.click();await flushPromises()
    expect(modal().querySelector('[role="alert"]')?.textContent).toContain('无法打开扩展管理页')
    expect(download().disabled).toBe(false)
    download().click();await flushPromises()
    expect(api.saveBrowserExtensionZip).toHaveBeenCalledTimes(1)
    expect(modal().textContent).toContain('已保存到本机')
    expect(modal().querySelector('[role="alert"]')).toBeNull()
  })
  it('does not report success after cancelling Save and prevents duplicate exports',async()=>{
    api.saveBrowserExtensionZip.mockResolvedValueOnce(null)
    download().click();await flushPromises()
    expect(modal().textContent).not.toContain('已保存到本机')
    let finish!:(path:string)=>void
    api.saveBrowserExtensionZip.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
    download().click();await flushPromises();download().click();await flushPromises()
    expect(api.saveBrowserExtensionZip).toHaveBeenCalledTimes(2)
    expect(download().disabled).toBe(true)
    finish('D:\\插件.zip');await flushPromises()
    expect(download().disabled).toBe(false)
    expect(modal().textContent).toContain('已保存到本机')
  })
  it('keeps fallback available if browser detection is unavailable and reports write errors',async()=>{
    wrapper.unmount();api.browserStatus.mockRejectedValue(new Error('unavailable'))
    wrapper=mount(BrowserExtension,{attachTo:document.body});await wrapper.get('.extension-tile').trigger('click');await flushPromises()
    expect(download().disabled).toBe(false)
    api.saveBrowserExtensionZip.mockRejectedValueOnce(new Error('保存目录只读'))
    download().click();await flushPromises()
    expect(modal().querySelector('[role="alert"]')?.textContent).toContain('保存目录只读')
    expect(modal().textContent).not.toContain('已保存到本机')
  })
})
