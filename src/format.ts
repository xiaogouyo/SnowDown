import type { TaskStatus } from './types'

export function bytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), 3)
  return `${(value / 1024 ** unit).toFixed(unit === 0 ? 0 : 1)} ${['B', 'KB', 'MB', 'GB'][unit]}`
}

export function duration(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  return `${minutes}:${Math.floor(seconds % 60).toString().padStart(2, '0')}`
}

export const statusLabels: Record<TaskStatus, string> = {
  queued: '等待下载', downloading: '正在下载', merging: '合并音视频', paused: '已暂停',
  completed: '已完成', failed: '下载失败', cancelled: '已取消',
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
