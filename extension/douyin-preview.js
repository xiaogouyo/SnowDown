// Runs in MAIN world. Project only nearby video identities and public card
// metadata; never return React state, account data, cookies or media URLs.
export function douyinPreviewTargets() {
  if (!/(^|\.)douyin\.com$/.test(location.hostname)) return [];
  const videoId = input => {
    try {
      const url = new URL(input, location.href);
      if (url.protocol !== 'https:' || !/(^|\.)douyin\.com$/.test(url.hostname) || url.username || url.password || url.port) return null;
      const id = url.pathname.match(/^\/video\/(\d{8,24})(?:\/|$)/)?.[1]
        || ['modal_id', 'aweme_id', 'vid'].map(key => url.searchParams.get(key)).find(value => /^\d{8,24}$/.test(value || ''));
      return id && /^\d{8,24}$/.test(id) ? id : null;
    } catch { return null; }
  };
  const pageId = videoId(location.href), results = new Map();
  let visited = 0;
  function nearbyIdentity(node) {
    for (let parent = node, level = 0; parent && level < 10 && parent !== document.body && parent !== document.documentElement; parent = parent.parentElement, level++) {
      const ids = new Set(), seen = new WeakSet();
      function find(value, depth = 0) {
        if (!value || typeof value !== 'object' || value instanceof Node || value === window || seen.has(value) || depth > 12 || ++visited > 18000) return;
        seen.add(value);
        const id = String(value.aweme_id || value.awemeId || '');
        if (/^\d{8,24}$/.test(id) && value.video && typeof value.video === 'object') { ids.add(id); return; }
        for (const key of Object.keys(value).slice(0, 80)) {
          if (['return', 'child', 'sibling', 'stateNode', '_owner'].includes(key)) continue;
          try { find(value[key], depth + 1); } catch {}
          if (ids.size > 1) return;
        }
      }
      for (const name of ['data-aweme-id', 'data-video-id']) {
        const id = parent.getAttribute(name);
        if (/^\d{8,24}$/.test(id || '')) ids.add(id);
      }
      for (const key of Object.keys(parent).filter(key => /^__react(?:Props|Fiber)\$/.test(key)).slice(0, 4)) {
        try {
          const value = parent[key];
          if (key.startsWith('__reactProps')) find(value);
          else { find(value?.memoizedProps); find(value?.pendingProps); }
        } catch {}
      }
      if (ids.size === 1) return [...ids][0];
      // An ancestor owning a feed/list is not evidence for any one child card.
      if (ids.size > 1) return null;
      if (parent.matches('a[href]')) { const id = videoId(parent.href); if (id) return id; }
    }
    return null;
  }
  const visible = node => {
    const r = node.getBoundingClientRect();
    if(r.width<100||r.height<70||r.bottom<=0||r.top>=innerHeight||r.right<=0||r.left>=innerWidth)return false;
    const style=getComputedStyle(node);
    return style.display !== 'none' && style.visibility !== 'hidden' && !(style.opacity && Number(style.opacity) === 0);
  };
  const videos = [...document.querySelectorAll('video')].filter(visible).slice(0, 16);
  const seeds = [...videos, ...document.querySelectorAll('img,[data-e2e="video-card"],[data-e2e="feed-active-video"],[data-aweme-id],[data-video-id],[style*="background-image"]')];
  let examined = 0;
  for (const node of seeds) {
    if (++examined > 1200 || results.size >= 36) break;
    if (!visible(node)) continue;
    let id = nearbyIdentity(node);
    // A video page may have no anchor around its current player. Do not apply
    // the page id to images or to multiple simultaneously visible players.
    if (!id && node.tagName === 'VIDEO' && videos.length === 1) id = pageId;
    if (!id) continue;
    const r = node.getBoundingClientRect(), card = node.closest('[data-e2e="video-card"],article,li') || node.parentElement;
    const image = node.tagName === 'IMG' ? node : node.querySelector('img');
    const url = `https://www.douyin.com/video/${id}`;
    let key=node.getAttribute('data-snowdown-preview-key');
    if(!/^sd-[a-f\d-]{36}$/.test(key||'')){key=`sd-${crypto.randomUUID()}`;node.setAttribute('data-snowdown-preview-key',key);}
    const cover=image?.currentSrc || image?.src || node.poster || '';
    const item = {key, url, title: String(node.title || image?.alt || card?.getAttribute('aria-label') || '').slice(0, 200), cover: /^https?:\/\//.test(cover)&&cover.length<8192?cover:'', rect: {left:r.left, top:r.top, right:r.right, bottom:r.bottom, width:r.width, height:r.height}, player: node.tagName === 'VIDEO'};
    const previous = results.get(url);
    if (!previous || item.player || !previous.player && r.width * r.height < previous.rect.width * previous.rect.height) results.set(url, item);
  }
  return [...results.values()];
}
