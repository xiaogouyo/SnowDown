import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import BatchDownload from '../src/components/BatchDownload.vue'
import { batchItem } from '../src/batch'
import { identifySources } from '../src/sources'
import type { Settings, VideoInfo } from '../src/types'

const api = vi.hoisted(() => ({ inspectVideo: vi.fn(), createBatch: vi.fn(), chooseDirectory: vi.fn() }))
vi.mock('../src/api', () => ({ ...api, __v_isRef: false }))
const settings: Settings = { downloadDir: 'D:\\视频', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false }
const bili = 'https://www.bilibili.com/video/BV1xx411c7mD'
const dy = 'https://www.douyin.com/video/1234567890123456789'
const video: VideoInfo = { platform: 'bilibili', sourceUrl: bili, bvid: 'BV1xx411c7mD', title: '标题', cover: '', author: '', duration: 1, selectedCid: 2, pages: [{ cid: 1, page: 1, part: 'P1', duration: 1 }, { cid: 2, page: 2, part: 'P2', duration: 1 }], qualities: [{ id: 80, label: '1080P', width: 1920, height: 1080 }, { id: 32, label: '480P', width: 852, height: 480 }] }
let wrapper: VueWrapper | undefined
function setup() {
  vi.resetAllMocks()
  api.inspectVideo.mockImplementation(async (url: string) => url === dy ? { ...structuredClone(video), platform: 'douyin', bvid: '1234567890123456789', sourceUrl: dy, pages: [video.pages[0]], selectedCid: 1 } : structuredClone(video))
  api.createBatch.mockResolvedValue({ added: 2, skipped: 0 })
  wrapper = mount(BatchDownload, { props: { settings: structuredClone(settings) } })
  return wrapper
}
afterEach(() => { wrapper?.unmount() })

describe('multiple source extraction and quality policy', () => {
  it('recognizes mixed share text, bare IDs, separators and normalized duplicates in order', () => {
    const result = identifySources(`分享 ${bili}?tracking=1，${dy} BV1xx411c7mD\n${bili}?p=2;https://evil.test/video/1`)
    expect(result.sources.map(source => source.url)).toEqual([bili, dy, `${bili}?p=2`])
    expect(result.duplicates).toBe(1); expect(result.unsupported).toBe(1)
  })
  it('keeps a complete XHS share token while deduplicating note URLs', () => {
    const url = 'https://www.xiaohongshu.com/explore/abcdef120000000012345678'
    const result = identifySources(`${url}\n${url}?xsec_token=fake%3D&xsec_source=pc_search`)
    expect(result.sources).toHaveLength(1)
    expect(result.sources[0]!.url).toContain('xsec_token=fake%3D')
  })
  it('uses available resolution, preserves selected parts and handles portrait and missing dimensions', () => {
    expect(batchItem(video, 'video', 720, false)).toMatchObject({ quality: 32, pageCids: [2] })
    expect(batchItem(video, 'audio', 0, true)).toMatchObject({ quality: 80, pageCids: [1, 2] })
    const portrait = { ...video, platform: 'douyin' as const, qualities: [{ id: 1080, label: '1080P', width: 1080, height: 1920 }, { id: 720, label: '720P', width: 720, height: 1280 }] }
    expect(batchItem(portrait, 'video', 1080, false).quality).toBe(1080)
    expect(batchItem(portrait, 'video', 480, false).quality).toBe(720)
    expect(batchItem({ ...video, qualities: video.qualities.map(q => ({ ...q, width: 0, height: 0 })) }, 'video', 720, false).quality).toBe(32)
  })
})

describe('batch download dialog', () => {
  it('parses mixed links once and enqueues their common settings in one click', async () => {
    const dialog = setup()
    await dialog.get('textarea').setValue(`${bili}\n${dy},${bili}`)
    await dialog.get('[aria-label="任务启动间隔秒数"]').setValue(7)
    await dialog.get('[aria-label="批量保存目录"]').setValue('D:\\批量')
    await dialog.findAll('input[type="checkbox"]')[0]!.setValue(true)
    await dialog.findAll('input[type="checkbox"]')[3]!.setValue(true)
    expect(dialog.text()).toContain('已去重 1 个')
    await dialog.get('.batch-submit').trigger('click'); await flushPromises()
    expect(api.inspectVideo.mock.calls).toEqual([[bili], [dy]])
    const request = api.createBatch.mock.calls[0]![0]
    expect(request.items.map((item: { pageCids: number[] }) => item.pageCids)).toEqual([[1, 2], [1]])
    expect(request.options).toMatchObject({ downloadDir: 'D:\\批量', intervalSecs: 7, saveCover: true })
    expect(dialog.emitted('queued')).toEqual([[{ added: 2, skipped: 0 }]])
    expect(dialog.emitted('close')).toHaveLength(1)
    expect(settings.downloadDir).toBe('D:\\视频')
  })
  it('retains failed links for retry without enqueueing the successful links again', async () => {
    const dialog = setup()
    api.inspectVideo.mockRejectedValueOnce(new Error('需要登录')).mockResolvedValueOnce({ ...video, platform: 'douyin', sourceUrl: dy })
    await dialog.get('textarea').setValue(`${bili}\n${dy}`)
    await dialog.get('.batch-submit').trigger('click'); await flushPromises()
    expect(api.createBatch.mock.calls[0]![0].items).toHaveLength(1)
    expect((dialog.get('textarea').element as HTMLTextAreaElement).value).toBe(bili)
    expect(dialog.find('.batch-result.failed').text()).toContain('需要登录')
    expect(dialog.emitted('close')).toBeUndefined()
    api.inspectVideo.mockClear(); api.inspectVideo.mockResolvedValue(video)
    await dialog.get('.batch-submit').trigger('click'); await flushPromises()
    expect(api.inspectVideo.mock.calls).toEqual([[bili]])
  })
  it('does not queue anything when closed during parsing, even if the native response arrives late', async () => {
    const dialog = setup()
    let finish!: (value: VideoInfo) => void
    api.inspectVideo.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    await dialog.get('textarea').setValue(`${bili}\n${dy}`)
    await dialog.get('.batch-submit').trigger('click')
    await dialog.get('[aria-label="关闭批量下载"]').trigger('click')
    finish(video); await flushPromises()
    expect(api.createBatch).not.toHaveBeenCalled()
    expect(api.inspectVideo).toHaveBeenCalledTimes(1)
  })
  it('keeps all links after a native commit failure and validates interval and count before parsing', async () => {
    const dialog = setup()
    await dialog.get('textarea').setValue(bili)
    await dialog.get('[aria-label="任务启动间隔秒数"]').setValue(1.5)
    await dialog.get('.batch-submit').trigger('click')
    expect(api.inspectVideo).not.toHaveBeenCalled()
    expect(dialog.get('[role="alert"]').text()).toContain('整数')
    await dialog.get('[aria-label="任务启动间隔秒数"]').setValue(0)
    api.createBatch.mockRejectedValueOnce(new Error('磁盘不可用'))
    await dialog.get('.batch-submit').trigger('click'); await flushPromises()
    expect((dialog.get('textarea').element as HTMLTextAreaElement).value).toBe(bili)
    expect(dialog.get('[role="alert"]').text()).toContain('磁盘不可用')
    await dialog.get('textarea').setValue(Array.from({ length: 101 }, (_, index) => `https://www.douyin.com/video/${123456780 + index}`).join('\n'))
    expect(dialog.get('.batch-submit').attributes('disabled')).toBeDefined()
  })
})
