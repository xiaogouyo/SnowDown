//! Clipboard detection and parsing live in Rust, independently of WebView timers/focus.
use crate::{clipboard, model::VideoInfo, sites, AppState};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Detected,
    Parsing,
    Ready,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capture {
    pub id: u64,
    pub source: sites::Source,
    pub phase: Phase,
    pub video: Option<VideoInfo>,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    enabled: bool,
    auto: bool,
    next_id: u64,
    revision: u64,
    capture: Option<Capture>,
    manual: Option<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub capture: Option<Capture>,
}

pub struct Monitor {
    inner: Mutex<Inner>,
    changed: watch::Sender<u64>,
    rescan: AtomicBool,
}
impl Default for Monitor {
    fn default() -> Self {
        Self {
            inner: Mutex::default(),
            changed: watch::channel(0).0,
            rescan: AtomicBool::new(true),
        }
    }
}

impl Monitor {
    #[cfg(any(test, debug_assertions))]
    pub fn snapshot(&self) -> Option<Capture> {
        self.inner.lock().unwrap().capture.clone()
    }
    pub fn status(&self) -> Snapshot {
        let inner = self.inner.lock().unwrap();
        Snapshot {
            revision: inner.revision,
            capture: inner.capture.clone(),
        }
    }
    fn signal(&self) {
        self.changed
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }
    pub fn configure(&self, enabled: bool, auto: bool) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let auto = enabled && auto;
        if inner.enabled == enabled && inner.auto == auto {
            return false;
        }
        inner.enabled = enabled;
        inner.auto = auto;
        self.rescan.store(true, Ordering::Relaxed);
        if !enabled {
            inner.capture = None;
        } else if let Some(capture) = &mut inner.capture {
            if capture.phase == Phase::Parsing {
                capture.phase = Phase::Detected;
            }
        }
        inner.revision += 1;
        self.signal();
        true
    }
    pub fn detect(&self, source: sites::Source, sequence: u32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled
            || inner.manual == Some(sequence)
            || inner.capture.as_ref().is_some_and(|capture| {
                capture.source.url == source.url && capture.phase != Phase::Error
            })
        {
            return false;
        }
        inner.next_id += 1;
        inner.capture = Some(Capture {
            id: inner.next_id,
            source,
            phase: Phase::Detected,
            video: None,
            error: None,
        });
        inner.revision += 1;
        self.signal();
        true
    }
    pub fn manual(&self, sequence: u32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        inner.manual = Some(sequence);
        if inner.capture.is_some() {
            inner.capture = None;
            inner.revision += 1;
            self.signal();
            return true;
        }
        false
    }
    pub fn acknowledge(&self, id: u64) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .capture
            .as_ref()
            .is_some_and(|capture| capture.id == id)
        {
            inner.capture = None;
            inner.revision += 1;
            self.signal();
            return true;
        }
        false
    }
    fn claim(&self, changes: &mut watch::Receiver<u64>) -> Option<Capture> {
        let mut inner = self.inner.lock().unwrap();
        // Observe the generation while holding the same lock as every mutation.
        // A signal for this very job must not cancel it and leave it stuck parsing.
        changes.borrow_and_update();
        if !inner.enabled || !inner.auto {
            return None;
        }
        let capture = inner.capture.as_mut()?;
        if capture.phase != Phase::Detected {
            return None;
        }
        capture.phase = Phase::Parsing;
        let job = capture.clone();
        inner.revision += 1;
        Some(job)
    }
    fn finish(&self, id: u64, result: Result<VideoInfo, String>) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled || !inner.auto {
            return false;
        }
        let Some(capture) = &mut inner.capture else {
            return false;
        };
        if capture.id != id || capture.phase != Phase::Parsing {
            return false;
        }
        match result {
            Ok(video) => {
                capture.video = Some(video);
                capture.phase = Phase::Ready;
            }
            Err(error) => {
                capture.error = Some(error);
                capture.phase = Phase::Error;
            }
        }
        inner.revision += 1;
        true
    }
}

pub fn emit(app: &AppHandle, state: &AppState) {
    let status = state.clipboard.status();
    let _ = app.emit_to("main", "snowdown://clipboard", &status);
    let text = match status.capture.as_ref().map(|capture| capture.phase) {
        Some(Phase::Parsing) => "SnowDown · 正在后台解析剪贴板链接",
        Some(Phase::Ready) => "SnowDown · 链接已解析，打开软件选择下载",
        Some(Phase::Error) => "SnowDown · 剪贴板链接解析失败，可打开软件重试",
        _ => "SnowDown · 好内容，慢慢收藏",
    };
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(text));
    }
}

pub fn start(app: AppHandle, state: Arc<AppState>) {
    let poll_app = app.clone();
    let poll_state = state.clone();
    tauri::async_runtime::spawn(async move {
        #[cfg(debug_assertions)]
        if std::env::var_os("SNOWDOWN_CLIPBOARD_VERIFY").is_some()
            || std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some()
            || std::env::var_os("SNOWDOWN_LOGIN_CHECK_VERIFY").is_some()
            || std::env::var_os("SNOWDOWN_MODE_MORPH_VERIFY").is_some()
        {
            return;
        }
        let mut seen = None;
        loop {
            let settings = poll_state.settings.lock().unwrap().clone();
            if poll_state
                .clipboard
                .configure(settings.clipboard_enabled, settings.clipboard_auto_parse)
            {
                emit(&poll_app, &poll_state);
            }
            if poll_state.clipboard.rescan.swap(false, Ordering::Relaxed) {
                seen = None;
            }
            if !settings.clipboard_enabled {
                seen = None;
            } else if !poll_state.updater.installing() {
                let sequence = clipboard::sequence();
                if sequence != 0 && seen != Some(sequence) {
                    // A busy clipboard is retried on the next native tick; it is
                    // never marked as seen before a successful, consistent read.
                    if let Ok(Ok(text)) = tokio::task::spawn_blocking(clipboard::read_text).await {
                        if clipboard::sequence() == sequence {
                            seen = Some(sequence);
                            if let Some(source) = text.and_then(|text| sites::recognize(&text).ok())
                            {
                                if poll_state.clipboard.detect(source, sequence) {
                                    emit(&poll_app, &poll_state);
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    });
    let mut changes = state.clipboard.changed.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            if let Some(capture) = state.clipboard.claim(&mut changes) {
                emit(&app, &state);
                let result = tokio::select! {
                    _ = changes.changed() => continue,
                    result = tokio::time::timeout(Duration::from_secs(45), crate::inspect_source(&app, &state, &capture.source)) => result.unwrap_or_else(|_| Err("后台解析超时，请在软件内重试。".into())),
                };
                if state.clipboard.finish(capture.id, result) {
                    emit(&app, &state);
                }
            } else if changes.changed().await.is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: &str) -> sites::Source {
        sites::recognize(id).unwrap()
    }
    #[test]
    fn a_new_copy_replaces_pending_work_and_stale_results_cannot_win() {
        let monitor = Monitor::default();
        let mut changes = monitor.changed.subscribe();
        monitor.configure(true, true);
        monitor.detect(source("BV1xx411c7mD"), 1);
        let first = monitor.claim(&mut changes).unwrap();
        assert!(!changes.has_changed().unwrap());
        assert!(!monitor.detect(first.source.clone(), 1));
        monitor.detect(source("BV17x411w7KC"), 2);
        assert!(changes.has_changed().unwrap());
        assert!(!monitor.finish(first.id, Err("stale".into())));
        assert!(!monitor.acknowledge(first.id));
        let second = monitor.claim(&mut changes).unwrap();
        assert_ne!(first.id, second.id);
        assert!(monitor.finish(second.id, Err("temporary".into())));
        assert_eq!(monitor.snapshot().unwrap().phase, Phase::Error);
        assert!(monitor.detect(second.source, 3)); // Explicitly copying again retries a failed link.
        assert!(monitor.claim(&mut changes).is_some());
    }
    #[test]
    fn opt_out_cancels_work_clears_results_and_does_not_accept_new_sources() {
        let monitor = Monitor::default();
        let mut changes = monitor.changed.subscribe();
        monitor.configure(true, true);
        monitor.detect(source("BV1xx411c7mD"), 1);
        let capture = monitor.claim(&mut changes).unwrap();
        monitor.configure(false, true);
        assert!(changes.has_changed().unwrap());
        assert!(monitor.snapshot().is_none());
        assert!(!monitor.finish(capture.id, Err("late".into())));
        assert!(!monitor.detect(capture.source, 4));
        monitor.configure(true, false);
        monitor.detect(source("BV17x411w7KC"), 2);
        assert!(monitor.claim(&mut changes).is_none());
        assert_eq!(monitor.snapshot().unwrap().phase, Phase::Detected);
        monitor.configure(true, true);
        assert!(monitor.claim(&mut changes).is_some());
    }
    #[test]
    fn stopping_auto_parse_keeps_a_suggestion_and_old_completion_is_rejected() {
        let monitor = Monitor::default();
        let mut changes = monitor.changed.subscribe();
        monitor.configure(true, true);
        monitor.detect(source("BV1xx411c7mD"), 1);
        let capture = monitor.claim(&mut changes).unwrap();
        monitor.configure(true, false);
        assert_eq!(monitor.snapshot().unwrap().phase, Phase::Detected);
        assert!(!monitor.finish(capture.id, Err("late".into())));
        assert!(monitor.acknowledge(capture.id));
        assert!(monitor.snapshot().is_none());
    }
    #[test]
    fn explicit_parsing_suppresses_only_that_clipboard_generation() {
        let monitor = Monitor::default();
        monitor.configure(true, true);
        let input = source("BV1xx411c7mD");
        monitor.manual(42);
        assert!(!monitor.detect(input.clone(), 42));
        assert!(monitor.detect(input.clone(), 43));
        let revision = monitor.status().revision;
        assert!(monitor.manual(43));
        assert!(monitor.status().revision > revision);
        assert!(monitor.snapshot().is_none());
        assert!(!monitor.detect(input.clone(), 43));
        assert!(monitor.detect(input, 44));
    }
    #[test]
    fn manually_entered_source_cancels_an_older_clipboard_source() {
        let monitor = Monitor::default();
        let mut changes = monitor.changed.subscribe();
        monitor.configure(true, true);
        monitor.detect(source("BV1xx411c7mD"), 1);
        let old = monitor.claim(&mut changes).unwrap();
        assert!(monitor.manual(1));
        assert!(!monitor.finish(old.id, Err("late old result".into())));
        assert!(!monitor.detect(source("BV17x411w7KC"), 1));
        assert!(monitor.detect(source("BV17x411w7KC"), 2));
    }
}
