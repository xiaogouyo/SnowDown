//! Authenticated, per-process egress for the compatibility extractor. Resolve
//! public destinations before connecting. Direct connections pin those exact
//! addresses; a configured HTTP proxy retains its own domain/DNS routing.
//! It is not a browser/application API.
use base64::Engine;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

pub struct Guard {
    pub url: String,
    task: JoinHandle<()>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn upstream() -> Option<url::Url> {
    for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(value) = std::env::var(key) {
            if let Ok(url) = url::Url::parse(&value) {
                if url.scheme() == "http" && url.host_str().is_some() {
                    return Some(url);
                }
            }
        }
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Registry::*;
        let key = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\0"
            .encode_utf16()
            .collect::<Vec<_>>();
        let enabled = "ProxyEnable\0".encode_utf16().collect::<Vec<_>>();
        let mut value = 0u32;
        let mut length = 4u32;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            enabled.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut value as *mut _ as _,
            &mut length,
        ) != 0
            || value != 1
        {
            return None;
        }
        let name = "ProxyServer\0".encode_utf16().collect::<Vec<_>>();
        let mut buffer = vec![0u16; 2048];
        let mut length = (buffer.len() * 2) as u32;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as _,
            &mut length,
        ) == 0
        {
            let value = String::from_utf16_lossy(
                &buffer[..(length as usize / 2).saturating_sub(1).min(buffer.len())],
            );
            let selected = value
                .split(';')
                .find_map(|p| p.strip_prefix("https="))
                .or_else(|| (!value.contains('=')).then_some(value.as_str()));
            if let Some(selected) = selected {
                return url::Url::parse(&format!("http://{selected}")).ok();
            }
        }
    }
    None
}
pub async fn start() -> Result<Guard, String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "无法初始化安全下载连接。")?;
    let port = listener
        .local_addr()
        .map_err(|_| "安全下载端口不可用。")?
        .port();
    let user = uuid::Uuid::new_v4().simple().to_string();
    let pass = uuid::Uuid::new_v4().simple().to_string();
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
    );
    let proxy = upstream();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
            incoming=listener.accept()=>{let Ok((stream,_))=incoming else{break};if clients.len()>=48{drop(stream);continue;}let auth=auth.clone();let proxy=proxy.clone();clients.spawn(async move{if let Err(error)=serve(stream,&auth,proxy.as_ref()).await{if std::env::var_os("SNOWDOWN_TRACE_EGRESS").is_some(){eprintln!("Extractor egress: {error}");}}});}
                _=clients.join_next(),if !clients.is_empty()=>{}
            }
        }
    });
    Ok(Guard {
        url: format!("http://{user}:{pass}@127.0.0.1:{port}"),
        task,
    })
}
async fn header(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    while data.len() < 16 * 1024 {
        data.push(stream.read_u8().await?);
        if data.ends_with(b"\r\n\r\n") {
            return Ok(data);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "header too large",
    ))
}
async fn response(stream: &mut TcpStream, code: &str) -> std::io::Result<()> {
    let challenge = if code.starts_with("407") {
        "Proxy-Authenticate: Basic realm=\"SnowDown extractor\"\r\n"
    } else {
        ""
    };
    stream
        .write_all(
            format!("HTTP/1.1 {code}\r\n{challenge}Connection: close\r\nContent-Length: 0\r\n\r\n")
                .as_bytes(),
        )
        .await
}
async fn connect(
    addresses: &[SocketAddr],
    proxy: Option<&url::Url>,
    host: &str,
    port: u16,
) -> std::io::Result<TcpStream> {
    let mut last = None;
    for address in addresses.iter().take(4) {
        let attempt = async {
            if let Some(proxy) = proxy {
                let mut stream = TcpStream::connect((
                    proxy.host_str().unwrap(),
                    proxy.port_or_known_default().unwrap_or(80),
                ))
                .await?;
                let auth = if !proxy.username().is_empty() {
                    format!(
                        "Proxy-Authorization: Basic {}\r\n",
                        base64::engine::general_purpose::STANDARD.encode(format!(
                            "{}:{}",
                            proxy.username(),
                            proxy.password().unwrap_or("")
                        ))
                    )
                } else {
                    String::new()
                };
                let target = if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                };
                stream
                    .write_all(
                        format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n{auth}\r\n")
                            .as_bytes(),
                    )
                    .await?;
                let head = header(&mut stream).await?;
                let line = String::from_utf8_lossy(&head);
                if !line
                    .lines()
                    .next()
                    .is_some_and(|l| l.split_whitespace().nth(1) == Some("200"))
                {
                    return Err(std::io::Error::other("upstream proxy rejected connection"));
                }
                Ok(stream)
            } else {
                TcpStream::connect(address).await
            }
        };
        match tokio::time::timeout(Duration::from_secs(8), attempt).await {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(error)) => last = Some(error),
            Err(_) => {
                last = Some(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "public connection timed out",
                ))
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no public destination")))
}
async fn serve(mut client: TcpStream, auth: &str, proxy: Option<&url::Url>) -> std::io::Result<()> {
    let head = tokio::time::timeout(Duration::from_secs(10), header(&mut client))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "proxy header timed out")
        })??;
    let text =
        String::from_utf8(head).map_err(|_| std::io::Error::other("invalid proxy header"))?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let words = first.split_whitespace().collect::<Vec<_>>();
    let fields = lines
        .filter_map(|line| line.split_once(':'))
        .collect::<Vec<_>>();
    if !fields
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("proxy-authorization") && v.trim() == auth)
    {
        return response(&mut client, "407 Proxy Authentication Required").await;
    }
    if words.len() != 3 || !["CONNECT", "GET", "HEAD", "POST"].contains(&words[0]) {
        return response(&mut client, "400 Bad Request").await;
    }
    let tunnel = words[0] == "CONNECT";
    let raw = if tunnel {
        format!("https://{}", words[1])
    } else {
        words[1].to_string()
    };
    let Ok(url) = url::Url::parse(&raw) else {
        return response(&mut client, "400 Bad Request").await;
    };
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(if tunnel { 443 } else { 80 })
        || !tunnel && url.scheme() != "http"
    {
        return response(&mut client, "403 Forbidden").await;
    }
    let Some(host) = url.host_str() else {
        return response(&mut client, "403 Forbidden").await;
    };
    let port = url.port_or_known_default().unwrap();
    let addresses = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::net::lookup_host((host.trim_matches(['[', ']']), port)),
    )
    .await
    .map_err(|_| std::io::Error::other("DNS timed out"))??
    .collect::<Vec<_>>();
    if std::env::var_os("SNOWDOWN_TRACE_EGRESS").is_some() {
        eprintln!(
            "Extractor destination: {host}:{port}, addresses={}, upstream={}",
            addresses.len(),
            proxy.is_some()
        );
    }
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|a| !crate::resource_download::public_address(a.ip()))
    {
        return response(&mut client, "403 Forbidden").await;
    }
    let mut remote = connect(&addresses, proxy, host, port).await?;
    if tunnel {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
    } else {
        let path = if let Some(query) = url.query() {
            format!("{}?{query}", url.path())
        } else {
            url.path().into()
        };
        let mut header = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            words[0], path, host
        );
        for (key, value) in fields {
            if ![
                "host",
                "connection",
                "proxy-connection",
                "proxy-authorization",
            ]
            .iter()
            .any(|k| key.eq_ignore_ascii_case(k))
            {
                header.push_str(&format!("{key}:{value}\r\n"));
            }
        }
        header.push_str("\r\n");
        remote.write_all(header.as_bytes()).await?;
    }
    tokio::io::copy_bidirectional(&mut client, &mut remote).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Checks a public HTTPS endpoint through the actual authenticated egress guard"]
    async fn live_public_https_guard() {
        let guard = start().await.unwrap();
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(&guard.url).unwrap())
            .timeout(Duration::from_secs(40))
            .build()
            .unwrap();
        let response = client.get("https://www.youtube.com/").send().await;
        assert!(response.is_ok(), "{:?}", response.err());
        assert!(response.unwrap().status().is_success());
    }
    #[tokio::test]
    async fn rejects_unauthenticated_and_private_destinations() {
        let guard = start().await.unwrap();
        let u = url::Url::parse(&guard.url).unwrap();
        let auth = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            u.username(),
            u.password().unwrap()
        ));
        for (target, credentials, code) in [
            ("example.com:443", "", "407"),
            ("127.0.0.1:443", auth.as_str(), "403"),
            ("192.168.0.1:443", auth.as_str(), "403"),
            ("example.com:22", auth.as_str(), "403"),
        ] {
            let mut client = TcpStream::connect(("127.0.0.1", u.port().unwrap()))
                .await
                .unwrap();
            client.write_all(format!("CONNECT {target} HTTP/1.1\r\nProxy-Authorization: Basic {credentials}\r\n\r\n").as_bytes()).await.unwrap();
            let reply = header(&mut client).await.unwrap();
            assert!(String::from_utf8_lossy(&reply).starts_with(&format!("HTTP/1.1 {code}")));
        }
        let port = u.port().unwrap();
        drop(guard);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }
}
