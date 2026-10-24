use crate::{model::Platform, sites, AppState};
use std::sync::Arc;
use tauri::{Manager, WebviewWindow};
use url::Url;

fn detail_source(input: &str) -> Option<sites::Source> {
    let url = Url::parse(input).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !sites::host_matches(url.host_str()?, "douyin.com")
        || url.path() != "/aweme/v1/web/aweme/detail/"
    {
        return None;
    }
    let id = url
        .query_pairs()
        .find(|(key, _)| key == "aweme_id")?
        .1
        .into_owned();
    if !(8..=24).contains(&id.len()) || !id.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(sites::Source {
        platform: Platform::Douyin,
        url: format!("https://www.douyin.com/video/{id}"),
    })
}

#[cfg(windows)]
pub fn attach(window: &WebviewWindow, destination: Url) -> Result<(), String> {
    use webview2_com::{
        CoTaskMemPWSTR,
        Microsoft::Web::WebView2::Win32::{ICoreWebView2_2, ICoreWebView2_8},
        WebResourceResponseReceivedEventHandler, WebResourceResponseViewGetContentCompletedHandler,
    };
    use windows::core::{Interface, PWSTR};
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    window
        .with_webview(move |webview| unsafe {
            let installed = (|| -> windows::core::Result<()> {
                let core = webview.controller().CoreWebView2()?;
                if let Ok(audio) = core.cast::<ICoreWebView2_8>() {
                    audio.SetIsMuted(true)?;
                }
                let events = core.cast::<ICoreWebView2_2>()?;
                let callback_app = app.clone();
                let handler =
                    WebResourceResponseReceivedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let request = args.Request()?;
                        let mut uri = PWSTR::null();
                        request.Uri(&mut uri)?;
                        let uri = CoTaskMemPWSTR::from(uri);
                        let Some(source) = detail_source(&uri.to_string()) else {
                            return Ok(());
                        };
                        let response = args.Response()?;
                        let mut status = 0;
                        response.StatusCode(&mut status)?;
                        if status != 200 {
                            return Ok(());
                        }
                        let body_app = callback_app.clone();
                        response.GetContent(
                            &WebResourceResponseViewGetContentCompletedHandler::create(Box::new(
                                move |result, stream| {
                                    result?;
                                    let Some(stream) = stream else {
                                        return Ok(());
                                    };
                                    let mut bytes = Vec::new();
                                    loop {
                                        let mut buffer = [0u8; 8192];
                                        let mut received = 0;
                                        let result = stream.Read(
                                            buffer.as_mut_ptr().cast(),
                                            buffer.len() as u32,
                                            Some(&mut received),
                                        );
                                        if result.is_err()
                                            || bytes.len() + received as usize > 8 * 1024 * 1024
                                        {
                                            return Ok(());
                                        }
                                        bytes.extend_from_slice(&buffer[..received as usize]);
                                        if received == 0 {
                                            break;
                                        }
                                    }
                                    let state = body_app.state::<Arc<AppState>>();
                                    state.site_sessions.remember_response(&source, &bytes);
                                    Ok(())
                                },
                            )),
                        )?;
                        Ok(())
                    }));
                let mut token = 0;
                events.add_WebResourceResponseReceived(&handler, &mut token)?;
                let target = CoTaskMemPWSTR::from(destination.as_str());
                core.Navigate(*target.as_ref().as_pcwstr())?;
                Ok(())
            })();
            if installed.is_err() {
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.navigate(destination);
                }
            }
        })
        .map_err(|_| "无法连接官方页面的视频响应读取组件。".into())
}

#[cfg(not(windows))]
pub fn attach(window: &WebviewWindow, destination: Url) -> Result<(), String> {
    window
        .navigate(destination)
        .map_err(|_| "无法打开官方页面。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes_only_official_video_detail_routes() {
        assert!(detail_source("https://www-hj.douyin.com/aweme/v1/web/aweme/detail/?aweme_id=1234567890123456789&a_bogus=fake").is_some());
        assert!(detail_source("https://www-hj.douyin.com.evil.test/aweme/v1/web/aweme/detail/?aweme_id=1234567890123456789").is_none());
        assert!(detail_source(
            "https://www.douyin.com/aweme/v1/web/query/account/type/?aweme_id=1234567890123456789"
        )
        .is_none());
    }
}
