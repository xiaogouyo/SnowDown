import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import App from '../src/App.vue'
import type { AppSnapshot, VideoInfo } from '../src/types'

const api = vi.hoisted(() => ({
  desktop: true, getSnapshot: vi.fn(), subscribe: vi.fn(), inspectVideo: vi.fn(),
  createTasks: vi.fn(), taskAction: vi.fn(), saveSettings: vi.fn(), beginLogin: vi.fn(), pollLogin: vi.fn(), cancelLogin: vi.fn(), logout: vi.fn(), checkLogin: vi.fn(), windowAction: vi.fn(), watchMaximized: vi.fn(),
  chooseDirectory: vi.fn(), openDirectory: vi.fn(), readClipboardLink: vi.fn(), watchFocus: vi.fn(), watchSiteReturn: vi.fn(), openSiteSession: vi.fn(), captureSiteSession: vi.fn(), clearSiteSession: vi.fn(),
}))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))

const initial: AppSnapshot = {
  tasks: [], settings: { downloadDir: 'D:\\视频', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false },
  siteSessions: [], mediaReady: true, authenticated: false, auth: { state: 'loggedout', account: null, message: null }, version: '0.1.3',
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
  api.watchSiteReturn.mockResolvedValue(() => {})
  api.cancelLogin.mockResolvedValue(undefined)
  api.watchMaximized.mockResolvedValue(() => {})
  api.getSnapshot.mockResolvedValue(structuredClone(initial))
  api.subscribe.mockResolvedValue(() => {})
  api.inspectVideo.mockResolvedValue(video)
  api.createTasks.mockResolvedValue(undefined)
  wrapper = mount(App)
})
afterEach(() => { wrapper?.unmount(); vi.useRealTimers() })

describe('desktop download flow', () => {
  it('keeps the entered link and does not parse when login returns', async () => {
    await flushPromises()
    const source = 'https://www.douyin.com/video/1234567890123456789'
    await wrapper.get('input[aria-label="视频链接"]').setValue(source)
    api.watchSiteReturn.mock.calls[0]![0]('douyin', true)
    await flushPromises()
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe(source)
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(wrapper.find('.video-modal').exists()).toBe(false)
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledWith(source)
  })
  it('keeps an empty input after login and reports save failures', async () => {
    await flushPromises()
    api.watchSiteReturn.mock.calls[0]![0]('douyin', false, '读取登录状态超时')
    await flushPromises()
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe('')
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(wrapper.get('[role="status"]').text()).toContain('读取登录状态超时')
  })
  it('opens login without sending the download link and preserves later edits', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('https://www.douyin.com/video/1234567890123456789')
    await wrapper.get('.link-card-footer .text-button').trigger('click')
    expect(api.openSiteSession).toHaveBeenCalledWith('douyin')
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    api.watchSiteReturn.mock.calls[0]![0]('douyin', true)
    await flushPromises()
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe('BV1xx411c7mD')
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })
  it('saves login from settings without retrying or closing the settings', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('https://www.douyin.com/video/1234567890123456789')
    await wrapper.get('.profile').trigger('click')
    await wrapper.get('.site-login-actions .primary').trigger('click')
    await flushPromises()
    expect(api.captureSiteSession).toHaveBeenCalledWith('douyin')
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(wrapper.find('.settings-modal').exists()).toBe(true)
  })
  it('does not auto parse clipboard content when focusing back from login', async () => {
    await flushPromises(); wrapper.unmount()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.getSnapshot.mockResolvedValue(enabled)
    wrapper = mount(App); await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('https://www.douyin.com/video/1234567890123456789')
    await wrapper.get('.link-card-footer .text-button').trigger('click')
    api.readClipboardLink.mockClear()
    api.watchFocus.mock.calls.at(-1)![0]()
    await flushPromises()
    expect(api.readClipboardLink).not.toHaveBeenCalled()
    expect(api.inspectVideo).not.toHaveBeenCalled()
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

  it('opens official login for the selected platform without a video URL', async () => {
    await flushPromises()
    const source = 'https://www.douyin.com/video/1234567890123456789'
    await wrapper.get('input[aria-label="视频链接"]').setValue(source)
    await wrapper.get('.profile').trigger('click')
    expect(wrapper.get('.account-platform-tabs .active').text()).toBe('抖音')
    await wrapper.get('.site-login-actions .secondary').trigger('click')
    expect(api.openSiteSession).toHaveBeenCalledWith('douyin')
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
    expect(wrapper.get('.qr-status').text()).toContain('请在手机 B站确认')
    await wrapper.get('[aria-label="关闭设置"]').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(10000)
    expect(api.pollLogin).toHaveBeenCalledTimes(1)
    expect(api.cancelLogin).toHaveBeenCalled()
  })

  it('replaces an expired QR with a refresh action', async () => {
    await flushPromises()
    vi.useFakeTimers()
    api.beginLogin.mockResolvedValue({ id: 'expired-qr', image: 'data:image/svg+xml;base64,PHN2Zy8+', expiresAt: Date.now() + 180000 })
    api.pollLogin.mockResolvedValue({ state: 'expired' })
    await wrapper.get('.profile').trigger('click')
    await wrapper.get('.login-start').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(1500)
    await flushPromises()
    expect(wrapper.get('.qr-expired button').text()).toContain('刷新二维码')
    expect(wrapper.get('.qr-status').text()).toContain('已过期')
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
