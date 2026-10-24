import { describe, expect, it } from 'vitest'
import { identifySource, sourceSummary } from '../src/sources'

describe('shared platform detection', () => {
  it('extracts a Douyin modal link from a complete share message', () => {
    expect(identifySource('分享给你 https://www.douyin.com/jingxuan?modal_id=7689371847216893193 复制打开抖音'))
      .toEqual({ platform: 'douyin', url: 'https://www.douyin.com/video/7689371847216893193' })
    expect(identifySource('https://v.douyin.com/example/')).toEqual({ platform: 'douyin', url: 'https://v.douyin.com/example/' })
  })
  it('preserves XHS access parameters and hides them in the visible summary', () => {
    const source = identifySource('https://www.xiaohongshu.com/discovery/item/abcdef120000000012345678?xsec_token=fake%3D&xsec_source=pc_search&source=feed')!
    expect(source.platform).toBe('xiaohongshu')
    expect(source.url).toContain('xsec_token=fake%3D&xsec_source=pc_search')
    expect(source.url).not.toContain('source=feed')
    expect(sourceSummary(source)).toBe('www.xiaohongshu.com/explore/abcdef120000000012345678')
  })
  it('cleans Bili tracking while retaining the selected page', () => {
    expect(identifySource('https://www.bilibili.com/video/BV1xx411c7mD/?p=2&spm_id_from=tracking')?.url).toBe('https://www.bilibili.com/video/BV1xx411c7mD?p=2')
    expect(identifySource('BV1xx411c7mD')?.platform).toBe('bilibili')
  })
  it('rejects unrelated domains, credentials and unsupported URLs', () => {
    for (const text of ['https://douyin.com.evil.test/video/123456789', 'https://user@www.douyin.com/video/123456789', 'https://www.xiaohongshu.com:8443/explore/abcdef120000000012345678', 'https://www.douyin.com/user/example', 'plain text']) expect(identifySource(text)).toBeNull()
  })
})
