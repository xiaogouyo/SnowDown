import type { Platform, SourceLink } from './types'
import sites from '../shared/platforms.json'

export const platformLabels: Record<Platform, string> = { bilibili: 'B站', douyin: '抖音', xiaohongshu: '小红书', web: '网页视频' }
const matches = (host: string, base: string) => host === base || host.endsWith(`.${base}`)
export function sourceLabel(source: {platform: Platform; url?: string; sourceUrl?: string}): string {
  if(source.platform !== 'web') return platformLabels[source.platform]
  try { const host=new URL(source.url||source.sourceUrl||'').hostname;return sites.find(site=>site.hosts.some(base=>matches(host,base)))?.label||platformLabels.web } catch {return platformLabels.web}
}

export function identifySource(text: string): SourceLink | null {
  const bare = text.trim()
  if (/^(BV[A-Za-z0-9]{10}|av\d+)$/i.test(bare)) return { platform: 'bilibili', url: `https://www.bilibili.com/video/${bare}` }
  for (const raw of text.match(/https?:\/\/[^\s<>"'“”，,；;。！？（）【】()[\]{}]+/gi) ?? []) {
    try {
      const url = new URL(raw.replace(/[.,;]+$/, ''))
      if (url.username || url.password || url.port) continue
      url.protocol = 'https:'; url.hash = ''
      const host = url.hostname.toLowerCase()
      if (matches(host, 'bilibili.com') || host === 'b23.tv') {
        if (host === 'b23.tv') return { platform: 'bilibili', url: url.toString() }
        const id = url.pathname.match(/\/(BV[A-Za-z0-9]{10}|av\d+)/i)?.[1]
        if (!id) continue
        const page = Number(url.searchParams.get('p'))
        return { platform: 'bilibili', url: `https://www.bilibili.com/video/${id}${page > 1 ? `?p=${page}` : ''}` }
      }
      if (matches(host, 'douyin.com') || matches(host, 'iesdouyin.com')) {
        if (host === 'v.douyin.com') return { platform: 'douyin', url: url.toString() }
        const id = url.pathname.match(/\/video\/(\d{8,24})(?:\/|$)/)?.[1] ?? url.searchParams.get('modal_id') ?? url.searchParams.get('aweme_id') ?? url.searchParams.get('vid')
        if (id && /^\d{8,24}$/.test(id)) return { platform: 'douyin', url: `https://www.douyin.com/video/${id}` }
      }
      if (matches(host, 'xiaohongshu.com') || matches(host, 'xhslink.com')) {
        if (matches(host, 'xhslink.com')) return { platform: 'xiaohongshu', url: url.toString() }
        const id = url.pathname.match(/\/(?:explore|discovery\/item)\/([a-f\d]{24})/i)?.[1]
        if (!id) continue
        const canonical = new URL(`https://www.xiaohongshu.com/explore/${id}`)
        for (const key of ['xsec_token', 'xsec_source']) { const value = url.searchParams.get(key); if (value) canonical.searchParams.set(key, value) }
        return { platform: 'xiaohongshu', url: canonical.toString() }
      }
      if (sites.some(site => !['bilibili','douyin','xiaohongshu'].includes(site.id) && site.hosts.some(base => matches(host,base)))) return {platform:'web',url:url.toString()}
    } catch { /* Ignore text that is not a supported URL. */ }
  }
  return null
}

/** Multiple share messages, URL lists and bare Bili IDs; never split an access query on '&'. */
export function identifySources(text: string): { sources: SourceLink[]; duplicates: number; unsupported: number } {
  const tokens = text.match(/https?:\/\/[^\s<>"'“”，,；;。！？（）【】()[\]{}]+|\bBV[A-Za-z0-9]{10}\b|\bav\d+\b/gi) ?? []
  const unique = new Map<string, SourceLink>()
  let duplicates = 0, unsupported = 0
  for (const token of tokens) {
    const source = identifySource(token)
    if (!source) { unsupported++; continue }
    const key = source.platform === 'xiaohongshu' ? sourceSummary(source) : source.url
    const previous = unique.get(key)
    if (previous) {
      duplicates++
      // A later complete share URL may carry the access token missing from the first one.
      if (source.platform === 'xiaohongshu' && !new URL(previous.url).searchParams.has('xsec_token')) unique.set(key, source)
    } else unique.set(key, source)
  }
  return { sources: [...unique.values()], duplicates, unsupported }
}

export function sourceSummary(source: SourceLink): string {
  try { const url = new URL(source.url); return `${url.hostname}${url.pathname}` } catch { return '' }
}
