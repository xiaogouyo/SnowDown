import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import App from '../src/App.vue'
import type { AppSnapshot, VideoInfo } from '../src/types'

const api = vi.hoisted(() => ({
  desktop: true, getSnapshot: vi.fn(), subscribe: vi.fn(), inspectVideo: vi.fn(),
  createTasks: vi.fn(), createBatch: vi.fn(), taskAction: vi.fn(), saveSettings: vi.fn(), beginLogin: vi.fn(), pollLogin: vi.fn(), cancelLogin: vi.fn(), logout: vi.fn(), checkLogin: vi.fn(), windowAction: vi.fn(), watchMaximized: vi.fn(),
  chooseDirectory: vi.fn(), openDirectory: vi.fn(), readClipboardLink: vi.fn(), watchFocus: vi.fn(), windowFocused: vi.fn(), getClipboardStatus: vi.fn(), subscribeClipboard: vi.fn(), acknowledgeClipboard: vi.fn(),
  getUpdateStatus: vi.fn(), checkUpdate: vi.fn(), subscribeUpdates: vi.fn(), deferUpdate: vi.fn(), downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(), installUpdate: vi.fn(), openUpdateRelease: vi.fn(),
}))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))

const initial: AppSnapshot = {
  tasks: [], settings: { downloadDir: 'D:\\视频', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false },
  siteSessions: [], mediaReady: true, authenticated: false, auth: { state: 'loggedout', account: null, message: null }, version: '0.1.6',
}
const video: VideoInfo = {
  platform: 'bilibili', sourceUrl: 'https://www.bilibili.com/video/BV1xx411c7mD',
  bvid: 'BV1xx411c7mD', title: '测试视频', cover: '', author: '测试作者', duration: 60,
  selectedCid: 1, pages: [{ cid: 1, page: 1, part: '第一部分', duration: 30 }, { cid: 2, page: 2, part: '第二部分', duration: 30 }],
  qualities: [{ id: 32, label: '480P 清晰', width: 852, height: 480 }],
}

let wrapper: VueWrapper
beforeEach(() => {
  localStorage.clear()
  vi.clearAllMocks()
  api.readClipboardLink.mockResolvedValue(null)
  api.windowFocused.mockResolvedValue(true)
  api.getClipboardStatus.mockResolvedValue({ revision: 0, capture: null })
  api.subscribeClipboard.mockResolvedValue(() => {})
  api.acknowledgeClipboard.mockResolvedValue(undefined)
  api.watchFocus.mockResolvedValue(() => {})
  api.cancelLogin.mockResolvedValue(undefined)
  api.logout.mockResolvedValue(undefined)
  api.checkLogin.mockResolvedValue({state:'authenticated',message:'登录状态有效。'})
  api.beginLogin.mockResolvedValue({ id: 'official-qr', image: 'data:image/png;base64,iVBORw0KGgo=', expiresAt: Date.now() + 120000 })
  api.pollLogin.mockResolvedValue({ state: 'waiting' })
  api.watchMaximized.mockResolvedValue(() => {})
  api.getSnapshot.mockResolvedValue(structuredClone(initial))
  api.subscribe.mockResolvedValue(() => {})
  api.inspectVideo.mockResolvedValue(video)
  api.createTasks.mockResolvedValue(undefined)
  api.createBatch.mockResolvedValue({ added: 2, skipped: 0 })
  api.getUpdateStatus.mockResolvedValue({ currentVersion: '0.1.6', release: null, phase: 'idle', downloaded: 0, total: 0, error: null, deferred: false })
  api.checkUpdate.mockResolvedValue({ currentVersion: '0.1.6', release: null, phase: 'idle', downloaded: 0, total: 0, error: null, deferred: false })
  api.subscribeUpdates.mockResolvedValue(() => {})
  wrapper = mount(App)
})
afterEach(() => { wrapper?.unmount(); localStorage.clear(); vi.useRealTimers() })

describe('desktop download flow', () => {
  async function minimal() { await flushPromises(); await wrapper.get('[aria-label="切换到简约模式"]').trigger('click'); await flushPromises() }
  it('switches to a centered minimal input with only brand and window controls and remembers the mode', async () => {
    await minimal()
    expect(wrapper.find('.minimal-mode').exists()).toBe(true)
    expect(wrapper.get('.standard-mode').attributes('style')).toContain('display: none')
    expect(wrapper.find('.minimal-mode nav').exists()).toBe(false)
    expect(wrapper.find('.minimal-mode .statusbar').exists()).toBe(false)
    expect(wrapper.find('.minimal-stage.expanded').exists()).toBe(false)
    expect(wrapper.findAll('.minimal-window-controls button')).toHaveLength(3)
    expect(localStorage.getItem('snowdown.interface')).toBe('minimal')
    await wrapper.get('[aria-label="切换回完整模式"]').trigger('click'); await flushPromises()
    expect(wrapper.find('.minimal-mode').exists()).toBe(false)
    expect(wrapper.get('.standard-mode').attributes('style') ?? '').not.toContain('display: none')
  })
  it('moves the input up and renders video settings inline after Enter without a modal', async () => {
    await minimal()
    let complete!: (value: VideoInfo) => void
    api.inspectVideo.mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    await wrapper.get('[aria-label="简约视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.minimal-input-bar').trigger('submit'); await flushPromises()
    expect(wrapper.find('.minimal-stage.expanded').exists()).toBe(true)
    expect(wrapper.get('.minimal-loading').text()).toContain('正在解析')
    expect(wrapper.find('.video-modal').exists()).toBe(false)
    complete(video); await flushPromises()
    expect(wrapper.get('[aria-label="视频下载设置"]').text()).toContain(video.title)
    expect(wrapper.find('.modal-overlay').exists()).toBe(false)
  })
  it('keeps an in-flight parse and download options when switching modes in both directions', async () => {
    await flushPromises()
    let complete!: (value: VideoInfo) => void
    api.inspectVideo.mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    await wrapper.get('[aria-label="视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.link-form').trigger('submit'); await flushPromises()
    await wrapper.get('[aria-label="切换到简约模式"]').trigger('click'); await flushPromises()
    expect(wrapper.find('.minimal-loading').exists()).toBe(true)
    complete(video); await flushPromises()
    await wrapper.get('.minimal-segments button:nth-child(2)').trigger('click')
    await wrapper.get('[aria-label="切换回完整模式"]').trigger('click'); await flushPromises()
    expect(wrapper.get('.video-modal').text()).toContain(video.title)
    expect(wrapper.find('.format-option.chosen').text()).toContain('音频')
    expect(api.inspectVideo).toHaveBeenCalledTimes(1)
  })
  it('auto parses a stable recognized input once and reuses native prepared results', async () => {
    await minimal(); vi.useFakeTimers()
    await wrapper.get('[aria-label="简约视频链接"]').setValue('BV1xx411c7mD')
    await vi.advanceTimersByTimeAsync(550); await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(3000); expect(api.inspectVideo).toHaveBeenCalledTimes(1)
    await wrapper.get('[aria-label="清空并重新输入"]').trigger('click')
    api.subscribeClipboard.mock.calls[0]![0]({ revision: 3, capture: { id: 1, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video, error: null } }); await flushPromises()
    await vi.advanceTimersByTimeAsync(1000)
    expect(api.inspectVideo).toHaveBeenCalledTimes(1)
    expect(wrapper.find('.minimal-result').exists()).toBe(true)
    expect(wrapper.find('.video-modal').exists()).toBe(false)
  })
  it('waits for native clipboard parsing instead of starting a duplicate minimal request', async () => {
    await minimal(); vi.useFakeTimers()
    const source = { platform: 'bilibili', url: video.sourceUrl }
    const deliver = api.subscribeClipboard.mock.calls[0]![0]
    deliver({ revision: 1, capture: { id: 1, source, phase: 'detected', video: null, error: null } }); await flushPromises()
    deliver({ revision: 2, capture: { id: 1, source, phase: 'parsing', video: null, error: null } }); await flushPromises()
    await vi.advanceTimersByTimeAsync(1000)
    expect(api.inspectVideo).not.toHaveBeenCalled()
    deliver({ revision: 3, capture: { id: 1, source, phase: 'ready', video, error: null } }); await flushPromises()
    expect(wrapper.find('.minimal-result').exists()).toBe(true)
  })
  it('submits inline page, format, folder and attachment settings without changing global preferences', async () => {
    await minimal()
    await wrapper.get('[aria-label="简约视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.minimal-input-bar').trigger('submit'); await flushPromises()
    api.chooseDirectory.mockResolvedValueOnce('D:\\简约下载')
    await wrapper.get('.minimal-directory button').trigger('click'); await flushPromises()
    await wrapper.findAll('.minimal-segments button')[1]!.trigger('click')
    await wrapper.get('.minimal-attachments input').setValue(true)
    await wrapper.findAll('.minimal-pages-list input')[1]!.setValue(true)
    await wrapper.get('.minimal-download-row .primary').trigger('click'); await flushPromises()
    expect(api.createBatch).toHaveBeenCalledWith({ items: [{ video, pageCids: [1, 2], quality: 32, format: 'audio' }], options: { downloadDir: 'D:\\简约下载', intervalSecs: 0, saveCover: true, saveDanmaku: false, saveSubtitles: false } })
    expect(api.saveSettings).not.toHaveBeenCalled()
    expect(api.createTasks).not.toHaveBeenCalled()
    expect(wrapper.get('.minimal-notice').text()).toContain('已加入')
    expect(wrapper.find('.modal-overlay').exists()).toBe(false)
  })
  it('keeps inline queue progress and native controls working across mode changes', async () => {
    await minimal()
    await wrapper.get('[aria-label="简约视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.minimal-input-bar').trigger('submit'); await flushPromises()
    const task = { id: 'minimal-task', platform: 'bilibili', sourceUrl: video.sourceUrl, bvid: video.bvid, cid: 1, title: '任务标题', pageTitle: '第一部分', page: 1, cover: '', quality: 32, qualityLabel: '480P', format: 'video', status: 'downloading', progress: 25, downloadedBytes: 256, totalBytes: 1024, speed: 64, error: null, outputPath: null, createdAt: Date.now(), downloadDir: initial.settings.downloadDir, saveCover: false, saveDanmaku: false, saveSubtitles: false, warning: null }
    const deliver = api.subscribe.mock.calls[0]![0]
    deliver({ ...initial, tasks: [task] }); await flushPromises()
    expect(wrapper.get('.minimal-task [role="progressbar"]').attributes('aria-valuenow')).toBe('25')
    await wrapper.get('.minimal-task [aria-label="暂停下载"]').trigger('click'); await flushPromises()
    expect(api.taskAction).toHaveBeenCalledWith('minimal-task', 'pause')
    deliver({ ...initial, tasks: [{ ...task, status: 'completed', progress: 100, outputPath: 'D:\\视频\\test.mp4' }] }); await flushPromises()
    expect(wrapper.get('.minimal-task').text()).toContain('已保存')
    await wrapper.get('[aria-label="切换回完整模式"]').trigger('click'); await flushPromises()
    expect(api.taskAction.mock.calls).toHaveLength(1)
    expect(api.cancelUpdateDownload).not.toHaveBeenCalled()
  })
  it('ignores a late parse result after clearing minimal mode and reports failures inline', async () => {
    await minimal()
    let complete!: (value: VideoInfo) => void
    api.inspectVideo.mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    await wrapper.get('[aria-label="简约视频链接"]').setValue('BV1xx411c7mD')
    await wrapper.get('.minimal-input-bar').trigger('submit'); await flushPromises()
    await wrapper.get('[aria-label="清空并重新输入"]').trigger('click')
    complete(video); await flushPromises()
    expect(wrapper.find('.minimal-result').exists()).toBe(false)
    expect(wrapper.find('.minimal-stage.expanded').exists()).toBe(false)
    await wrapper.get('[aria-label="简约视频链接"]').setValue('不支持的链接')
    await wrapper.get('.minimal-input-bar').trigger('submit'); await flushPromises()
    expect(wrapper.get('.minimal-error').text()).toContain('没有识别到')
    expect(wrapper.find('.modal-overlay').exists()).toBe(false)
  })
  it('offers version checking in the top-right version button', async () => {
    await flushPromises()
    await wrapper.get('.topbar [aria-label="版本与检查更新"]').trigger('click'); await flushPromises()
    expect(document.body.querySelector('.update-modal')?.textContent).toContain('当前已是最新正式版本')
    expect(api.checkUpdate).toHaveBeenCalledTimes(1)
    await wrapper.get('.new-task').trigger('click')
    expect(wrapper.find('.batch-modal').exists()).toBe(false)
  })
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
  it('shows an inconclusive XHS check without inferring validity from a saved cookie or logging out',async()=>{
    await flushPromises();wrapper.unmount()
    const saved=structuredClone(initial);saved.siteSessions=[{platform:'xiaohongshu',saved:true,account:{name:'已保存的账号',avatar:''}}]
    api.getSnapshot.mockResolvedValue(saved)
    api.checkLogin.mockResolvedValue({state:'unknown',message:'暂时无法确认登录，原有账号和会话已保留。'})
    wrapper=mount(App);await flushPromises()
    await wrapper.get('.profile').trigger('click')
    await wrapper.findAll('.account-platform-tabs button')[2]!.trigger('click')
    await wrapper.findAll('.account-buttons button')[1]!.trigger('click');await flushPromises()
    expect(wrapper.get('.toast-message').text()).toContain('原有账号和会话已保留')
    expect(wrapper.get('.toast-message').text()).not.toContain('登录状态有效')
    expect(wrapper.text()).not.toContain('登录已失效')
    expect(wrapper.findAll('.account-buttons button').map(b=>b.text())).toEqual(['退出账号','检查登录'])
    expect(wrapper.find('.login-start').exists()).toBe(false)
    expect(api.logout).not.toHaveBeenCalled()
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
    api.getClipboardStatus.mockResolvedValue({ revision: 1, capture: { id: 1, source: { platform: 'douyin', url: 'https://www.douyin.com/video/1234567890123456789' }, phase: 'detected', video: null, error: null } })
    api.watchFocus.mock.calls[0]![0](true)
    await flushPromises()
    expect(wrapper.get('.clipboard-suggestion').text()).toContain('识别到抖音')
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe('BV1xx411c7mD')
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(api.createTasks).not.toHaveBeenCalled()
  })

  it('presents a natively prepared result when automatic parsing is enabled and respects opt-out', async () => {
    await flushPromises(); wrapper.unmount(); api.inspectVideo.mockClear()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.getSnapshot.mockResolvedValue(enabled)
    api.getClipboardStatus.mockResolvedValue({ revision: 2, capture: { id: 1, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video, error: null } })
    wrapper = mount(App); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(true)
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(api.acknowledgeClipboard).toHaveBeenCalledWith(1)
    wrapper.unmount(); vi.clearAllMocks()
    enabled.settings.clipboardEnabled = false
    api.getSnapshot.mockResolvedValue(enabled)
    wrapper = mount(App); await flushPromises()
    expect(api.getClipboardStatus).not.toHaveBeenCalled()
    expect(api.readClipboardLink).not.toHaveBeenCalled()
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })

  it('keeps background results until focus returns and never starts a download automatically', async () => {
    await flushPromises()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.subscribe.mock.calls[0]![0](enabled)
    api.watchFocus.mock.calls[0]![0](false)
    const result = { revision: 3, capture: { id: 2, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video, error: null } }
    api.subscribeClipboard.mock.calls[0]![0](result); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(false)
    api.getClipboardStatus.mockResolvedValue(result)
    api.watchFocus.mock.calls[0]![0](true); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(true)
    expect(api.createTasks).not.toHaveBeenCalled()
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })

  it('retains a result while settings are open and discards stale clipboard revisions', async () => {
    await flushPromises()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.subscribe.mock.calls[0]![0](enabled)
    await wrapper.get('.profile').trigger('click')
    const result = { revision: 8, capture: { id: 3, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video: { ...video, title: '最新结果' }, error: null } }
    api.subscribeClipboard.mock.calls[0]![0](result)
    api.subscribeClipboard.mock.calls[0]![0]({ ...result, revision: 7, capture: { ...result.capture, id: 2, video: { ...video, title: '旧结果' } } }); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(false)
    await wrapper.get('[aria-label="关闭设置"]').trigger('click'); await flushPromises()
    expect(wrapper.get('.video-modal').text()).toContain('最新结果')
    expect(wrapper.get('.video-modal').text()).not.toContain('旧结果')
    expect(api.acknowledgeClipboard).toHaveBeenCalledWith(3)
  })

  it('preserves an edited link and lets the user view a cached result without reparsing', async () => {
    await flushPromises()
    await wrapper.get('input[aria-label="视频链接"]').setValue('BV17x411w7KC')
    api.subscribeClipboard.mock.calls[0]![0]({ revision: 3, capture: { id: 2, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video, error: null } }); await flushPromises()
    expect((wrapper.get('input[aria-label="视频链接"]').element as HTMLInputElement).value).toBe('BV17x411w7KC')
    expect(wrapper.find('.video-modal').exists()).toBe(false)
    await wrapper.get('.clipboard-suggestion .text-button').trigger('click'); await flushPromises()
    expect(wrapper.find('.video-modal').exists()).toBe(true)
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })

  it('acknowledges ignored suggestions and recovers a failed background parse manually', async () => {
    await flushPromises()
    const status = { revision: 3, capture: { id: 2, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'error', video: null, error: '临时网络失败' } }
    api.subscribeClipboard.mock.calls[0]![0](status); await flushPromises()
    expect(wrapper.get('.clipboard-suggestion').text()).toContain('重试解析')
    await wrapper.get('.clipboard-suggestion .text-button').trigger('click'); await flushPromises()
    expect(api.inspectVideo).toHaveBeenCalledWith(video.sourceUrl)
    expect(wrapper.find('.video-modal').exists()).toBe(true)
    expect(api.acknowledgeClipboard).toHaveBeenCalledWith(2)
  })

  it('continues with the next copied link after closing a previous automatic result', async () => {
    await flushPromises()
    const enabled = structuredClone(initial); enabled.settings.clipboardAutoParse = true
    api.subscribe.mock.calls[0]![0](enabled)
    const deliver = api.subscribeClipboard.mock.calls[0]![0]
    deliver({ revision: 2, capture: { id: 1, source: { platform: 'bilibili', url: video.sourceUrl }, phase: 'ready', video, error: null } }); await flushPromises()
    await wrapper.get('[aria-label="关闭视频选择"]').trigger('click')
    deliver({ revision: 4, capture: { id: 2, source: { platform: 'bilibili', url: 'https://www.bilibili.com/video/BV17x411w7KC' }, phase: 'ready', video: { ...video, title: '下一个复制的链接' }, error: null } }); await flushPromises()
    expect(wrapper.get('.video-modal').text()).toContain('下一个复制的链接')
    expect(api.acknowledgeClipboard.mock.calls.map(call => call[0])).toEqual([1, 2])
    expect(api.inspectVideo).not.toHaveBeenCalled()
  })

  it('releases a subscription that arrives after the component has unmounted', async () => {
    await flushPromises(); wrapper.unmount()
    let complete!: (stop: () => void) => void
    api.subscribe.mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
    const stop = vi.fn()
    wrapper = mount(App); wrapper.unmount(); complete(stop); await flushPromises()
    expect(stop).toHaveBeenCalledTimes(1)
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
