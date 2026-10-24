export type TaskStatus = 'queued' | 'downloading' | 'merging' | 'paused' | 'completed' | 'failed' | 'cancelled'
export type OutputFormat = 'video' | 'audio'
export type Platform = 'bilibili' | 'douyin' | 'xiaohongshu'
export interface SourceLink { platform: Platform; url: string }

export interface Settings {
  downloadDir: string
  concurrency: number
  maxSpeedMbps: number
  saveCover: boolean
  saveDanmaku: boolean
  saveSubtitles: boolean
  clipboardEnabled: boolean
  clipboardAutoParse: boolean
}

export interface VideoPage { cid: number; page: number; part: string; duration: number }
export interface Quality { id: number; label: string; width: number; height: number }
export interface VideoInfo {
  platform: Platform
  sourceUrl: string
  bvid: string
  title: string
  cover: string
  author: string
  duration: number
  pages: VideoPage[]
  qualities: Quality[]
  selectedCid: number
}

export interface DownloadTask {
  platform: Platform
  sourceUrl: string
  id: string
  bvid: string
  cid: number
  title: string
  pageTitle: string
  page: number
  cover: string
  quality: number
  qualityLabel: string
  format: OutputFormat
  status: TaskStatus
  progress: number
  downloadedBytes: number
  totalBytes: number
  speed: number
  error: string | null
  outputPath: string | null
  createdAt: number
  downloadDir: string
  saveCover: boolean
  saveDanmaku: boolean
  saveSubtitles: boolean
  warning: string | null
}

export interface AppSnapshot {
  tasks: DownloadTask[]
  settings: Settings
  mediaReady: boolean
  authenticated: boolean
  auth: { state: 'loggedout' | 'checking' | 'authenticated' | 'expired' | 'offline'; account: { uid: number; name: string; avatar: string } | null; message: string | null }
  siteSessions: { platform: Platform; saved: boolean }[]
  version: string
}

export interface LoginQr { id: string; image: string; expiresAt: number; state?: 'confirmed' }
export interface LoginPoll { state: 'waiting' | 'scanned' | 'expired' | 'confirmed'; qr?: LoginQr }

export interface CreateTaskRequest {
  video: VideoInfo
  pageCids: number[]
  quality: number
  format: OutputFormat
}
