import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import UpdateCenter from '../src/components/UpdateCenter.vue'
import type { UpdateStatus } from '../src/types'

const api = vi.hoisted(() => ({ desktop: true, getUpdateStatus: vi.fn(), checkUpdate: vi.fn(), subscribeUpdates: vi.fn(), deferUpdate: vi.fn(), downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(), installUpdate: vi.fn(), openUpdateRelease: vi.fn() }))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))
const empty: UpdateStatus = { revision: 0, currentVersion: '0.1.6', release: null, phase: 'idle', downloaded: 0, total: 0, error: null, deferred: false, downloadSource: null, latencyMs: null, mirrors: [] }
const available: UpdateStatus = { ...empty, release: { version: '0.1.7', title: 'SnowDown v0.1.7', notes: '改进下载体验\n<script>alert(1)</script>', publishedAt: '2026-10-24T12:00:00Z', url: 'https://github.com/xiaogouyo/SnowDown/releases/tag/v0.1.7', fileName: 'SnowDown_0.1.7_x64-setup.exe', size: 1024 }, total: 1024 }
let wrapper: VueWrapper
const dialog = () => document.body.querySelector('.update-modal')!
function click(selector: string) { (document.body.querySelector(selector) as HTMLButtonElement).click() }
beforeEach(() => {
  vi.resetAllMocks(); vi.useFakeTimers()
  api.getUpdateStatus.mockResolvedValue(structuredClone(empty))
  api.checkUpdate.mockResolvedValue(structuredClone(available))
  api.subscribeUpdates.mockResolvedValue(() => {})
  api.deferUpdate.mockResolvedValue({ ...structuredClone(available), deferred: true })
  api.downloadUpdate.mockResolvedValue({ ...structuredClone(available), phase: 'ready', downloaded: 1024 })
  api.installUpdate.mockResolvedValue(undefined)
  wrapper = mount(UpdateCenter, { props: { currentVersion: '0.1.6', canPrompt: true, activeDownloads: 0 } })
})
afterEach(() => { wrapper?.unmount(); vi.useRealTimers(); document.body.innerHTML = '' })

describe('GitHub update center', () => {
  it('blocks a new download while synchronizing the native state on reopen', async () => {
    await flushPromises()
    api.subscribeUpdates.mock.calls[0]![0]({ ...available, revision: 1 })
    let resolveStatus!: (value: UpdateStatus) => void
    api.getUpdateStatus.mockImplementationOnce(() => new Promise(resolve => { resolveStatus = resolve }))
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(dialog().querySelector('.update-download')?.hasAttribute('disabled')).toBe(true)
    click('.update-download'); await flushPromises()
    expect(api.downloadUpdate).not.toHaveBeenCalled()
    resolveStatus({ ...available, revision: 2 }); await flushPromises()
    expect(dialog().querySelector('.update-download')?.hasAttribute('disabled')).toBe(false)
  })
  it('keeps a native transfer running after hiding the panel and restores current progress', async () => {
    await wrapper.get('button').trigger('click'); await flushPromises()
    const begun: UpdateStatus = { ...available, revision: 2, phase: 'probing' }
    api.downloadUpdate.mockResolvedValueOnce(begun)
    click('.update-download'); await flushPromises()
    click('.update-background'); await flushPromises()
    expect(document.body.querySelector('.update-modal')).toBeNull()
    expect(api.cancelUpdateDownload).not.toHaveBeenCalled()
    expect(api.deferUpdate).not.toHaveBeenCalled()
    const progress: UpdateStatus = { ...available, revision: 3, phase: 'downloading', downloaded: 512 }
    api.getUpdateStatus.mockResolvedValue(progress)
    await vi.advanceTimersByTimeAsync(1500); await flushPromises()
    expect(wrapper.text()).toContain('50%')
    const checks = api.checkUpdate.mock.calls.length
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(api.checkUpdate).toHaveBeenCalledTimes(checks)
    expect(dialog().querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('50')
    click('.update-background'); await flushPromises()
    api.getUpdateStatus.mockResolvedValue({ ...available, revision: 4, phase: 'ready', downloaded: 1024 })
    await vi.advanceTimersByTimeAsync(1500); await flushPromises()
    expect(wrapper.text()).toContain('待安装')
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(dialog().querySelector('.update-install')).not.toBeNull()
    expect(api.cancelUpdateDownload).not.toHaveBeenCalled()
  })
  it('rejects a late start response after a newer completion event', async () => {
    await wrapper.get('button').trigger('click'); await flushPromises()
    let finish!: (value: UpdateStatus) => void
    api.downloadUpdate.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    click('.update-download'); await flushPromises()
    api.subscribeUpdates.mock.calls[0]![0]({ ...available, revision: 4, phase: 'ready', downloaded: 1024 })
    finish({ ...available, revision: 2, phase: 'probing' }); await flushPromises()
    expect(dialog().textContent).toContain('更新已准备好')
    expect(dialog().querySelector('.update-install')).not.toBeNull()
  })
  it('automatically prompts a new version, renders release text safely, and persists postponement', async () => {
    await flushPromises(); await vi.advanceTimersByTimeAsync(2500); await flushPromises()
    expect(dialog().textContent).toContain('发现新版本')
    expect(dialog().querySelector('.update-notes')?.textContent).toContain('<script>alert(1)</script>')
    expect(dialog().querySelector('script')).toBeNull()
    click('.update-actions .secondary'); await flushPromises()
    expect(api.deferUpdate).toHaveBeenCalledTimes(1)
    expect(document.body.querySelector('.update-modal')).toBeNull()
    api.checkUpdate.mockResolvedValue({ ...available, deferred: true })
    await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000); await flushPromises()
    expect(document.body.querySelector('.update-modal')).toBeNull()
    await wrapper.get('[aria-label="版本与检查更新"]').trigger('click'); await flushPromises()
    expect(dialog().textContent).toContain('v0.1.7')
  })
  it('waits until an existing app modal closes before showing the automatic prompt', async () => {
    await wrapper.setProps({ canPrompt: false }); await flushPromises()
    await vi.advanceTimersByTimeAsync(2500); await flushPromises()
    expect(document.body.querySelector('.update-modal')).toBeNull()
    await wrapper.setProps({ canPrompt: true }); await flushPromises()
    expect(dialog()).toBeTruthy()
  })
  it('keeps automatic network errors quiet and reports a manual error without claiming latest', async () => {
    api.checkUpdate.mockRejectedValue(new Error('GitHub 无法连接'))
    await flushPromises(); await vi.advanceTimersByTimeAsync(2500); await flushPromises()
    expect(document.body.querySelector('.update-modal')).toBeNull()
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(dialog().querySelector('[role="alert"]')?.textContent).toContain('GitHub 无法连接')
    expect(dialog().textContent).not.toContain('当前已是最新')
  })
  it('reports up-to-date only after a successful check and allows repeating it', async () => {
    api.checkUpdate.mockResolvedValue(structuredClone(empty))
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(dialog().textContent).toContain('当前已是最新正式版本')
    click('.update-check'); await flushPromises()
    expect(api.checkUpdate).toHaveBeenCalledTimes(2)
  })
  it('downloads with progress, continues in the background, and allows install only after tasks stop', async () => {
    await wrapper.get('button').trigger('click'); await flushPromises()
    let finish!: (value: UpdateStatus) => void
    api.downloadUpdate.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    click('.update-download'); await flushPromises()
    const onProgress = api.subscribeUpdates.mock.calls[0]![0]
    onProgress({ ...available, phase: 'downloading', downloaded: 512 }); await flushPromises()
    expect(dialog().querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('50')
    click('.update-actions .secondary'); await flushPromises()
    expect(api.cancelUpdateDownload).not.toHaveBeenCalled()
    finish({ ...available, phase: 'ready', downloaded: 1024 }); await flushPromises()
    api.checkUpdate.mockResolvedValue({ ...available, phase: 'ready', downloaded: 1024 })
    await wrapper.setProps({ activeDownloads: 1 })
    await wrapper.get('button').trigger('click'); await flushPromises()
    expect(dialog().querySelector('.update-install')?.hasAttribute('disabled')).toBe(true)
    expect(api.installUpdate).not.toHaveBeenCalled()
    await wrapper.setProps({ activeDownloads: 0 }); await flushPromises()
    click('.update-install'); await flushPromises()
    expect(api.installUpdate).toHaveBeenCalledTimes(1)
  })
  it('can cancel a transfer and retry after an integrity error', async () => {
    await wrapper.get('button').trigger('click'); await flushPromises()
    let reject!: (cause: Error) => void
    api.downloadUpdate.mockImplementationOnce(() => new Promise((_, fail) => { reject = fail }))
    click('.update-download'); await flushPromises()
    click('.update-actions button:last-child'); await flushPromises()
    expect(api.cancelUpdateDownload).toHaveBeenCalledTimes(1)
    const onProgress = api.subscribeUpdates.mock.calls[0]![0]
    onProgress({ ...available, phase: 'error', error: '更新包校验失败' })
    reject(new Error('更新包校验失败')); await flushPromises()
    expect(dialog().querySelector('[role="alert"]')?.textContent).toContain('校验失败')
    expect(dialog().querySelector('.update-download')?.textContent).toContain('重试下载')
  })
  it('stops subscriptions and timers on unmount and never reopens a dismissed late manual result', async () => {
    const unsubscribe = vi.fn()
    wrapper.unmount(); api.subscribeUpdates.mockResolvedValue(unsubscribe)
    wrapper = mount(UpdateCenter, { props: { currentVersion: '0.1.6', canPrompt: true, activeDownloads: 0 } }); await flushPromises()
    let finish!: (value: UpdateStatus) => void
    api.checkUpdate.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    await wrapper.get('button').trigger('click'); await flushPromises()
    click('[aria-label="关闭版本更新"]'); await flushPromises()
    finish(available); await flushPromises()
    expect(document.body.querySelector('.update-modal')).toBeNull()
    wrapper.unmount()
    expect(unsubscribe).toHaveBeenCalledTimes(1)
    const calls = api.checkUpdate.mock.calls.length
    await vi.advanceTimersByTimeAsync(24 * 60 * 60 * 1000)
    expect(api.checkUpdate).toHaveBeenCalledTimes(calls)
  })
  it('shows mirror detection progress, all library rows, source selection and cancellation', async () => {
    await wrapper.get('button').trigger('click'); await flushPromises()
    let finish!: (value: UpdateStatus) => void
    api.downloadUpdate.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    click('.update-download'); await flushPromises()
    const onProgress = api.subscribeUpdates.mock.calls[0]![0]
    const mirrors = Array.from({ length: 66 }, (_, index) => ({ id: index + 1, host: index === 65 ? 'github.nswrz.cn' : `mirror${index}.test`, latencyMs: index < 33 ? 100 + index : null, state: index < 33 ? 'available' as const : 'testing' as const, message: null }))
    onProgress({ ...available, phase: 'probing', mirrors }); await flushPromises()
    expect(dialog().textContent).toContain('正在选择更新镜像')
    expect(dialog().textContent).toContain('33 / 66 个节点')
    expect(dialog().querySelector('[aria-label="镜像检测进度"]')?.getAttribute('aria-valuenow')).toBe('50')
    expect(dialog().querySelectorAll('.update-mirrors tbody tr')).toHaveLength(66)
    expect(dialog().textContent).toContain('github.nswrz.cn')
    expect(dialog().querySelector('.update-install')).toBeNull()
    click('.update-actions button:last-child'); await flushPromises()
    expect(api.cancelUpdateDownload).toHaveBeenCalledTimes(1)
    onProgress({ ...available, phase: 'downloading', mirrors, downloadSource: 'mirror0.test', latencyMs: 100 }); await flushPromises()
    expect(dialog().querySelector('.update-source')?.textContent).toContain('mirror0.test · 100 ms')
    expect(dialog().querySelectorAll('tr.selected')).toHaveLength(1)
    finish({ ...available, phase: 'error', mirrors, error: '已取消下载更新。' }); await flushPromises()
    expect(dialog().querySelector('.update-download')?.textContent).toContain('重试下载')
  })
})
