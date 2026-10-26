import {afterEach,describe,expect,it,vi} from 'vitest'
// @ts-expect-error Serialized browser function.
import {youtubePagePlayer} from '../extension/youtube-player.js'
// @ts-expect-error Browser parser helpers.
import {youtubeMedia,youtubeQualities} from '../extension/youtube.js'
// @ts-expect-error Browser manifest parser.
import {hlsVariants} from '../extension/manifests.js'
const id='8fVlLJcwMeI',target=`https://www.youtube.com/shorts/${id}`
const answer=()=>({playabilityStatus:{status:'OK'},videoDetails:{videoId:id,title:'Shorts',author:'Author',lengthSeconds:'87'},streamingData:{hlsManifestUrl:'https://manifest.googlevideo.com/api/manifest/hls_playlist/test.m3u8'}})
function setup(value:unknown=answer()){
 vi.stubGlobal('location',new URL(target))
 const mock=vi.fn(async()=>({ok:true,body:new ReadableStream({start(controller){controller.enqueue(new TextEncoder().encode(JSON.stringify(value)));controller.close()}})}))
 vi.stubGlobal('fetch',mock);return mock
}
afterEach(()=>vi.unstubAllGlobals())
describe('YouTube official player and real variants',()=>{
 it('reads the exact Shorts through the official same-origin player with the browser session',async()=>{
  const fetch=setup();const value=await youtubePagePlayer(target)
  expect(value.videoDetails.videoId).toBe(id)
  const [url,options]=fetch.mock.calls[0] as unknown as [string,RequestInit]
  expect(url).toBe('/youtubei/v1/player?prettyPrint=false');expect(options.credentials).toBe('include')
  const body=JSON.parse(options.body as string);expect(body.videoId).toBe(id);expect(body.context.client.clientName).toBe('VISIONOS')
  expect(youtubeMedia(value,id)[0]).toMatchObject({kind:'manifest',ext:'mp4'})
 })
 it('does not request a foreign site or a YouTube navigation page',async()=>{
  const fetch=setup();await expect(youtubePagePlayer('https://evil.test/watch?v='+id)).rejects.toThrow()
  await expect(youtubePagePlayer('https://www.youtube.com/')).rejects.toThrow();expect(fetch).not.toHaveBeenCalled()
 })
 it('rejects a wrong video, official verification failure and live responses',async()=>{
  for(const value of [{...answer(),videoDetails:{videoId:'jNQXAC9IVRw'}},{...answer(),playabilityStatus:{status:'LOGIN_REQUIRED'}},{...answer(),videoDetails:{videoId:id,isLiveContent:true}}]){
   setup(value);await expect(youtubePagePlayer(target)).rejects.toThrow(/验证/)
  }
 })
 it('caps the player response instead of processing unlimited webpage data',async()=>{
  setup({padding:'x'.repeat(4*1024*1024+1)});await expect(youtubePagePlayer(target)).rejects.toThrow(/过大/)
 })
 it('does not offer untransformed n URLs, signature ciphers or an unrelated manifest as downloadable files',()=>{
  const value={...answer(),streamingData:{formats:[{mimeType:'video/mp4',url:'https://rr.googlevideo.com/videoplayback?n=unsolved'},{mimeType:'video/mp4',signatureCipher:'s=unsolved'}],hlsManifestUrl:'https://evil.test/playlist.m3u8'}}
  expect(youtubeMedia(value,id)).toEqual([])
  expect(youtubeMedia(answer(),'jNQXAC9IVRw')).toEqual([])
 })
 it('keeps distinct resolutions, prefers H264 and the default audio without offering an unreadable master',()=>{
  const text='#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="a",DEFAULT=YES,URI="audio.m3u8"\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="a",URI="dubbed.m3u8"\n#EXT-X-STREAM-INF:RESOLUTION=1080x1920,CODECS="vp09",AUDIO="a"\nvp9.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=1080x1920,CODECS="avc1.640028",AUDIO="a"\nh264.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=720x1280,CODECS="avc1.64001F",AUDIO="a"\n720.m3u8'
  const rows=youtubeQualities(hlsVariants(text,'https://manifest.googlevideo.com/master.m3u8'))
  expect(rows).toHaveLength(2);expect(rows[0].url).toContain('h264.m3u8');expect(rows[0].audio).toContain('/audio.m3u8')
  expect(youtubeQualities([{url:'https://manifest.googlevideo.com/master.m3u8',kind:'manifest'}])).toEqual([])
 })
})
