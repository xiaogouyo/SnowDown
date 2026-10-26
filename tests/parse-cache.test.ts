import { describe, it, expect, vi } from 'vitest'
// @ts-expect-error Browser implementation.
import { ParseCache, createLimiter } from '../extension/parse-cache.js'
const value = (url='https://cdn.example.com/video.mp4') => ({page:{url:'https://vimeo.com/1'},video:{title:'视频',streams:[{url}]}})
describe('bounded parser cache and concurrency',()=>{
 it('coalesces in-flight work, clones results, and expires successful results',async()=>{
  let now=1000;const cache=new ParseCache({ttl:100,now:()=>now});let finish!: (v:unknown)=>void
  const work=vi.fn(()=>new Promise(resolve=>{finish=resolve}));const a=cache.resolve('1:video',work),b=cache.resolve('1:video',work);finish(value());const [x,y]=await Promise.all([a,b]);expect(work).toHaveBeenCalledTimes(1);x.video.title='changed';expect(y.video.title).toBe('视频');expect((await cache.resolve('1:video',work)).cached).toBe(true);now=1101;expect(cache.get('1:video')).toBeNull()
 })
 it('rejects stale in-flight completion on navigation, keeps unrelated tabs, and never caches failure',async()=>{
  const cache=new ParseCache();await cache.resolve('2:other',async()=>value());let finish!:(v:unknown)=>void;const pending=cache.resolve('1:video',()=>new Promise(resolve=>{finish=resolve}));cache.invalidate('1:');finish(value());await expect(pending).rejects.toThrow('已变化');expect(cache.get('1:video')).toBeNull();expect(cache.get('2:other')).not.toBeNull();await expect(cache.resolve('bad',async()=>{throw Error('offline')})).rejects.toThrow('offline');expect(cache.get('bad')).toBeNull()
 })
 it('expires signed URLs early and bounds entry count and memory',async()=>{
  const cache=new ParseCache({limit:2,bytes:3000,now:()=>1700000000000});await cache.resolve('expired',async()=>value('https://cdn.example.com/video.mp4?expires=1699999999'));expect(cache.get('expired')).toBeNull();for(let i=0;i<4;i++)await cache.resolve(String(i),async()=>value());expect(cache.rows.size).toBe(2);expect(cache.bytes).toBeLessThanOrEqual(3000);cache.clear();expect(cache.bytes).toBe(0)
 })
 it('runs two independent tasks concurrently, rejects overflow, and drains after failure',async()=>{
  const limited=createLimiter(2,1);let active=0,max=0;const releases:Array<()=>void>=[];const work=()=>{active++;max=Math.max(max,active);return new Promise(resolve=>releases.push(()=>{active--;resolve('ok')}))};const a=limited(work),b=limited(work),c=limited(work);await expect(limited(work)).rejects.toThrow('队列已满');await Promise.resolve();expect(active).toBe(2);releases.shift()!();await a;await new Promise(resolve=>setTimeout(resolve,0));expect(max).toBe(2);while(releases.length)releases.shift()!();await Promise.all([b,c]);await expect(limited(()=>Promise.reject(Error('failed')))).rejects.toThrow('failed');expect(await limited(async()=>42)).toBe(42)
 })
 it('drops queued requests that have waited too long without starting them',async()=>{
  vi.useFakeTimers();try{const limited=createLimiter(1,2);let release!:(v:unknown)=>void;const active=limited(()=>new Promise(resolve=>{release=resolve}));await Promise.resolve();const work=vi.fn();const waiting=expect(limited(work)).rejects.toThrow('等待超时');await vi.advanceTimersByTimeAsync(10001);await waiting;expect(work).not.toHaveBeenCalled();release('done');await active}finally{vi.useRealTimers()}
 })
})
