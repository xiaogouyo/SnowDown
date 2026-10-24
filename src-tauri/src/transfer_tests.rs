use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct Fixture {
    url: String,
    body: Arc<Mutex<Vec<u8>>>,
    version: Arc<AtomicUsize>,
    bytes: Arc<AtomicU64>,
    ranges: Arc<Mutex<Vec<(u64, u64)>>>,
    peak: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(size: usize, mode: usize, delay_ms: u64) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/resource", listener.local_addr().unwrap());
        let body = Arc::new(Mutex::new(
            (0..size).map(|n| (n % 251) as u8).collect::<Vec<_>>(),
        ));
        let version = Arc::new(AtomicUsize::new(1));
        let bytes = Arc::new(AtomicU64::new(0));
        let ranges = Arc::new(Mutex::new(vec![]));
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let truncate_once = Arc::new(AtomicBool::new(mode == 5));
        let (data, rev, count, requests, active, maximum, truncate) = (
            body.clone(),
            version.clone(),
            bytes.clone(),
            ranges.clone(),
            current.clone(),
            peak.clone(),
            truncate_once.clone(),
        );
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let (data, rev, count, requests, active, maximum, truncate) = (
                    data.clone(),
                    rev.clone(),
                    count.clone(),
                    requests.clone(),
                    active.clone(),
                    maximum.clone(),
                    truncate.clone(),
                );
                tokio::spawn(async move {
                    let mut input = vec![];
                    let mut buffer = [0u8; 2048];
                    while !input.windows(4).any(|value| value == b"\r\n\r\n") {
                        let Ok(n) = socket.read(&mut buffer).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        input.extend_from_slice(&buffer[..n]);
                        if input.len() > 16384 {
                            return;
                        }
                    }
                    let input = String::from_utf8_lossy(&input).to_lowercase();
                    let mut range = input
                        .lines()
                        .find_map(|line| line.strip_prefix("range: bytes="))
                        .and_then(|value| value.split_once('-'))
                        .and_then(|(a, b)| {
                            Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
                        });
                    let body = data.lock().unwrap().clone();
                    let total = body.len();
                    let tag = format!("\"v{}\"", rev.load(Ordering::Relaxed));
                    if input
                        .lines()
                        .find_map(|line| line.strip_prefix("if-range: "))
                        .is_some_and(|value| value != tag)
                    {
                        range = None;
                    }
                    if mode == 1 || (mode == 3 && range != Some((0, 0))) {
                        range = None;
                    }
                    let (start, end) = range.unwrap_or((0, total - 1));
                    if start > end || end >= total {
                        let _=socket.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                        return;
                    }
                    let mut headers = if range.is_some() {
                        format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{end}/{total}\r\n",if mode==2&&range!=Some((0,0)){start+1}else{start})
                    } else {
                        "HTTP/1.1 200 OK\r\n".into()
                    };
                    if mode != 4 {
                        headers.push_str(&format!("ETag: {tag}\r\n"));
                    }
                    headers.push_str(&format!(
                        "Content-Length: {}\r\nConnection: close\r\n\r\n",
                        end - start + 1
                    ));
                    if socket.write_all(headers.as_bytes()).await.is_err() {
                        return;
                    }
                    let probe = range == Some((0, 0));
                    if !probe {
                        requests.lock().unwrap().push((start as u64, end as u64));
                    }
                    let now = active.fetch_add(1, Ordering::Relaxed) + 1;
                    maximum.fetch_max(now, Ordering::Relaxed);
                    struct Guard(Arc<AtomicUsize>);
                    impl Drop for Guard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    let _guard = Guard(active);
                    let until = if !probe && truncate.swap(false, Ordering::Relaxed) {
                        start + (end - start + 1) / 2
                    } else {
                        end + 1
                    };
                    for chunk in body[start..until].chunks(32 * 1024) {
                        if !probe && delay_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        }
                        if socket.write_all(chunk).await.is_err() {
                            break;
                        }
                        if !probe {
                            count.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        Self {
            url,
            body,
            version,
            bytes,
            ranges,
            peak,
            task,
        }
    }
    fn client() -> Client {
        Client::builder().http1_only().build().unwrap()
    }
    fn content(&self) -> Vec<u8> {
        self.body.lock().unwrap().clone()
    }
}
fn path() -> PathBuf {
    std::env::temp_dir().join(format!("snowdown-ranges-{}.part", uuid::Uuid::new_v4()))
}
fn options(identity: &str, connections: usize, rate: u64) -> Options<'_> {
    Options {
        identity,
        expected_size: None,
        sha256: None,
        connections,
        max_bytes_per_second: rate,
    }
}
async fn finish(path: &Path) {
    discard(path).await;
    let _ = fs::remove_file(path.with_extension("etag")).await;
}

#[test]
fn strict_ranges_and_bounded_layout() {
    assert_eq!(content_range("bytes 5-9/10"), Some((5, 9, 10)));
    for bad in ["bytes 9-5/10", "bytes 0-10/10", "bytes */10", "bytes 0-1/*"] {
        assert!(content_range(bad).is_none());
    }
    for size in [1, 1024 * 1024, 33 * 1024 * 1024, MAX_OBJECT] {
        let (_, blocks) = layout(Some(size), true);
        assert!(blocks.len() <= 4096);
        assert_eq!(blocks[0].start, 0);
        assert_eq!(blocks.last().unwrap().end, Some(size - 1));
        assert!(blocks
            .windows(2)
            .all(|parts| parts[0].end.unwrap() + 1 == parts[1].start));
    }
}

#[tokio::test]
async fn parallel_ranges_produce_the_exact_file_without_exposing_preallocation() {
    let server = Fixture::new(2 * 1024 * 1024, 0, 35).await;
    let target = path();
    let mut peak = 0;
    let outcome = download(
        &Fixture::client(),
        &server.url,
        &target,
        options("parallel", 8, 0),
        &AtomicBool::new(false),
        |stats| peak = peak.max(stats.active_connections),
    )
    .await
    .unwrap();
    assert!(outcome.ranged);
    assert!(outcome.peak_connections >= 2);
    assert!(server.peak.load(Ordering::Relaxed) >= 2);
    assert!(peak > 0);
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    assert_eq!(outcome.bytes, 2 * 1024 * 1024);
    finish(&target).await;
}

async fn paused(server: &Fixture, target: &Path, identity: &str) -> u64 {
    let cancel = AtomicBool::new(false);
    let result = download(
        &Fixture::client(),
        &server.url,
        target,
        options(identity, 8, 1024 * 1024),
        &cancel,
        |stats| {
            if stats.downloaded >= 512 * 1024 {
                cancel.store(true, Ordering::Relaxed)
            }
        },
    )
    .await;
    assert_eq!(result.unwrap_err(), CANCELLED);
    let saved = saved_bytes(target, identity).await;
    assert!(saved > 0 && saved < server.content().len() as u64);
    assert!(fs::metadata(target).await.is_err());
    saved
}

#[tokio::test]
async fn pause_new_client_and_changed_connection_count_resume_verified_bytes() {
    let server = Fixture::new(4 * 1024 * 1024, 0, 1).await;
    let target = path();
    let saved = paused(&server, &target, "restart").await;
    let before = server.bytes.load(Ordering::Relaxed);
    let outcome = download(
        &Fixture::client(),
        &server.url,
        &target,
        options("restart", 4, 0),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(outcome.resumed_bytes, saved);
    assert!(outcome.peak_connections <= 4);
    assert!(server.bytes.load(Ordering::Relaxed) - before <= 4 * 1024 * 1024 - saved);
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    finish(&target).await;
}

#[tokio::test]
async fn corrupt_committed_bytes_are_redownloaded_instead_of_trusted() {
    let server = Fixture::new(4 * 1024 * 1024, 0, 1).await;
    let target = path();
    let saved = paused(&server, &target, "corruption").await;
    let directory = cache_directory(&target);
    let manifest: Manifest = json_read(&directory.join("manifest.json")).await.unwrap();
    let (_, blocks) = layout(manifest.remote.total, true);
    let mut edited = false;
    for block in blocks {
        let receipt: Receipt = json_read(&receipt_path(&directory, block.index))
            .await
            .unwrap_or_default();
        if receipt.received > 0 {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .open(directory.join("data.part"))
                .await
                .unwrap();
            file.seek(std::io::SeekFrom::Start(block.start))
                .await
                .unwrap();
            file.write_all(&[255]).await.unwrap();
            file.sync_all().await.unwrap();
            edited = true;
            break;
        }
    }
    assert!(edited);
    let outcome = download(
        &Fixture::client(),
        &server.url,
        &target,
        options("corruption", 8, 0),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert!(outcome.resumed_bytes < saved);
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    finish(&target).await;
}

#[tokio::test]
async fn changed_etag_discards_old_segments_even_when_length_matches() {
    let server = Fixture::new(4 * 1024 * 1024, 0, 1).await;
    let target = path();
    paused(&server, &target, "changed").await;
    server.version.store(2, Ordering::Relaxed);
    server.body.lock().unwrap().fill(9);
    let outcome = download(
        &Fixture::client(),
        &server.url,
        &target,
        options("changed", 8, 0),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(outcome.resumed_bytes, 0);
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    finish(&target).await;
}

#[tokio::test]
async fn ignored_or_wrong_range_falls_back_to_a_fresh_single_response() {
    for mode in [1, 2, 3, 4] {
        let server = Fixture::new(2 * 1024 * 1024, mode, 1).await;
        let target = path();
        let outcome = download(
            &Fixture::client(),
            &server.url,
            &target,
            options("fallback", 8, 0),
            &AtomicBool::new(false),
            |_| {},
        )
        .await
        .unwrap();
        assert!(!outcome.ranged);
        assert_eq!(outcome.peak_connections, 1);
        assert_eq!(fs::read(&target).await.unwrap(), server.content());
        finish(&target).await;
    }
}

#[tokio::test]
async fn interrupted_range_retries_only_the_remaining_part() {
    let server = Fixture::new(2 * 1024 * 1024, 5, 1).await;
    let target = path();
    download(
        &Fixture::client(),
        &server.url,
        &target,
        options("retry", 8, 0),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    assert!(server
        .ranges
        .lock()
        .unwrap()
        .iter()
        .any(|(start, _)| start % MIN_BLOCK != 0));
    finish(&target).await;
}

#[tokio::test]
async fn sha256_rejects_bad_updates_and_drops_the_poisoned_cache() {
    let server = Fixture::new(1024 * 1024, 0, 0).await;
    let target = path();
    let bad = "00".repeat(32);
    let mut request = options("update", 8, 0);
    request.sha256 = Some(&bad);
    assert!(download(
        &Fixture::client(),
        &server.url,
        &target,
        request,
        &AtomicBool::new(false),
        |_| {}
    )
    .await
    .unwrap_err()
    .contains("校验失败"));
    assert!(!cache_directory(&target).exists());
    assert!(!target.exists());
}

#[tokio::test]
async fn signed_url_or_mirror_change_can_resume_with_the_same_official_digest() {
    let first = Fixture::new(4 * 1024 * 1024, 0, 1).await;
    let second = Fixture::new(4 * 1024 * 1024, 0, 1).await;
    second.version.store(7, Ordering::Relaxed);
    let target = path();
    let hash = digest(&first.content());
    let cancel = AtomicBool::new(false);
    let mut request = options("official-update", 8, 1024 * 1024);
    request.sha256 = Some(&hash);
    assert_eq!(
        download(
            &Fixture::client(),
            &first.url,
            &target,
            request,
            &cancel,
            |stats| {
                if stats.downloaded >= 512 * 1024 {
                    cancel.store(true, Ordering::Relaxed)
                }
            }
        )
        .await
        .unwrap_err(),
        CANCELLED
    );
    let saved = saved_bytes(&target, "official-update").await;
    assert!(saved > 0);
    let mut request = options("official-update", 8, 0);
    request.sha256 = Some(&hash);
    let outcome = download(
        &Fixture::client(),
        &second.url,
        &target,
        request,
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(outcome.resumed_bytes, saved);
    assert_eq!(digest(&fs::read(&target).await.unwrap()), hash);
    finish(&target).await;
}

#[tokio::test]
async fn legacy_strong_etag_partial_files_are_migrated() {
    let server = Fixture::new(2 * 1024 * 1024, 0, 1).await;
    let target = path();
    let prefix = 600 * 1024;
    fs::write(&target, &server.content()[..prefix])
        .await
        .unwrap();
    fs::write(target.with_extension("etag"), "\"v1\"")
        .await
        .unwrap();
    let outcome = download(
        &Fixture::client(),
        &server.url,
        &target,
        options("legacy", 8, 0),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(outcome.resumed_bytes, prefix as u64);
    assert_eq!(fs::read(&target).await.unwrap(), server.content());
    finish(&target).await;
}

#[tokio::test]
async fn rate_limit_is_shared_by_all_connections() {
    let server = Fixture::new(2 * 1024 * 1024, 0, 0).await;
    let target = path();
    let began = Instant::now();
    download(
        &Fixture::client(),
        &server.url,
        &target,
        options("rate", 8, 1024 * 1024),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .unwrap();
    assert!(began.elapsed() >= Duration::from_millis(1700));
    finish(&target).await;
}

#[tokio::test]
#[ignore = "Controlled per-connection throttling benchmark; prints actual durations and verifies output hashes"]
async fn controlled_parallel_speed_comparison() {
    let mut durations = vec![];
    for connections in [1, 8] {
        let server = Fixture::new(8 * 1024 * 1024, 0, 8).await;
        let target = path();
        let began = Instant::now();
        let outcome = download(
            &Fixture::client(),
            &server.url,
            &target,
            options("benchmark", connections, 0),
            &AtomicBool::new(false),
            |_| {},
        )
        .await
        .unwrap();
        let elapsed = began.elapsed();
        assert_eq!(
            digest(&fs::read(&target).await.unwrap()),
            digest(&server.content())
        );
        eprintln!(
            "connections={connections}, peak={}, elapsed_ms={}",
            outcome.peak_connections,
            elapsed.as_millis()
        );
        durations.push(elapsed.as_secs_f64());
        finish(&target).await;
    }
    eprintln!("measured_speedup={:.2}x", durations[0] / durations[1]);
    assert!(durations[1] < durations[0] * 0.75);
}
