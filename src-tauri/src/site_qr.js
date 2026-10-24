// Read only the QR image and login status from the app's own official login page.
// This never reads form values, cookies, input text, or general page state.
(() => {
  const platform = __PLATFORM__;
  const visible = node => { const rect = node.getBoundingClientRect(); return rect.width > 0 && rect.height > 0 && getComputedStyle(node).visibility !== 'hidden'; };
  const nodes = [...document.querySelectorAll('img,canvas')].filter(visible);
  const candidate = nodes.find(node => {
    const rect = node.getBoundingClientRect();
    if (rect.width < 100 || rect.width > 420 || Math.abs(rect.width - rect.height) > 10) return false;
    const hint = `${node.className} ${node.getAttribute('alt') || ''}`;
    if (/qrcode|qr-code|二维码|\bRhjdbXj8\b/i.test(hint)) return true;
    if (node.tagName !== 'CANVAS' && !node.getAttribute('src')?.startsWith('data:image/')) return false;
    let parent = node.parentElement;
    for (let depth = 0; parent && depth < 6; depth++, parent = parent.parentElement) {
      const text = parent.textContent || '';
      if (text.length < 1500 && /扫码登录|扫描二维码|扫码后|如何扫码|打开.{0,8}扫一扫/.test(text)) return true;
    }
    return false;
  });
  let root = candidate?.parentElement || (window.__snowdownQrRoot?.isConnected ? window.__snowdownQrRoot : null);
  for (let depth = 0; root?.parentElement && depth < 5 && (root.textContent?.length || 0) < 1500; depth++) {
    if (/扫码登录|扫描二维码|二维码已|扫码成功/.test(root.textContent || '')) break;
    root = root.parentElement;
  }
  if (root) window.__snowdownQrRoot = root;
  const text = root?.textContent || '';
  const loginButton = [...document.querySelectorAll('button,[role="button"],[class*="login-btn"]')].find(node => visible(node) && /^(登录|立即登录|登录\/注册)$/.test(node.textContent.trim()));
  const challenge = [...document.querySelectorAll('[class*="captcha"],[id*="captcha"],[class*="verify"]')].some(node => visible(node) && /拖动|滑块|安全验证|验证码|完成验证/.test(node.textContent || ''));
  if (challenge) return JSON.stringify({state:'verification'});
  const profile = platform === 'xiaohongshu'
    ? [...document.querySelectorAll('a[href*="/user/profile/"]')].some(node => visible(node) && /^\/user\/profile\/[a-f\d]{24}(?:$|[/?])/i.test(node.getAttribute('href')) && /^(我|我的|个人主页)$/.test(node.textContent.trim()))
    : !![...document.querySelectorAll('header a[href*="/user/"],[data-e2e="user-avatar"],[data-e2e="user-info"]')].find(visible);
  if (profile && !candidate && !loginButton && document.readyState !== 'loading') {
    const root = platform === 'xiaohongshu'
      ? [...document.querySelectorAll('a[href*="/user/profile/"]')].find(node => /^(我|我的|个人主页)$/.test(node.textContent.trim()))
      : document.querySelector('[data-e2e="user-avatar"],[data-e2e="user-info"],header a[href*="/user/"]');
    const img = root?.querySelector('img') || (root?.tagName === 'IMG' ? root : null);
    const avatar = img?.currentSrc || img?.src || '';
    const name = root?.getAttribute('title') || img?.getAttribute('alt') || '';
    return JSON.stringify({state:'confirmed',...(avatar ? {account:{name:name.slice(0,80),avatar}} : {})});
  }
  if (/已过期|已失效|二维码失效|二维码过期/.test(text)) return JSON.stringify({state:'expired'});
  if (!candidate && /扫码成功|扫描成功|请在.{0,12}确认|已扫码/.test(text)) return JSON.stringify({state:'scanned'});
  let image = '';
  if (candidate) {
    try {
      if (candidate.tagName === 'CANVAS') image = candidate.toDataURL('image/png');
      else if (/^data:image\/(png|jpeg|webp);base64,/.test(candidate.src)) image = candidate.src;
      else if (candidate.complete && candidate.naturalWidth > 0) {
        const canvas = document.createElement('canvas');
        canvas.width = candidate.naturalWidth; canvas.height = candidate.naturalHeight;
        canvas.getContext('2d').drawImage(candidate, 0, 0); image = canvas.toDataURL('image/png');
      }
    } catch { /* Cross-origin images are not copied or exposed. */ }
  }
  if (image) return JSON.stringify({state:/扫码成功|扫描成功|请在.{0,12}确认|已扫码/.test(text)?'scanned':'waiting',image});
  const requested = window.__snowdownQrRequested || {at:0,count:0};
  if (requested.count < 4 && (!requested.at || Date.now() - requested.at >= 3000)) {
    const button = [...document.querySelectorAll('button,[role="button"]')].find(node => visible(node) && !node.closest('form,.login-container,.login-modal,[class*="captcha"]') && /^(登录|立即登录|登录\/注册|扫码登录)$/.test(node.textContent.trim()));
    if (button) { window.__snowdownQrRequested = {at:Date.now(),count:requested.count+1}; button.click(); }
  }
  return JSON.stringify({state:'loading'});
})()
