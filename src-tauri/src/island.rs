use serde::Serialize;
use std::sync::Arc;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "dynamic-island";

pub fn create(app: &AppHandle, state: Arc<crate::AppState>) -> tauri::Result<()> {
    let island =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=island".into()))
            .title("SnowDown 灵动岛")
            .inner_size(710., 42.)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .focused(false)
            .build()?;
    let main = app.get_webview_window("main").expect("main window");
    std::thread::spawn(move || {
        let mut shown = false;
        #[cfg(windows)]
        let mut sampler = None;
        let mut next_sample = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let minimized =
                main.is_visible().unwrap_or(false) && main.is_minimized().unwrap_or(false);
            if !minimized || !state.settings.lock().unwrap().dynamic_island_enabled {
                if shown {
                    let _ = island.hide();
                    shown = false;
                    #[cfg(windows)]
                    {
                        sampler = None;
                    }
                }
                continue;
            }
            if !shown {
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
