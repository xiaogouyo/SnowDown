import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import TrayMenu from '../src/components/TrayMenu.vue'

const api = vi.hoisted(() => ({ getSnapshot: vi.fn(), subscribe: vi.fn(), watchFocus: vi.fn(), trayAction: vi.fn() }))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))
let wrapper: VueWrapper
beforeEach(() => {
  vi.resetAllMocks()
  api.getSnapshot.mockResolvedValue({ version: '0.1.8', settings: { clipboardEnabled: true, clipboardAutoParse: true } })
  api.subscribe.mockResolvedValue(() => {})
  api.watchFocus.mockResolvedValue(() => {})
  api.trayAction.mockResolvedValue(undefined)
  wrapper = mount(TrayMenu, { attachTo: document.body })
})
afterEach(() => { wrapper.unmount(); document.body.innerHTML = '' })

describe('SnowDown tray popup', () => {
  it('shows the shared brand, version and native background setting, and routes all three actions', async () => {
    await flushPromises()
    expect(wrapper.text()).toContain('v0.1.8')
    expect(wrapper.text()).toContain('自动解析已开启')
    const items = wrapper.findAll('[role="menuitem"]')
    expect(items.map(item => item.text())).toEqual(['打开 SnowDown', '打开下载目录', '退出 SnowDown'])
    for (const item of items) { await item.trigger('click'); await flushPromises() }
    expect(api.trayAction.mock.calls.map(call => call[0])).toEqual(['show', 'folder', 'quit'])
  })
  it('supports arrow navigation and Escape, and releases subscriptions', async () => {
    const stop = vi.fn(); api.subscribe.mockResolvedValue(stop)
    wrapper.unmount(); wrapper = mount(TrayMenu, { attachTo: document.body }); await flushPromises()
    await wrapper.get('.tray-shell').trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement?.textContent).toBe('打开下载目录')
    await wrapper.get('.tray-shell').trigger('keydown', { key: 'End' })
    expect(document.activeElement?.textContent).toBe('退出 SnowDown')
    await wrapper.get('.tray-shell').trigger('keydown', { key: 'Escape' }); await flushPromises()
    expect(api.trayAction).toHaveBeenCalledWith('dismiss')
    wrapper.unmount(); expect(stop).toHaveBeenCalledTimes(1)
  })
  it('reports an action failure and allows retrying', async () => {
    await flushPromises(); api.trayAction.mockRejectedValueOnce(new Error('目录不可用'))
    await wrapper.findAll('[role="menuitem"]')[1]!.trigger('click'); await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toContain('目录不可用')
    await wrapper.findAll('[role="menuitem"]')[1]!.trigger('click'); await flushPromises()
    expect(api.trayAction).toHaveBeenCalledTimes(2)
  })
})
