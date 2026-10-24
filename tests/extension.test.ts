import {describe,it,expect} from 'vitest'
// Browser-neutral extension logic is tested against exact share/response fixtures.
// @ts-expect-error JavaScript extension source is bundled separately.
import {platformOf,normalizeResources,filterResources,safeName} from '../extension/core.js'
// @ts-expect-error JavaScript extension source is bundled separately.
import {detailVideo} from '../extension/sites.js'
describe('browser resource and platform parsing',()=>{
 it('restricts source platforms to their real host suffix',()=>{expect(platformOf('https://www.bilibili.com/video/BV17x411w7KC')).toBe('bilibili');expect(platformOf('https://douyin.com.evil.test/video/1')).toBeNull();expect(platformOf('https://www.xiaohongshu.com/explore/a')).toBe('xiaohongshu')})
 it('deduplicates loaded resources, preserves dimensions and applies filters',()=>{const items=normalizeResources([{url:'https://example.com/a.jpg',width:800,height:600},{url:'https://example.com/a.jpg',width:0},{url:'https://example.com/a.mp4'},{url:'https://example.com/a.m3u8'},{url:'javascript:alert(1)',kind:'image'}]);expect(items).toHaveLength(3);expect(filterResources(items,{kind:'image',minWidth:700,minHeight:500})).toHaveLength(1);expect(filterResources(items,{kind:'video'})).toHaveLength(1);expect(safeName('../bad:file.mp4')).not.toContain('/')})
 it('reads Douyin video/audio from the matching detail',()=>{const video=detailVideo({platform:'douyin',title:'page',detail:{aweme_detail:{desc:'视频',author:{nickname:'作者'},video:{width:1280,height:720,play_addr:{url_list:['https://v.douyinvod.com/a.mp4']},bit_rate_audio:[{audio_meta:{bitrate:128000,url_list:{main_url:'https://v.douyinvod.com/audio.m4a'}}}]}}}});expect(video.title).toBe('视频');expect(video.streams[0].audio).toContain('audio.m4a')})
 it('reads XHS streams with fallback addresses',()=>{const id='1234567890abcdef12345678';const video=detailVideo({platform:'xiaohongshu',id,detail:{note:{noteDetailMap:{[id]:{note:{title:'笔记',video:{media:{stream:{h264:[{width:720,height:1280,backupUrls:['https://sns-video.xhscdn.com/a.mp4']}]}}}}}}}}});expect(video.title).toBe('笔记');expect(video.streams[0].label).toBe('720P')})
})
