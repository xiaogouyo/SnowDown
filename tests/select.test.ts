import { afterEach, describe, expect, it } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import UiSelect from '../src/components/UiSelect.vue'

let wrapper: VueWrapper
afterEach(() => { wrapper?.unmount(); document.body.innerHTML = '' })
describe('shared dropdown interactions', () => {
  it('supports keyboard navigation and selection with the same styled popup', async () => {
    wrapper = mount(UiSelect, { props: { modelValue: 'all', label: '状态', options: [{ value: 'all', label: '全部状态' }, { value: 'paused', label: '已暂停' }] }, attachTo: document.body })
    await wrapper.get('button').trigger('keydown', { key: 'ArrowDown' })
    expect(document.querySelector('[role="listbox"]')).not.toBeNull()
    await document.querySelector('[role="listbox"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await document.querySelector('[role="listbox"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['paused'])
    expect(wrapper.get('button').attributes('aria-expanded')).toBe('false')
  })
  it('keeps popup scrolling usable and closes on outside pointer input', async () => {
    wrapper = mount(UiSelect, { props: { modelValue: 1, label: '并发', options: [{ value: 1, label: '1 个任务' }, { value: 2, label: '2 个任务' }] }, attachTo: document.body })
    await wrapper.get('button').trigger('click')
    document.querySelector('[role="listbox"]')!.dispatchEvent(new Event('scroll'))
    expect(wrapper.get('button').attributes('aria-expanded')).toBe('true')
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await wrapper.vm.$nextTick()
    expect(document.querySelector('[role="listbox"]')).toBeNull()
  })
})
