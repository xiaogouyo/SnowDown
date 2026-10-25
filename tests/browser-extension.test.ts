import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import BrowserExtension from '../src/components/BrowserExtension.vue'
const api=vi.hoisted(()=>({desktop:true,browserStatus:vi.fn(),installBrowserExtension:vi.fn(),saveBrowserExtensionZip:vi.fn()}))
vi.mock('../src/api',()=>({...api,__v_isRef:false}))
let wrapper:VueWrapper
const modal=()=>document.body.querySelector('.extension-modal')!
const download=()=>modal().querySelector<HTMLButtonElement>('.package-download')!
beforeEach(async()=>{
  vi.clearAllMocks()
  api.browserStatus.mockResolvedValue([{id:'chrome',name:'Google Chrome',installed:true,connected:false},{id:'edge',name:'Microsoft Edge',installed:true,connected:true}])
  api.installBrowserExtension.mockResolvedValue('C:\\extension')
  api.saveBrowserExtensionZip.mockResolvedValue('D:\\下载\\SnowDown.zip')
  wrapper=mount(BrowserExtension,{attachTo:document.body})
  await wrapper.get('.extension-tile').trigger('click');await flushPromises()
})
afterEach(()=>{wrapper.unmount();document.body.innerHTML=''})
describe('browser extension local installation',()=>{
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
