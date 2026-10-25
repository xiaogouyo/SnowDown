import {afterEach,describe,expect,it,vi} from 'vitest'
// @ts-expect-error Extension module is bundled separately.
import {douyinPreviewTargets} from '../extension/douyin-preview.js'
// @ts-expect-error Extension module is bundled separately.
import {capturePage} from '../extension/page.js'
const first='1234567890123456789',second='9876543210987654321'
const item=(id=first)=>({awemeId:id,desc:'卡片视频',author:{nickname:'测试作者',private:'never project'},video:{width:720,height:1280,playAddr:[{src:'https://fixture.douyinvod.com/current.mp4'}]}})
function setup(path='/jingxuan'){
 vi.stubGlobal('location',new URL('https://www.douyin.com'+path));vi.stubGlobal('innerHeight',900);vi.stubGlobal('innerWidth',1400)
 document.body.innerHTML='<section><article><img alt="预览封面"></article><article><img alt="另一个视频"></article></section>'
 for(const [index,node]of [...document.querySelectorAll('img')].entries())vi.spyOn(node,'getBoundingClientRect').mockReturnValue({x:100+index*320,y:100,left:100+index*320,top:100,right:400+index*320,bottom:400,width:300,height:300,toJSON(){}})
}
afterEach(()=>{vi.restoreAllMocks();vi.unstubAllGlobals();document.body.innerHTML=''})
describe('Douyin cards without video anchors',()=>{
 it('finds distinct React-backed cards and returns no React or account state',()=>{setup('/user/fixture');document.querySelectorAll('article').forEach((node,index)=>Object.assign(node,{__reactProps$test:{item:item(index?second:first)}}));const result=douyinPreviewTargets();expect(result.map((x:{url:string})=>x.url)).toEqual([`https://www.douyin.com/video/${first}`,`https://www.douyin.com/video/${second}`]);expect(result[0].key).toMatch(/^sd-/);expect(document.querySelector(`[data-snowdown-preview-key="${result[0].key}"]`)).toBe(document.querySelector('img'));expect(JSON.stringify(result)).not.toMatch(/never project|playAddr|awemeId/);expect(capturePage({targetUrl:result[1].url}).detail.aweme_detail.aweme_id).toBe(second)})
 it('does not assign an entire feed to one ambiguous child card',()=>{setup();Object.assign(document.querySelector('section')!,{__reactProps$test:{items:[item(),item(second)]}});expect(douyinPreviewTargets()).toHaveLength(0)})
 it('uses the current modal id for one visible anchorless player',()=>{setup(`/?modal_id=${first}`);document.body.innerHTML='<video></video>';vi.spyOn(document.querySelector('video')!,'getBoundingClientRect').mockReturnValue({x:100,y:100,left:100,top:100,right:900,bottom:600,width:800,height:500,toJSON(){}});expect(douyinPreviewTargets()[0].url).toBe(`https://www.douyin.com/video/${first}`)})
 it('ignores hidden cards and untrusted lookalike domains',()=>{setup();Object.assign(document.querySelector('article')!,{__reactProps$test:{item:item()}});document.querySelector('img')!.style.visibility='hidden';expect(douyinPreviewTargets()).toHaveLength(0);vi.stubGlobal('location',new URL('https://www.douyin.com.evil.test/jingxuan'));expect(douyinPreviewTargets()).toHaveLength(0)})
})
