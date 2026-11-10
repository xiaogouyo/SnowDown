use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

pub const LABEL: &str = "dynamic-island";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let island =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=island".into()))
            .title("SnowDown 灵动岛")
            .inner_size(710., 48.)
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
        let mut monitor = None;
        let mut shown = false;
        #[cfg(windows)]
        let mut sampler = None;
        let mut next_sample = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let minimized =
                main.is_visible().unwrap_or(false) && main.is_minimized().unwrap_or(false);
            if !minimized {
                if shown {
                    let _ = island.hide();
                    shown = false;
                    #[cfg(windows)]
                    {
                        sampler = None;
                    }
                }
                if let Ok(Some(current)) = main.current_monitor() {
                    monitor = Some(current);
                }
                continue;
            }
            if !shown {
                if let Some(screen) = monitor
                    .clone()
                    .or_else(|| main.primary_monitor().ok().flatten())
                {
                    let _ = place(&island, &screen, None);
                }
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

fn place(island: &WebviewWindow, screen: &Monitor, size: Option<(u32, u32)>) -> tauri::Result<()> {
    let scale = screen.scale_factor();
    let size = if let Some((width, height)) = size {
        PhysicalSize::new(
            (width as f64 * scale).round() as u32,
            (height as f64 * scale).round() as u32,
        )
    } else {
        island.inner_size()?
    };
    let width = size.width.min(screen.size().width);
    let height = size.height.min(screen.size().height);
    island.set_size(PhysicalSize::new(width, height))?;
    island.set_position(PhysicalPosition::new(
        screen.position().x + ((screen.size().width - width) / 2) as i32,
        screen.position().y,
    ))?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, RGN_OR,
        };
        let hwnd = island.hwnd()?.0;
        let radius = (16. * scale).round() as i32;
        // Fill the upper corners of the rounded region so it meets the screen edge.
        unsafe {
            let region =
                CreateRoundRectRgn(0, 0, width as i32, height as i32, radius * 2, radius * 2);
            let top = CreateRectRgn(0, 0, width as i32, radius);
            if !region.is_null() && !top.is_null() {
                CombineRgn(region, region, top, RGN_OR);
            }
            if !top.is_null() {
                DeleteObject(top);
            }
            if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
                DeleteObject(region);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn resize_island(window: WebviewWindow, width: u32, height: u32) -> Result<(), String> {
    if window.label() != LABEL {
        return Err("仅限灵动岛窗口。".into());
    }
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("无法确定屏幕。")?;
    place(
        &window,
        &monitor,
        Some((width.clamp(180, 2400), height.clamp(26, 600))),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_island_frost(window: WebviewWindow, enabled: bool) -> Result<(), String> {
    use tauri::utils::{
        config::{Color, WindowEffectsConfig},
        WindowEffect,
    };
    if window.label() != LABEL {
        return Err("仅限灵动岛窗口。".into());
    }
    let effects = enabled.then(|| WindowEffectsConfig {
        effects: vec![WindowEffect::Acrylic],
        color: Some(Color(18, 25, 40, 24)),
        ..Default::default()
    });
    window
        .set_effects(effects)
        .map_err(|error| error.to_string())
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
