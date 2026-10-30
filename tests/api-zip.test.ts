import {beforeEach,describe,expect,it,vi} from 'vitest'
const native=vi.hoisted(()=>({invoke:vi.fn(),save:vi.fn()}))
vi.mock('@tauri-apps/api/core',()=>({invoke:native.invoke,isTauri:()=>true}))
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn(),save:native.save}))
import {saveBrowserExtensionZip} from '../src/api'
beforeEach(()=>vi.clearAllMocks())
describe('ZIP save dialog adapter',()=>{
  it('does not export when the user cancels Save',async()=>{
    native.save.mockResolvedValue(null)
    expect(await saveBrowserExtensionZip()).toBeNull()
    expect(native.invoke).not.toHaveBeenCalled()
  })
  it('offers a ZIP filter and passes the literal chosen path to the native exporter',async()=>{
    const path="D:\\本地 插件 ' $目录\\插件.zip"
    native.save.mockResolvedValue(path);native.invoke.mockResolvedValue(path)
    expect(await saveBrowserExtensionZip()).toBe(path)
    expect(native.save).toHaveBeenCalledWith(expect.objectContaining({defaultPath:'SnowDown_BrowserExtension_0.1.17.zip',filters:[{name:'ZIP 压缩包',extensions:['zip']}]}))
    expect(native.invoke).toHaveBeenCalledWith('export_browser_extension_zip',{path})
  })
  it('propagates a native write failure for the panel to display',async()=>{
    native.save.mockResolvedValue('D:\\插件.zip');native.invoke.mockRejectedValue(new Error('写入失败'))
    await expect(saveBrowserExtensionZip()).rejects.toThrow('写入失败')
  })
})
