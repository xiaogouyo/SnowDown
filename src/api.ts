import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type { AppSnapshot, CreateTaskRequest, Settings, VideoInfo } from './types'

export const desktop = isTauri()

export async function getSnapshot(): Promise<AppSnapshot> {
  if (!desktop) return {
    tasks: [],
    settings: { downloadDir: '', concurrency: 2, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false },
    mediaReady: false, authenticated: false, version: '0.1.0',
  }
  return invoke('get_snapshot')
}

function requireDesktop() {
  if (!desktop) throw new Error('当前是界面预览。请启动 SnowDown Windows 客户端后解析和下载视频。')
}

export async function inspectVideo(input: string): Promise<VideoInfo> {
  requireDesktop()
  return invoke('inspect_video', { input })
}

export async function createTasks(request: CreateTaskRequest): Promise<void> {
  requireDesktop()
  await invoke('create_tasks', { request })
}

export async function taskAction(id: string, action: 'pause' | 'resume' | 'cancel' | 'remove' | 'reveal'): Promise<void> {
  requireDesktop()
  await invoke('task_action', { id, action })
}

export async function saveSettings(settings: Settings): Promise<void> {
  requireDesktop()
  await invoke('save_settings', { settings })
}

export async function setSession(cookie: string): Promise<void> {
  requireDesktop()
  await invoke('set_session', { cookie })
}

export async function chooseDirectory(): Promise<string | null> {
  requireDesktop()
  return await open({ directory: true, multiple: false, title: '选择下载目录' }) as string | null
}

export async function openDirectory(): Promise<void> {
  requireDesktop()
  await invoke('open_download_directory')
}

export async function subscribe(callback: (snapshot: AppSnapshot) => void): Promise<() => void> {
  if (!desktop) return () => {}
  return listen<AppSnapshot>('snowdown://state', (event) => callback(event.payload))
}
