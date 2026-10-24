import { afterEach, describe, expect, it, vi } from 'vitest'
import script from '../src-tauri/src/site_qr.js?raw'

function read(platform = 'douyin') { return JSON.parse(window.eval(script.replace('__PLATFORM__', JSON.stringify(platform)))) }
function visibleElements() {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({ width: 180, height: 180, top: 0, left: 0, bottom: 180, right: 180, x: 0, y: 0, toJSON() {} })
}
const qr = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUg=='
afterEach(() => { document.body.innerHTML = ''; vi.restoreAllMocks(); vi.useRealTimers(); delete (window as unknown as Record<string, unknown>).__snowdownQrRequested })
describe('official QR capture boundaries', () => {
  it('captures only the logged-in account avatar and name',()=>{
    visibleElements();document.body.innerHTML='<header><a href="/user/self" title="我的账号"><img src="https://p3.douyinpic.com/avatar.jpg" alt="头像"></a></header><img src="https://unrelated.test/author.jpg">';
    expect(read()).toMatchObject({state:'confirmed',account:{name:'我的账号',avatar:'https://p3.douyinpic.com/avatar.jpg'}})
  })
  it('captures the Douyin QR image near the scan-login text without reading the phone form', () => {
    visibleElements()
    document.body.innerHTML = `<section><h3>扫码登录</h3><div><div><img class="RhjdbXj8" src="${qr}"></div><span>如何扫码</span></div></section><input value="private-phone"><p>无关页面文字</p>`
    const result = read()
    expect(result).toEqual({ state: 'waiting', image: qr })
    expect(JSON.stringify(result)).not.toContain('private-phone')
  })
  it('captures the XHS QR and reports official expiration and phone confirmation', () => {
    visibleElements()
    document.body.innerHTML = `<div class="qrcode"><img class="qrcode-img" src="${qr}"><p>二维码已过期</p></div>`
    expect(read('xiaohongshu').state).toBe('expired')
    document.querySelector('p')!.textContent = '已扫码，请在手机确认'
    expect(read('xiaohongshu')).toEqual({ state: 'scanned', image: qr })
  })
  it('does not expose unrelated images, inputs or page text', () => {
    visibleElements()
    document.body.innerHTML = `<img src="${qr}" alt="用户头像"><img src="https://example.test/picture.jpg"><input value="private-password">`
    expect(read()).toEqual({ state: 'loading' })
  })
  it('opens only a login button once, leaving verification and SMS controls alone', () => {
    visibleElements()
    document.body.innerHTML = '<button>获取验证码</button><button>登录</button>'
    const sms = vi.spyOn(document.querySelectorAll('button')[0]!, 'click')
    const login = vi.spyOn(document.querySelectorAll('button')[1]!, 'click')
    read(); read()
    expect(login).toHaveBeenCalledTimes(1)
    expect(sms).not.toHaveBeenCalled()
  })
  it('retries a login entry after page hydration without submitting a phone form', () => {
    visibleElements(); vi.useFakeTimers(); vi.setSystemTime(new Date('2026-10-24T12:00:00Z'))
    document.body.innerHTML = '<form><button>登录</button></form><button id="entry">登录</button>'
    const submit=vi.spyOn(document.querySelector('form button')! as HTMLElement,'click')
    const entry=vi.spyOn(document.getElementById('entry')!,'click')
    read(); vi.setSystemTime(Date.now()+3100); read()
    expect(entry).toHaveBeenCalledTimes(2)
    expect(submit).not.toHaveBeenCalled()
  })
  it('hands off security verification and confirms only the platform account control', () => {
    visibleElements()
    document.body.innerHTML = '<div class="captcha-box">拖动滑块完成安全验证</div>'
    expect(read()).toEqual({ state: 'verification' })
    document.body.innerHTML = '<a href="/user/profile/123">某笔记作者</a>'
    expect(read('xiaohongshu').state).toBe('loading')
    document.body.innerHTML = '<a href="/user/profile/1234567890abcdef12345678">我</a>'
    expect(read('xiaohongshu')).toEqual({ state: 'confirmed' })
    document.body.innerHTML += '<button class="login-btn">登录</button>'
    expect(read('xiaohongshu').state).toBe('loading')
  })
})
