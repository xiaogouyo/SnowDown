import type { CreateTaskRequest, OutputFormat, Quality, VideoInfo } from './types'

export const batchLimit = 100
export const qualityOptions = [
  { value: 0, label: '最高可用画质' },
  { value: 1080, label: '最高 1080P' },
  { value: 720, label: '最高 720P' },
  { value: 480, label: '最高 480P' },
]

function resolution(quality: Quality, video: VideoInfo): number {
  if (quality.width && quality.height) return Math.min(quality.width, quality.height)
  if (video.platform === 'bilibili') return ({ 6: 240, 16: 360, 32: 480, 64: 720, 74: 720, 80: 1080, 112: 1080, 116: 1080, 120: 2160, 125: 2160, 126: 2160, 127: 4320 } as Record<number, number>)[quality.id] ?? 0
  return quality.id <= 4320 ? quality.id : 0
}

export function batchItem(video: VideoInfo, format: OutputFormat, maxResolution: number, allPages: boolean): CreateTaskRequest {
  const available = [...video.qualities].sort((a, b) => resolution(b, video) - resolution(a, video) || b.id - a.id)
  const quality = (maxResolution ? available.find(item => resolution(item, video) > 0 && resolution(item, video) <= maxResolution) ?? available.at(-1) : available[0])
  if (!quality || !video.pages.length) throw new Error('没有可下载的画质或分P。')
  return {
    video, format, quality: quality.id,
    pageCids: allPages ? video.pages.map(page => page.cid) : [video.selectedCid],
  }
}
