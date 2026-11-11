import { flushPromises, mount } from '@vue/test-utils'
import { expect, it, vi } from 'vitest'
import DynamicIsland from '../src/components/DynamicIsland.vue'

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  getSnapshot: vi.fn(),
  getClipboardStatus: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({}) }))
vi.mock('../src/api', () => ({
  getSnapshot: native.getSnapshot,
  getClipboardStatus: native.getClipboardStatus,
  subscribe: async () => () => {},
  subscribeClipboard: async () => () => {},
}))

it('keeps the island visible when placement and initial status requests fail', async () => {
  native.invoke.mockRejectedValueOnce(new Error('monitor unavailable'))
  native.getSnapshot.mockRejectedValueOnce(new Error('snapshot unavailable'))
  native.getClipboardStatus.mockResolvedValue({ capture: null })
  let show: (() => void) | undefined
  native.listen.mockImplementation(async (event, handler) => {
    if (event === 'snowdown://island-shown') show = handler
    return () => {}
  })
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
  const animationFrame = vi.spyOn(window, 'requestAnimationFrame')
  const island = mount(DynamicIsland)
  try {
    await flushPromises()
    expect(island.get('.island-bar').text()).toContain('等待链接')
    expect(native.invoke).toHaveBeenCalledWith('resize_island', expect.objectContaining({ height: 42 }))
    native.invoke.mockResolvedValueOnce({ x: 0, y: 0, docked: true })
    expect(show).toBeDefined()
    show?.()
    await flushPromises()
    expect(island.get('.island-shell').classes()).toContain('docked')
    expect(native.invoke).toHaveBeenCalledTimes(2)
    expect(animationFrame).not.toHaveBeenCalled()
  } finally {
    island.unmount()
    warn.mockRestore()
    animationFrame.mockRestore()
  }
})
