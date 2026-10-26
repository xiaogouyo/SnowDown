import {describe,it,expect,vi,afterEach} from 'vitest'
import catalog from '../shared/platforms.json'
import {identifySource} from '../src/sources'
// @ts-expect-error Browser source is bundled separately.
import {platformOf} from '../extension/core.js'
// @ts-expect-error Browser source is bundled separately.
import {siteOf,previewUrl} from '../extension/catalog.js'
// @ts-expect-error Browser source is bundled separately.
import {genericVideo,downloadRows} from '../extension/media.js'
// @ts-expect-error Browser source is bundled separately.
import {hlsVariants} from '../extension/manifests.js'
// @ts-expect-error Browser source is bundled separately.
import {capturePage} from '../extension/page.js'
afterEach(()=>{vi.unstubAllGlobals();document.body.innerHTML=''})
describe('shared site catalog and real format menus',()=>{
 it('handles a YouTube page that has no player data or video id',async()=>{vi.stubGlobal('location',new URL('https://www.youtube.com/'));expect((await capturePage()).media).toHaveLength(0)})
 it('recognizes all fifty catalog entries in desktop and extension and rejects lookalike hosts',async()=>{expect(catalog).toHaveLength(50);for(const site of catalog){const path=site.id==='bilibili'?'/video/BV17x411w7KC':site.id==='douyin'?'/video/1234567890123456789':site.id==='xiaohongshu'?'/explore/1234567890abcdef12345678':'/video/fixture';const url='https://www.'+site.hosts[0]+path;expect(siteOf(url)?.id).toBe(site.id);expect(identifySource(url)?.platform).toBe(platformOf(url));expect(platformOf('https://'+site.hosts[0]+'.evil.test/video/fixture')).toBeNull();}})
 it('accepts real preview links and excludes profile/search/navigation links',async()=>{expect(previewUrl('/watch?v=fixture','https://www.youtube.com/')).toContain('/watch?v=fixture');expect(previewUrl('/@someone','https://www.youtube.com/')).toBeNull();expect(previewUrl('/search','https://www.bilibili.com/')).toBeNull();expect(previewUrl('https://www.bilibili.com.evil.test/video/BV17x411w7KC','https://www.bilibili.com')).toBeNull();expect(previewUrl('/reel/fixture/','https://www.instagram.com/')).toContain('/reel/fixture/')})
 it('preserves only the provided formats instead of manufacturing resolutions or MP3',async()=>{const video=genericVideo({url:'https://vimeo.com/123',title:'fixture',media:[{url:'https://cdn.example.com/a.webm',width:1280,height:720},{url:'https://cdn.example.com/a.webm',height:1080},{url:'javascript:alert(1)'}]});expect(video.streams).toHaveLength(1);expect(downloadRows(video).map((r:{label:string})=>r.label)).toEqual(['webm (720p)','m4a']);expect(video.detail.webMedia.sourceUrl).toBe('https://vimeo.com/123')})
 it('labels portrait resolution by its short side while preserving native height selection',async()=>{const video=genericVideo({url:'https://www.tiktok.com/@fixture/video/123',media:[{url:'https://cdn.example.com/a.mp4',width:720,height:1280}]});expect(video.streams[0].quality).toBe(1280);expect(downloadRows(video)[0].label).toBe('mp4 (720p)')})
 it('resolves HLS variants and their associated audio without discarding signed query parameters',async()=>{const rows=hlsVariants('#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="a",URI="audio.m3u8?token=fixture"\n#EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1920x1080,AUDIO="a"\nvideo.m3u8?token=fixture\n','https://cdn.example.com/master.m3u8');expect(rows[0]).toMatchObject({height:1080,audio:'https://cdn.example.com/audio.m3u8?token=fixture',url:'https://cdn.example.com/video.m3u8?token=fixture',masterUrl:'https://cdn.example.com/master.m3u8'})})
 it('rejects DRM HLS and excludes DASH/blob addresses from simple file download',async()=>{expect(()=>hlsVariants('#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI="key"','https://cdn.example.com/m.m3u8')).toThrow('受保护');expect(genericVideo({url:'https://vimeo.com/123',media:[{url:'blob:https://vimeo.com/fixture'},{url:'https://cdn.example.com/a.mpd'}]})).toBeNull()})
 it('recovers direct YouTube formats only for the current video and keeps a separate audio stream',async()=>{vi.stubGlobal('location',new URL('https://www.youtube.com/watch?v=fixture'));vi.stubGlobal('ytInitialPlayerResponse',{videoDetails:{videoId:'fixture'},streamingData:{adaptiveFormats:[{url:'https://cdn.example.com/video',mimeType:'video/mp4',height:720,width:1280},{url:'https://cdn.example.com/audio',mimeType:'audio/mp4',bitrate:128000}]}});expect((await capturePage()).media[0]).toMatchObject({url:'https://cdn.example.com/video',audio:'https://cdn.example.com/audio'});vi.stubGlobal('location',new URL('https://www.youtube.com/watch?v=other'));expect((await capturePage()).media).toHaveLength(0)})
})
