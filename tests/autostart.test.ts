import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
const api = vi.hoisted(() => ({ autostartStatus: vi.fn(), setAutostart: vi.fn() }))
vi.mock('../src/api', () => api)
import AutostartToggle from '../src/components/AutostartToggle.vue'
let wrapper: VueWrapper
afterEach(() => { wrapper?.unmount(); vi.resetAllMocks() })
describe('Windows startup preference', () => {
  it('reads actual Windows status without enabling anything on mount', async () => {
    api.autostartStatus.mockResolvedValue({ enabled: false, blocked: false })
    wrapper = mount(AutostartToggle); await flushPromises()
    expect(wrapper.get('input').element.checked).toBe(false)
    expect(api.setAutostart).not.toHaveBeenCalled()
  })
  it('serializes changes and reflects the confirmed status', async () => {
    api.autostartStatus.mockResolvedValue({ enabled: false, blocked: true })
    let finish!: (value: { enabled: boolean; blocked: boolean }) => void
    api.setAutostart.mockImplementation(() => new Promise(resolve => { finish = resolve }))
    wrapper = mount(AutostartToggle); await flushPromises()
    expect(wrapper.text()).toContain('已被 Windows 禁用')
    await wrapper.get('input').trigger('click'); await wrapper.get('input').trigger('click')
    expect(api.setAutostart).toHaveBeenCalledTimes(1)
    expect(api.setAutostart).toHaveBeenCalledWith(true)
    expect(wrapper.get('input').attributes('disabled')).toBeDefined()
    finish({ enabled: true, blocked: false }); await flushPromises()
    expect(wrapper.get('input').element.checked).toBe(true)
    api.setAutostart.mockResolvedValue({ enabled: false, blocked: false })
    await wrapper.get('input').trigger('click'); await flushPromises()
    expect(api.setAutostart).toHaveBeenLastCalledWith(false)
    expect(wrapper.get('input').element.checked).toBe(false)
  })
  it('does not show a successful toggle when Windows rejects the write', async () => {
    api.autostartStatus.mockResolvedValue({ enabled: true, blocked: false })
    api.setAutostart.mockRejectedValue(new Error('拒绝访问'))
    wrapper = mount(AutostartToggle); await flushPromises()
    await wrapper.get('input').trigger('click'); await flushPromises()
    expect(wrapper.get('input').element.checked).toBe(true)
    expect(wrapper.get('[role="alert"]').text()).toContain('拒绝访问')
  })
  it('disables changes when startup status cannot be read', async () => {
    api.autostartStatus.mockRejectedValue(new Error('读取失败'))
    wrapper = mount(AutostartToggle); await flushPromises()
    expect(wrapper.get('input').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[role="alert"]').text()).toContain('读取失败')
    expect(api.setAutostart).not.toHaveBeenCalled()
  })
})
