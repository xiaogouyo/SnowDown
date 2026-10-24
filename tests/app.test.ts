import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import App from '../src/App.vue'
import type { AppSnapshot, VideoInfo } from '../src/types'

const api = vi.hoisted(() => ({
  desktop: true, getSnapshot: vi.fn(), subscribe: vi.fn(), inspectVideo: vi.fn(),
  createTasks: vi.fn(), taskAction: vi.fn(), saveSettings: vi.fn(), beginLogin: vi.fn(), pollLogin: vi.fn(), cancelLogin: vi.fn(), logout: vi.fn(), checkLogin: vi.fn(), windowAction: vi.fn(), watchMaximized: vi.fn(),
  chooseDirectory: vi.fn(), openDirectory: vi.fn(),
}))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))

const initial: AppSnapshot = {
  tasks: [], settings: { downloadDir: 'D:\\视频', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false },
  mediaReady: true, authenticated: false, auth: { state: 'loggedout', account: null, message: null }, version: '0.1.1',
}
const video: VideoInfo = {
  bvid: 'BV1xx411c7mD', title: '测试视频', cover: '', author: '测试作者', duration: 60,
  selectedCid: 1, pages: [{ cid: 1, page: 1, part: '第一部分', duration: 30 }, { cid: 2, page: 2, part: '第二部分', duration: 30 }],
  qualities: [{ id: 32, label: '480P 清晰', width: 852, height: 480 }],
}

let wrapper: VueWrapper
beforeEach(() => {
  vi.clearAllMocks()
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
  it('starts with a real empty queue and disables empty bulk actions', async () => {
    await flushPromises()
    expect(wrapper.text()).toContain('还没有下载任务')
    expect(wrapper.find('.bulk-actions button').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('下载引擎已就绪')
  })

  it('passes only the selected pages and the available quality to the native engine', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="B站视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledWith('BV1xx411c7mD')
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
    await wrapper.get('input[aria-label="B站视频链接"]').setValue('BV1xx411c7mD')
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
    await wrapper.get('input[aria-label="B站视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toContain('B站暂时限制了请求')
    expect(wrapper.get('input[aria-label="B站视频链接"]').attributes('disabled')).toBeUndefined()
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
