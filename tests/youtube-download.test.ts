import {afterEach,describe,expect,it,vi} from 'vitest'
// @ts-expect-error Browser download helper.
import {youtubeRangeFetch,youtubeDownloadBuffer} from '../extension/youtube-download.js'
afterEach(()=>vi.unstubAllGlobals())
describe('bounded YouTube media reading',()=>{
 it('reads all chunks with exact ranges and validates totals',async()=>{
  const total=128*1024+19,calls:string[]=[]
  vi.stubGlobal('fetch',vi.fn(async(_url:string,init:RequestInit)=>{
   const range=new Headers(init.headers).get('Range')!;calls.push(range);const [,a,b]=/^bytes=(\d+)-(\d+)$/.exec(range)!;const first=Number(a),last=Number(b)
   expect(init.credentials).toBe('omit')
   return new Response(new Uint8Array(last-first+1).fill(first?2:1),{status:206,headers:{'Content-Range':`bytes ${first}-${last}/${total}`}})
  }))
  const bytes=await youtubeDownloadBuffer(`https://rr.googlevideo.com/videoplayback?clen=${total}`)
  expect(bytes.length).toBe(total);expect(bytes[0]).toBe(1);expect(bytes.at(-1)).toBe(2)
  expect(calls).toEqual(['bytes=0-131071','bytes=131072-131090'])
 })
 it('rejects ignored ranges and mismatched or truncated byte counts',async()=>{
  for(const response of [new Response(new Uint8Array(2),{status:200}),new Response(new Uint8Array(2),{status:206,headers:{'Content-Range':'bytes 0-9/10'}}),new Response(new Uint8Array(11),{status:206,headers:{'Content-Range':'bytes 0-9/10'}})]){
   vi.stubGlobal('fetch',vi.fn(async()=>response));await expect(youtubeDownloadBuffer('https://rr.googlevideo.com/videoplayback?clen=10')).rejects.toThrow(/分块/)
  }
 })
 it('does not buffer other hosts, unknown sizes or large files',async()=>{
  const fetch=vi.fn();vi.stubGlobal('fetch',fetch)
  for(const url of ['https://rr.googlevideo.com.evil.test/videoplayback?clen=12','https://rr.googlevideo.com/videoplayback','https://rr.googlevideo.com/videoplayback?clen=999999999'])expect(await youtubeDownloadBuffer(url)).toBeNull()
  expect(fetch).not.toHaveBeenCalled()
 })
 it('passes ordinary site downloads to the browser unchanged',async()=>{
  const response=new Response('ok'),fetch=vi.fn(async()=>response);vi.stubGlobal('fetch',fetch)
  const options={headers:{Range:'bytes=1-'}};expect(await youtubeRangeFetch('https://example.com/video.mp4',options)).toBe(response)
  expect(fetch).toHaveBeenCalledWith('https://example.com/video.mp4',options)
 })
})
