import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import App from '../src/App.vue'
import type { AppSnapshot, VideoInfo } from '../src/types'

const api = vi.hoisted(() => ({
  desktop: true, getSnapshot: vi.fn(), subscribe: vi.fn(), inspectVideo: vi.fn(),
  createTasks: vi.fn(), createBatch: vi.fn(), taskAction: vi.fn(), saveSettings: vi.fn(), beginLogin: vi.fn(), pollLogin: vi.fn(), cancelLogin: vi.fn(), logout: vi.fn(), checkLogin: vi.fn(), windowAction: vi.fn(), watchMaximized: vi.fn(),
  chooseDirectory: vi.fn(), openDirectory: vi.fn(), readClipboardLink: vi.fn(), watchFocus: vi.fn(),
}))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))

const initial: AppSnapshot = {
  tasks: [], settings: { downloadDir: 'D:\\视频', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false },
  siteSessions: [], mediaReady: true, authenticated: false, auth: { state: 'loggedout', account: null, message: null }, version: '0.1.5',
}
const video: VideoInfo = {
  platform: 'bilibili', sourceUrl: 'https://www.bilibili.com/video/BV1xx411c7mD',
  bvid: 'BV1xx411c7mD', title: '测试视频', cover: '', author: '测试作者', duration: 60,
  selectedCid: 1, pages: [{ cid: 1, page: 1, part: '第一部分', duration: 30 }, { cid: 2, page: 2, part: '第二部分', duration: 30 }],
  qualities: [{ id: 32, label: '480P 清晰', width: 852, height: 480 }],
}

let wrapper: VueWrapper
beforeEach(() => {
  vi.clearAllMocks()
  api.readClipboardLink.mockResolvedValue(null)
  api.watchFocus.mockResolvedValue(() => {})
  api.cancelLogin.mockResolvedValue(undefined)
  api.logout.mockResolvedValue(undefined)
  api.checkLogin.mockResolvedValue(undefined)
  api.beginLogin.mockResolvedValue({ id: 'official-qr', image: 'data:image/png;base64,iVBORw0KGgo=', expiresAt: Date.now() + 120000 })
  api.pollLogin.mockResolvedValue({ state: 'waiting' })
  api.watchMaximized.mockResolvedValue(() => {})
  api.getSnapshot.mockResolvedValue(structuredClone(initial))
  api.subscribe.mockResolvedValue(() => {})
  api.inspectVideo.mockResolvedValue(video)
  api.createTasks.mockResolvedValue(undefined)
  api.createBatch.mockResolvedValue({ added: 2, skipped: 0 })
  wrapper = mount(App)
})
afterEach(() => { wrapper?.unmount(); vi.useRealTimers() })

describe('desktop download flow', () => {
  it('opens batch download with the top button and Ctrl+N while retaining single-link parsing', async () => {
    await flushPromises()
    expect(wrapper.get('.new-task').text()).toContain('批量下载')
    await wrapper.get('.new-task').trigger('click')
    expect(wrapper.find('[aria-label="批量视频链接"]').exists()).toBe(true)
    await wrapper.get('[aria-label="关闭批量下载"]').trigger('click')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true }))
    await flushPromises()
    expect(wrapper.find('.batch-modal').exists()).toBe(true)
    await wrapper.get('[aria-label="关闭批量下载"]').trigger('click')
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit'); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(true)
  })
  it.each(['bilibili', 'douyin', 'xiaohongshu'] as const)('automatically refreshes %s on official expiration and stops after close', async platform => {
    await flushPromises(); vi.useFakeTimers()
    api.beginLogin.mockResolvedValue({id:'fresh-qr',image:'data:image/png;base64,iVBORw0KGgo=',expiresAt:Date.now()+120000})
    api.pollLogin.mockResolvedValueOnce({state:'expired'}).mockResolvedValue({state:'waiting'})
    await wrapper.get('.profile').trigger('click')
    const index = ['bilibili', 'douyin', 'xiaohongshu'].indexOf(platform)
    await wrapper.findAll('.account-platform-tabs button')[index]!.trigger('click')
    await wrapper.get('.login-start').trigger('click'); await flushPromises()
    await vi.advanceTimersByTimeAsync(1500); await flushPromises()
    expect(api.beginLogin.mock.calls).toEqual([[platform],[platform]])
    await wrapper.get('[aria-label="关闭设置"]').trigger('click'); await flushPromises()
    const calls = api.pollLogin.mock.calls.length
    await vi.advanceTimersByTimeAsync(15000)
    expect(api.pollLogin).toHaveBeenCalledTimes(calls)
    expect(api.cancelLogin).toHaveBeenLastCalledWith(platform)
  })
  it.each(['bilibili', 'douyin', 'xiaohongshu'] as const)('refreshes %s at its local deadline even without an expiration response', async platform => {
    await flushPromises(); vi.useFakeTimers()
    api.beginLogin.mockResolvedValueOnce({id:'old',image:'data:image/png;base64,iVBORw0KGgo=',expiresAt:Date.now()+1000}).mockResolvedValue({id:'new',image:'data:image/png;base64,iVBORw0KGgo=',expiresAt:Date.now()+120000})
    await wrapper.get('.profile').trigger('click')
    await wrapper.findAll('.account-platform-tabs button')[['bilibili','douyin','xiaohongshu'].indexOf(platform)]!.trigger('click')
    await wrapper.get('.login-start').trigger('click'); await flushPromises()
    await vi.advanceTimersByTimeAsync(1500); await flushPromises()
    expect(api.beginLogin).toHaveBeenCalledTimes(2)
    expect(api.pollLogin).not.toHaveBeenCalled()
  })
  it('uses a QR automatically replaced by the official page without restarting login', async () => {
    await flushPromises(); vi.useFakeTimers()
    api.pollLogin.mockResolvedValue({state:'waiting',qr:{id:'official-qr',image:'data:image/png;base64,bmV3',expiresAt:Date.now()+180000}})
    await wrapper.get('.profile').trigger('click')
    await wrapper.findAll('.account-platform-tabs button')[1]!.trigger('click')
    await wrapper.get('.login-start').trigger('click'); await flushPromises()
    await vi.advanceTimersByTimeAsync(1500); await flushPromises()
    expect(wrapper.get('img[alt="抖音官方登录二维码"]').attributes('src')).toBe('data:image/png;base64,bmV3')
    expect(api.beginLogin).toHaveBeenCalledTimes(1)
  })
  it('does not publish a late QR or start polling after switching platform', async () => {
    await flushPromises(); vi.useFakeTimers()
    let resolveQr!: (qr: unknown) => void
    api.beginLogin.mockImplementationOnce(() => new Promise(resolve => { resolveQr=resolve }))
    await wrapper.get('.profile').trigger('click')
    await wrapper.get('.login-start').trigger('click'); await flushPromises()
    await wrapper.findAll('.account-platform-tabs button')[1]!.trigger('click')
    resolveQr({id:'late',image:'data:image/png;base64,bGF0ZQ==',expiresAt:Date.now()+120000}); await flushPromises()
    await vi.advanceTimersByTimeAsync(10000)
    expect(wrapper.find('.login-qr img').exists()).toBe(false)
    expect(api.pollLogin).not.toHaveBeenCalled()
  })
  it('keeps download input unchanged and does not parse when QR login confirms', async () => {
    await flushPromises()
    const source = 'https://www.douyin.com/video/1234567890123456789'
    api.beginLogin.mockResolvedValue({id:'confirmed',image:'',expiresAt:Date.now(),state:'confirmed'})
    await wrapper.get('input[aria-label="视频链接"]').setValue(source)
    await wrapper.get('.link-card-footer .text-button').trigger('click'); await flushPromises()
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe(source)
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })
  it.each(['douyin','xiaohongshu'] as const)('shows only logout and login check for saved %s and calls the selected platform', async platform => {
    await flushPromises(); wrapper.unmount()
    const saved = structuredClone(initial); saved.siteSessions=[{platform,saved:true}]
    api.getSnapshot.mockResolvedValue(saved)
    wrapper=mount(App); await flushPromises()
    await wrapper.get('.profile').trigger('click')
    await wrapper.findAll('.account-platform-tabs button')[['bilibili','douyin','xiaohongshu'].indexOf(platform)]!.trigger('click')
    expect(wrapper.findAll('.account-buttons button').map(button=>button.text())).toEqual(['退出账号','检查登录'])
    expect(wrapper.text()).not.toContain('官方验证')
    expect(wrapper.text()).not.toContain('重新扫码登录')
    await wrapper.findAll('.account-buttons button')[1]!.trigger('click'); await flushPromises()
    expect(api.checkLogin).toHaveBeenCalledWith(platform)
    const loggedOut=structuredClone(saved); loggedOut.siteSessions=[]
    api.getSnapshot.mockResolvedValue(loggedOut)
    await wrapper.findAll('.account-buttons button')[0]!.trigger('click'); await flushPromises()
    expect(api.logout).toHaveBeenCalledWith(platform)
    expect(wrapper.find('.login-start').exists()).toBe(true)
  })
  it('pastes and parses a recognized platform in one action', async () => {
    await flushPromises()
    api.readClipboardLink.mockResolvedValue({ platform: 'douyin', url: 'https://www.douyin.com/video/1234567890123456789' })
    await wrapper.get('.paste-button').trigger('click')
    await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledWith('https://www.douyin.com/video/1234567890123456789')
  })

  it('detects the clipboard on focus without replacing an edited link or starting a download', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    api.readClipboardLink.mockResolvedValue({ platform: 'douyin', url: 'https://www.douyin.com/video/1234567890123456789' })
    api.watchFocus.mock.calls[0]![0]()
    await flushPromises()
    expect(wrapper.get('.clipboard-suggestion').text()).toContain('识别到抖音')
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe('BV1xx411c7mD')
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(api.createTasks).not.toHaveBeenCalled()
  })

  it('offers opt-in auto parse and respects disabled clipboard recognition', async () => {
    await flushPromises(); wrapper.unmount()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.getSnapshot.mockResolvedValue(enabled)
    api.readClipboardLink.mockResolvedValue({ platform: 'douyin', url: 'https://www.douyin.com/video/1234567890123456789' })
    wrapper = mount(App); await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledTimes(1)
    wrapper.unmount(); vi.clearAllMocks()
    enabled.settings.clipboardEnabled = false
    api.getSnapshot.mockResolvedValue(enabled)
    wrapper = mount(App); await flushPromises()
    expect(api.readClipboardLink).not.toHaveBeenCalled()
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })

  it('starts with a real empty queue and disables empty bulk actions', async () => {
    await flushPromises()
    expect(wrapper.text()).toContain('还没有下载任务')
    expect(wrapper.find('.bulk-actions button').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('下载引擎已就绪')
  })

  it('passes only the selected pages and the available quality to the native engine', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledWith('https://www.bilibili.com/video/BV1xx411c7mD')
    const checkboxes = wrapper.findAll('.page-option input')
    expect((checkboxes[0]!.element as HTMLInputElement).checked).toBe(true)
    expect((checkboxes[1]!.element as HTMLInputElement).checked).toBe(false)
    await checkboxes[1]!.setValue(true)
    await wrapper.get('.video-modal .modal-footer button').trigger('click')
    await flushPromises()
    expect(api.createTasks).toHaveBeenCalledWith({ video, pageCids: [1, 2], quality: 32, format: 'video' })
    expect(wrapper.find('.video-modal').exists()).toBe(false)
  })

  it('does not dismiss the selection when the engine rejects the task', async () => {
    api.createTasks.mockRejectedValueOnce('下载目录不可用')
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    await wrapper.get('.video-modal .modal-footer button').trigger('click')
    await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(true)
    expect(wrapper.get('[role="status"]').text()).toContain('下载目录不可用')
  })

  it('surfaces network errors and makes the input usable again', async () => {
    api.inspectVideo.mockRejectedValueOnce('B站暂时限制了请求')
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toContain('B站暂时限制了请求')
    expect(wrapper.get('input[aria-label="视频链接"]').attributes('disabled')).toBeUndefined()
  })

  it('routes the integrated titlebar buttons to native window actions', async () => {
    await flushPromises()
    await wrapper.get('[aria-label="最小化窗口"]').trigger('click')
    await wrapper.get('[aria-label="最大化窗口"]').trigger('click')
    await wrapper.get('[aria-label="关闭到托盘"]').trigger('click')
    expect(api.windowAction.mock.calls).toEqual([['minimize'], ['maximize'], ['close']])
  })

  it('shows the official QR states and stops polling after the account panel closes', async () => {
    await flushPromises()
    vi.useFakeTimers()
    api.beginLogin.mockResolvedValue({ id: 'test-qr', image: 'data:image/svg+xml;base64,PHN2Zy8+', expiresAt: Date.now() + 180000 })
    api.pollLogin.mockResolvedValue({ state: 'scanned' })
    await wrapper.get('.profile').trigger('click')
    expect(wrapper.find('input[type="password"]').exists()).toBe(false)
    await wrapper.get('.login-start').trigger('click')
    await flushPromises()
    expect(wrapper.get('img[alt="B站官方登录二维码"]').attributes('src')).toContain('data:image/svg+xml;')
    await vi.advanceTimersByTimeAsync(1500)
    await flushPromises()
    expect(wrapper.get('.qr-status').text()).toContain('请在手机B站确认')
    await wrapper.get('[aria-label="关闭设置"]').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(10000)
    expect(api.pollLogin).toHaveBeenCalledTimes(1)
    expect(api.cancelLogin).toHaveBeenCalled()
  })

  it('automatically obtains a fresh QR when Bilibili reports expiration', async () => {
    await flushPromises()
    vi.useFakeTimers()
    api.beginLogin.mockResolvedValue({ id: 'expired-qr', image: 'data:image/svg+xml;base64,PHN2Zy8+', expiresAt: Date.now() + 180000 })
    api.pollLogin.mockResolvedValue({ state: 'expired' })
    await wrapper.get('.profile').trigger('click')
    await wrapper.get('.login-start').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(1500)
    await flushPromises()
    expect(api.beginLogin).toHaveBeenCalledTimes(2)
    expect(wrapper.get('.qr-status').text()).toContain('过期自动更新')
    expect(wrapper.find('.qr-expired').exists()).toBe(false)
  })

  it('shows a saved account only after native confirmation succeeds', async () => {
    await flushPromises()
    vi.useFakeTimers()
    api.beginLogin.mockResolvedValue({ id: 'confirmed-qr', image: 'data:image/svg+xml;base64,PHN2Zy8+', expiresAt: Date.now() + 180000 })
    api.pollLogin.mockResolvedValue({ state: 'confirmed' })
    api.getSnapshot.mockResolvedValue({ ...initial, authenticated: true, auth: { state: 'authenticated', account: { uid: 123, name: '测试账号', avatar: '' }, message: null } })
    await wrapper.get('.profile').trigger('click')
    await wrapper.get('.login-start').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(1500)
    await flushPromises()
    expect(wrapper.get('.login-saved').text()).toContain('已加密保存')
    expect(wrapper.get('.profile-text strong').text()).toBe('测试账号')
    expect(wrapper.find('.login-qr').exists()).toBe(false)
  })
})
