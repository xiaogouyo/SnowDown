import {mount,flushPromises} from '@vue/test-utils'
import {it,expect,vi} from 'vitest'
import Notice from '../src/components/UpdateNotice.vue'
const api=vi.hoisted(()=>({subscribeUpdates:vi.fn(),getUpdateStatus:vi.fn(),installUpdate:vi.fn(),dismissUpdateNotice:vi.fn()}))
vi.mock('../src/api',()=>api)
it('uses the native ready status, installs only after a click and allows retry on failure',async()=>{
 api.subscribeUpdates.mockResolvedValue(()=>{});api.getUpdateStatus.mockResolvedValue({phase:'ready',revision:4,release:{version:'0.2.0'}});api.installUpdate.mockRejectedValueOnce(Error('重试安装'));
 const wrapper=mount(Notice);await flushPromises();expect(wrapper.text()).toContain('0.2.0');expect(api.installUpdate).not.toHaveBeenCalled();await wrapper.get('.primary').trigger('click');await flushPromises();expect(wrapper.get('[role=alert]').text()).toBe('重试安装');await wrapper.get('[aria-label=稍后更新]').trigger('click');expect(api.dismissUpdateNotice).toHaveBeenCalled();wrapper.unmount();
})
