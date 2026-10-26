import {describe,it,expect,vi} from 'vitest'
// @ts-expect-error Browser modules.
import {cacheKey,cacheSite,authCookieSite,retryStableParse} from '../extension/cache-context.js'
// @ts-expect-error Browser module.
import {ParseCache} from '../extension/parse-cache.js'
const result={video:{streams:[{url:'https://cdn.example.com/a.mp4'}]}}
describe('parse invalidation scope',()=>{
 it('separates explicit preview targets from the currently playing video',async()=>{
  const cache=new ParseCache(),current=cacheKey(1,'https://www.bilibili.com/',null),preview=cacheKey(1,'https://www.bilibili.com/','https://www.bilibili.com/video/BV17x411w7KC');
  let finish!:(v:unknown)=>void;const work=cache.resolve(preview,()=>new Promise(resolve=>{finish=resolve}));cache.invalidate('1:current:');finish(result);await expect(work).resolves.toEqual(result);expect(cacheSite(current)).toBe('bilibili');expect(cacheSite(preview)).toBe('bilibili')
 })
 it('ignores tracking and csrf cookies and recognizes exact authentication cookies',()=>{
  for(const name of ['b_lsid','buvid3','bili_jct','analytics_token'])expect(authCookieSite({domain:'.bilibili.com',name})).toBeNull();
  expect(authCookieSite({domain:'.bilibili.com',name:'SESSDATA'})).toBe('bilibili');expect(authCookieSite({domain:'.xiaohongshu.com',name:'web_session'})).toBe('xiaohongshu');expect(authCookieSite({domain:'.bilibili.com.evil.test',name:'SESSDATA'})).toBeNull()
 })
 it('invalidates only the account platform that actually changed',async()=>{
  const cache=new ParseCache();const a=cacheKey(1,'https://www.bilibili.com/',null),b=cacheKey(2,'https://www.xiaohongshu.com/',null);await cache.resolve(a,async()=>result);await cache.resolve(b,async()=>result);cache.invalidateWhere((key:string)=>cacheSite(key)==='xiaohongshu','auth');expect(cache.get(a)).not.toBeNull();expect(cache.get(b)).toBeNull()
 })
 it('retries a stable context once and never loops or retries revoked permission/navigation',async()=>{
  const stale=Object.assign(Error('changed'),{code:'PARSE_CONTEXT_CHANGED',retryable:true});const work=vi.fn().mockRejectedValueOnce(stale).mockResolvedValue(result);expect(await retryStableParse(work,async()=>true)).toEqual(result);expect(work).toHaveBeenCalledTimes(2);
  for(const stable of [true,false]){const job=vi.fn().mockRejectedValue({...stale,retryable:!stable});await expect(retryStableParse(job,async()=>stable)).rejects.toBeDefined();expect(job).toHaveBeenCalledTimes(1)}
  const repeat=vi.fn().mockRejectedValue(stale);await expect(retryStableParse(repeat,async()=>true)).rejects.toBeDefined();expect(repeat).toHaveBeenCalledTimes(2)
 })
 it('does not cancel a browser parse when disconnected native-only cache entries are removed',async()=>{
  const cache=new ParseCache();let finish!:(v:unknown)=>void;const job=cache.resolve('browser',()=>new Promise(resolve=>{finish=resolve}));await cache.resolve('native',async()=>({video:{streams:[{native:true}]}}));cache.dropWhere((value:typeof result)=>value.video.streams.some((s:unknown)=>(s as {native?:boolean}).native));finish(result);await expect(job).resolves.toEqual(result);expect(cache.get('native')).toBeNull()
 })
})
