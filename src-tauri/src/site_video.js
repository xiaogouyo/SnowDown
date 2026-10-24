// Runs only on the app's hidden parser. Return media metadata for
// the requested video, never form values, credentials, or unrelated page state.
(() => {
  const id = __VIDEO_ID__;
  const result = { pageUrl: location.href, title: document.title };
  const note = window.__INITIAL_STATE__?.note?.noteDetailMap?.[id]?.note;
  if (note) result.state = { note: { noteDetailMap: { [id]: { note: {
    title: note.title, user: { nickname: note.user?.nickname },
    imageList: (note.imageList || []).slice(0, 1).map(x => ({ urlDefault: x.urlDefault })),
    video: { media: { stream: note.video?.media?.stream } }
  } } } } };
  const seen = new Set();
  function find(value, depth) {
    if (!value || typeof value !== 'object' || depth > 7 || seen.has(value)) return null;
    seen.add(value);
    if (String(value.aweme_id) === id && value.video) return value;
    for (const child of Object.values(value).slice(0, 150)) {
      const found = find(child, depth + 1); if (found) return found;
    }
    return null;
  }
  let detail = find(window._ROUTER_DATA, 0);
  if (!detail) {
    try {
      const raw = document.getElementById('RENDER_DATA')?.textContent;
      if (raw && raw.length < 2 * 1024 * 1024) detail = find(JSON.parse(decodeURIComponent(raw)), 0);
    } catch { /* Page configurations may not contain video metadata. */ }
  }
  if (detail) result.detail = {
    aweme_id: detail.aweme_id, desc: detail.desc,
    author: { nickname: detail.author?.nickname },
    video: { width: detail.video.width, height: detail.video.height,
      duration: detail.video.duration, cover: detail.video.cover,
      bit_rate: detail.video.bit_rate, play_addr: detail.video.play_addr,
      bit_rate_audio: detail.video.bit_rate_audio,
      play_addr_h264: detail.video.play_addr_h264, play_addr_bytevc1: detail.video.play_addr_bytevc1 }
  };
  const visible = Array.from(document.querySelectorAll('video')).filter(video => {
    const rect = video.getBoundingClientRect();
    return video.videoWidth > 0 && video.readyState > 0 && rect.width > 100 && rect.height > 100 && rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth;
  });
  if (visible.length === 1) {
    const video = visible[0];
    result.player = { url: video.currentSrc, cover: video.poster,
      width: video.videoWidth, height: video.videoHeight, duration: video.duration };
  }
  return JSON.stringify(result);
})()
