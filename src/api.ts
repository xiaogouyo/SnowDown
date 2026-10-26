import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { AppSnapshot, CreateTaskRequest, CreateBatchRequest, BatchResult, Settings, VideoInfo, LoginQr, LoginPoll, Platform, SourceLink, UpdateStatus, ClipboardSnapshot } from './types'

export const desktop = isTauri()

export async function getSnapshot(): Promise<AppSnapshot> {
  if (!desktop) return {
    tasks: [],
    settings: { downloadDir: '', concurrency: 2, downloadConnections: 8, maxSpeedMbps: 0, saveCover: false, saveDanmaku: false, saveSubtitles: false, clipboardEnabled: true, clipboardAutoParse: false },
    siteSessions: [],
    mediaReady: false, authenticated: false, auth: { state: 'loggedout', account: null, message: null }, version: '0.2.4',
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

export async function createBatch(request: CreateBatchRequest): Promise<BatchResult> {
  requireDesktop()
  return invoke('create_batch_tasks', { request })
}

export async function taskAction(id: string, action: 'pause' | 'resume' | 'cancel' | 'remove' | 'reveal'): Promise<void> {
  requireDesktop()
  await invoke('task_action', { id, action })
}

export async function saveSettings(settings: Settings): Promise<void> {
  requireDesktop()
  await invoke('save_settings', { settings })
}

export async function beginLogin(platform: Platform = 'bilibili'): Promise<LoginQr> { requireDesktop(); return invoke('begin_login', { platform }) }
export async function pollLogin(id: string, platform: Platform = 'bilibili'): Promise<LoginPoll> { requireDesktop(); return invoke('poll_login', { id, platform }) }
export async function cancelLogin(platform: Platform = 'bilibili'): Promise<void> { if (desktop) await invoke('cancel_login', { platform }) }
export async function logout(platform: Platform = 'bilibili'): Promise<void> { requireDesktop(); await invoke('logout', { platform }) }
export async function checkLogin(platform: Platform = 'bilibili'): Promise<{state:'authenticated'|'loggedout'|'unknown';message:string}> { requireDesktop(); return invoke('check_login', { platform }) }
export async function readClipboardLink(): Promise<SourceLink | null> { requireDesktop(); return invoke('read_clipboard_link') }
export async function getClipboardStatus(): Promise<ClipboardSnapshot> { requireDesktop(); return invoke('clipboard_status') }
export async function acknowledgeClipboard(id: number): Promise<void> { requireDesktop(); await invoke('acknowledge_clipboard', { id }) }
export async function subscribeClipboard(callback: (status: ClipboardSnapshot) => void): Promise<() => void> {
  if (!desktop) return () => {}
  return listen<ClipboardSnapshot>('snowdown://clipboard', event => callback(event.payload))
}
export async function windowFocused(): Promise<boolean> { return desktop && await getCurrentWindow().isFocused() }
export async function trayAction(action: 'show' | 'folder' | 'quit' | 'dismiss'): Promise<void> { requireDesktop(); await invoke('tray_action', { action }) }
export async function watchFocus(callback: (focused: boolean) => void): Promise<() => void> {
  if (!desktop) return () => {}
  return getCurrentWindow().onFocusChanged(({ payload }) => callback(payload))
}

export async function windowAction(action: 'minimize' | 'maximize' | 'close'): Promise<void> {
  requireDesktop()
  const window = getCurrentWindow()
  if (action === 'maximize') await window.toggleMaximize()
  else if (action === 'minimize') await window.minimize()
  else await window.close()
}

export async function watchMaximized(callback: (maximized: boolean) => void): Promise<() => void> {
  if (!desktop) return () => {}
  const window = getCurrentWindow()
  callback(await window.isMaximized())
  return window.onResized(async () => callback(await window.isMaximized()))
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

export async function getUpdateStatus(): Promise<UpdateStatus> { requireDesktop(); return invoke('update_status') }
export async function checkUpdate(): Promise<UpdateStatus> { requireDesktop(); return invoke('check_update') }
export async function downloadUpdate(): Promise<UpdateStatus> { requireDesktop(); return invoke('download_update') }
export async function deferUpdate(): Promise<UpdateStatus> { requireDesktop(); return invoke('defer_update') }
export async function cancelUpdateDownload(): Promise<void> { requireDesktop(); await invoke('cancel_update_download') }
export async function installUpdate(): Promise<void> { requireDesktop(); await invoke('install_update') }
export async function dismissUpdateNotice(): Promise<void> { requireDesktop(); await invoke('dismiss_update_notice') }
export interface BrowserStatus {id:string;name:string;installed:boolean;connected:boolean;extensionVersion?:string;bundledVersion?:string;updateAvailable?:boolean;refreshRequired?:boolean}
export async function browserStatus(): Promise<BrowserStatus[]> { requireDesktop(); return invoke('browser_status') }
export interface AutostartStatus { enabled: boolean; blocked: boolean }
export async function autostartStatus(): Promise<AutostartStatus> { requireDesktop(); return invoke('autostart_status') }
export async function setAutostart(enabled: boolean): Promise<AutostartStatus> { requireDesktop(); return invoke('set_autostart', { enabled }) }
export interface ExtensionRelease {version:string;title:string;notes:string;url:string;fileName:string;size:number;publishedAt:string}
export async function checkBrowserExtensionUpdate():Promise<ExtensionRelease>{requireDesktop();return invoke('check_browser_extension_update')}
export async function saveBrowserExtensionUpdate(release:ExtensionRelease):Promise<string|null>{requireDesktop();const path=await save({title:'下载插件更新',defaultPath:release.fileName,filters:[{name:'ZIP 压缩包',extensions:['zip']}]});return path?invoke('download_browser_extension_update',{path,version:release.version}):null}
export async function openBrowserExtensionRelease(url:string):Promise<void>{requireDesktop();return invoke('open_browser_extension_release',{url})}
export async function installBrowserExtension(browser:string): Promise<string> { requireDesktop(); return invoke('install_browser_extension',{browser}) }
export async function saveBrowserExtensionZip(): Promise<string | null> {
  requireDesktop()
  const path = await save({ title: '保存插件 ZIP', defaultPath: 'SnowDown_BrowserExtension_0.1.9.zip', filters: [{ name: 'ZIP 压缩包', extensions: ['zip'] }] })
  return path ? invoke('export_browser_extension_zip', { path }) : null
}
export async function openUpdateRelease(): Promise<void> { requireDesktop(); await invoke('open_update_release') }
export async function subscribeUpdates(callback: (status: UpdateStatus) => void): Promise<() => void> {
  if (!desktop) return () => {}
  return listen<UpdateStatus>('snowdown://update', event => callback(event.payload))
}
