use serde::Serialize;
use std::sync::Arc;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "dynamic-island";
const DEFAULT_WIDTH: u32 = 710;
const DEFAULT_HEIGHT: u32 = 42;
const EXIT_DURATION: std::time::Duration = std::time::Duration::from_millis(180);

pub fn create(app: &AppHandle, state: Arc<crate::AppState>) -> tauri::Result<()> {
    let island =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=island".into()))
            .title("SnowDown 灵动岛")
            .inner_size(DEFAULT_WIDTH.into(), DEFAULT_HEIGHT.into())
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .focused(false)
            .build()?;
    // Give the window an on-screen position even if its WebView has not finished loading.
    if let Err(error) = resize_island(island.clone(), DEFAULT_WIDTH, DEFAULT_HEIGHT, None, false) {
        eprintln!("灵动岛初始位置调整失败：{error}");
    }
    let main = app.get_webview_window("main").expect("main window");
    std::thread::spawn(move || {
        let mut shown = false;
        let mut hide_at = None;
        #[cfg(windows)]
        let mut sampler = None;
        let mut next_sample = std::time::Instant::now();
        loop {
            let poll_ms = if hide_at.is_some() { 30 } else { 150 };
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
            let minimized =
                main.is_visible().unwrap_or(false) && main.is_minimized().unwrap_or(false);
            if !minimized || !state.settings.lock().unwrap().dynamic_island_enabled {
                if shown {
                    let deadline = hide_at.get_or_insert_with(|| {
                        let _ = island.emit("snowdown://island-hiding", ());
                        std::time::Instant::now() + EXIT_DURATION
                    });
                    if std::time::Instant::now() >= *deadline && island.hide().is_ok() {
                        shown = false;
                        hide_at = None;
                    }
                    #[cfg(windows)]
                    {
                        sampler = None;
                    }
                }
                continue;
            }
            if !shown || hide_at.take().is_some() {
                if island.show().is_err() {
                    continue;
                }
                let _ = island.emit("snowdown://island-shown", ());
                shown = true;
                next_sample = std::time::Instant::now();
            }
            if std::time::Instant::now() >= next_sample {
                #[cfg(windows)]
                {
                    let usage = sampler.get_or_insert_with(metrics::Sampler::new).sample();
                    let _ = island.emit("snowdown://island-usage", usage);
                }
                next_sample = std::time::Instant::now() + std::time::Duration::from_secs(1);
            }
        }
    });
    Ok(())
}

#[derive(Serialize)]
struct SettingsRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Serialize)]
pub struct SettingsLayout {
    source: SettingsRect,
    studio: SettingsRect,
}

#[tauri::command]
pub fn layout_island_settings(
    window: WebviewWindow,
    flight: bool,
) -> Result<SettingsLayout, String> {
    if window.label() != LABEL {
        return Err("仅限灵动岛窗口。".into());
    }
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or("无法确定屏幕。")?;
    let work = monitor.work_area();
    let scale = monitor.scale_factor();
    let source = window.outer_position().map_err(|error| error.to_string())?;
    let source_size = window.inner_size().map_err(|error| error.to_string())?;
    let width = ((600. * scale).round() as u32).min(work.size.width);
    let height = ((560. * scale).round() as u32).min(work.size.height);
    let x = work.position.x + ((work.size.width - width) / 2) as i32;
    let y = work.position.y + ((work.size.height - height) / 2) as i32;
    let left = if flight { x.min(source.x) } else { x };
    let top = if flight { y.min(source.y) } else { y };
    let right = if flight {
        (x + width as i32).max(source.x + source_size.width as i32)
    } else {
        x + width as i32
    };
    let bottom = if flight {
        (y + height as i32).max(source.y + source_size.height as i32)
    } else {
        y + height as i32
    };
    window
        .set_position(PhysicalPosition::new(left, top))
        .map_err(|error| error.to_string())?;
    window
        .set_size(PhysicalSize::new(
            (right - left) as u32,
            (bottom - top) as u32,
        ))
        .map_err(|error| error.to_string())?;
    let local_rect = |x: i32, y: i32, width: u32, height: u32| SettingsRect {
        x: (x - left) as f64 / scale,
        y: (y - top) as f64 / scale,
        width: width as f64 / scale,
        height: height as f64 / scale,
    };
    Ok(SettingsLayout {
        source: local_rect(source.x, source.y, source_size.width, source_size.height),
        studio: local_rect(x, y, width, height),
    })
}

#[derive(Serialize)]
pub struct Placement {
    x: i32,
    y: i32,
    docked: bool,
}

#[tauri::command]
pub fn resize_island(
    window: WebviewWindow,
    width: u32,
    height: u32,
    position: Option<PhysicalPosition<i32>>,
    snap: bool,
) -> Result<Placement, String> {
    if window.label() != LABEL {
        return Err("仅限灵动岛窗口。".into());
    }
    let monitor = position
        .and_then(|point| {
            window
                .monitor_from_point(point.x as f64, point.y as f64)
                .ok()
                .flatten()
        })
        .or_else(|| {
            let main = window.app_handle().get_webview_window("main")?;
            main.current_monitor().ok().flatten()
        })
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("无法确定屏幕。")?;
    let scale = monitor.scale_factor();
    let width = ((width.clamp(240, 2400) as f64 * scale).round() as u32).min(monitor.size().width);
    let height = ((height.clamp(32, 460) as f64 * scale).round() as u32).min(monitor.size().height);
    let origin = monitor.position();
    let right = origin.x + (monitor.size().width - width) as i32;
    let bottom = origin.y + (monitor.size().height - height) as i32;
    let point = position
        .unwrap_or_else(|| PhysicalPosition::new(origin.x + (right - origin.x) / 2, origin.y));
    let x = point.x.clamp(origin.x, right);
    let mut y = point.y.clamp(origin.y, bottom);
    if snap && (y - origin.y) as f64 <= 20. * scale {
        y = origin.y;
    }
    // Position first so the receiving monitor applies the correct DPI before resizing.
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|error| error.to_string())?;
    Ok(Placement {
        x,
        y,
        docked: y == origin.y,
    })
}

#[tauri::command]
pub fn restore_from_island(window: WebviewWindow) {
    if window.label() == LABEL {
        crate::show_main(window.app_handle());
    }
}

#[cfg(windows)]
mod metrics {
    use super::Serialize;
    use std::{collections::HashMap, mem::size_of, ptr::null};
    use windows_sys::Win32::System::{
        Performance::*,
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
    };

    #[derive(Clone, Serialize)]
    pub struct Usage {
        cpu: Option<u32>,
        gpu: Option<u32>,
        memory: Option<u32>,
    }

    pub struct Sampler {
        query: PDH_HQUERY,
        cpu: PDH_HCOUNTER,
        gpu: PDH_HCOUNTER,
    }

    impl Sampler {
        pub fn new() -> Self {
            let mut sampler = Self {
                query: std::ptr::null_mut(),
                cpu: std::ptr::null_mut(),
                gpu: std::ptr::null_mut(),
            };
            unsafe {
                if PdhOpenQueryW(null(), 0, &mut sampler.query) == 0 {
                    for (path, counter) in [
                        (r"\Processor(_Total)\% Processor Time", &mut sampler.cpu),
                        (r"\GPU Engine(*)\Utilization Percentage", &mut sampler.gpu),
                    ] {
                        let path: Vec<_> = path.encode_utf16().chain([0]).collect();
                        PdhAddEnglishCounterW(sampler.query, path.as_ptr(), 0, counter);
                    }
                }
            }
            sampler
        }

        pub fn sample(&self) -> Usage {
            let mut usage = Usage {
                cpu: None,
                gpu: None,
                memory: None,
            };
            unsafe {
                let mut memory = MEMORYSTATUSEX {
                    dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                    ..Default::default()
                };
                if GlobalMemoryStatusEx(&mut memory) != 0 {
                    usage.memory = Some(memory.dwMemoryLoad);
                }
                if !self.query.is_null() && PdhCollectQueryData(self.query) == 0 {
                    let mut value = PDH_FMT_COUNTERVALUE::default();
                    if !self.cpu.is_null()
                        && PdhGetFormattedCounterValue(
                            self.cpu,
                            PDH_FMT_DOUBLE,
                            std::ptr::null_mut(),
                            &mut value,
                        ) == 0
                        && value.CStatus <= 1
                    {
                        usage.cpu = Some(percent(value.Anonymous.doubleValue));
                    }
                    usage.gpu = self.gpu_usage();
                }
            }
            usage
        }

        fn gpu_usage(&self) -> Option<u32> {
            if self.gpu.is_null() {
                return None;
            }
            let (mut bytes, mut count) = (0, 0);
            unsafe {
                if PdhGetFormattedCounterArrayW(
                    self.gpu,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    std::ptr::null_mut(),
                ) != PDH_MORE_DATA
                    || bytes == 0
                {
                    return None;
                }
                // The API writes the aligned item array and its UTF-16 strings into the same buffer.
                let mut items = vec![
                    PDH_FMT_COUNTERVALUE_ITEM_W::default();
                    (bytes as usize)
                        .div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>())
                ];
                if PdhGetFormattedCounterArrayW(
                    self.gpu,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    items.as_mut_ptr(),
                ) != 0
                {
                    return None;
                }
                let mut engines = HashMap::<Vec<u16>, f64>::new();
                for item in items.iter().take(count as usize) {
                    if item.FmtValue.CStatus > 1 || item.szName.is_null() {
                        continue;
                    }
                    let len = (0..)
                        .take_while(|&index| *item.szName.add(index) != 0)
                        .count();
                    let name = std::slice::from_raw_parts(item.szName, len);
                    // Ignore the PID prefix and sum usage per physical engine across processes.
                    let marker = [
                        b'_' as u16,
                        b'l' as u16,
                        b'u' as u16,
                        b'i' as u16,
                        b'd' as u16,
                        b'_' as u16,
                    ];
                    if let Some(start) = name.windows(marker.len()).position(|part| part == marker)
                    {
                        let engine: Vec<_> = name[start..]
                            .iter()
                            .copied()
                            .take_while(|&c| c != b'#' as u16)
                            .collect();
                        *engines.entry(engine).or_default() += item.FmtValue.Anonymous.doubleValue;
                    }
                }
                engines.into_values().reduce(f64::max).map(percent)
            }
        }
    }

    impl Drop for Sampler {
        fn drop(&mut self) {
            if !self.query.is_null() {
                unsafe {
                    PdhCloseQuery(self.query);
                }
            }
        }
    }

    fn percent(value: f64) -> u32 {
        value.round().clamp(0., 100.) as u32
    }
}
