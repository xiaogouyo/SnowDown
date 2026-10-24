//! Verified, persistent range downloads shared by media and application updates.
//! The destination only appears after every byte has been committed successfully.
use futures_util::{stream, StreamExt};
use reqwest::{header, Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Semaphore,
};

pub const CANCELLED: &str = "下载已暂停，进度已保存。";
const MIN_BLOCK: u64 = 256 * 1024;
const MAX_BLOCK: u64 = 8 * 1024 * 1024;
const MAX_OBJECT: u64 = 2 * 1024 * 1024 * 1024 * 1024;
const MAX_BLOCKS: u64 = 4096;
static CONNECTIONS: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub struct Options<'a> {
    pub identity: &'a str,
    pub expected_size: Option<u64>,
    pub sha256: Option<&'a str>,
    pub connections: usize,
    pub max_bytes_per_second: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub active_connections: usize,
    pub resumed_bytes: u64,
    pub ranged: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    pub bytes: u64,
    pub resumed_bytes: u64,
    pub peak_connections: usize,
    pub ranged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Remote {
    total: Option<u64>,
    validator: Option<String>,
    ranges: bool,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u8,
    identity: String,
    remote: Remote,
    block_size: u64,
    ranged: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Receipt {
    received: u64,
    sha256: String,
    complete: bool,
}
#[derive(Clone, Copy)]
struct Block {
    index: usize,
    start: u64,
    end: Option<u64>,
}
impl Block {
    fn length(self) -> Option<u64> {
        self.end.map(|end| end - self.start + 1)
    }
}
#[derive(Debug)]
enum Failure {
    Paused,
    Restart,
    Other(String),
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Other(format!("无法保存下载进度：{e}"))
    }
}
fn message(error: Failure) -> String {
    match error {
        Failure::Paused => CANCELLED.into(),
        Failure::Restart => "下载源的资源或分段信息发生变化，请重试。".into(),
        Failure::Other(text) => text,
    }
}
struct Cancel<'a> {
    user: &'a AtomicBool,
    stop: AtomicBool,
}
impl Cancel<'_> {
    fn stopped(&self) -> bool {
        self.user.load(Ordering::Relaxed) || self.stop.load(Ordering::Relaxed)
    }
    async fn wait(&self) {
        while !self.stopped() {
            tokio::time::sleep(Duration::from_millis(40)).await
        }
    }
}
struct Active<'a> {
    current: &'a AtomicUsize,
}
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }
}
struct Limiter {
    rate: u64,
    next: Mutex<Instant>,
}
impl Limiter {
    async fn take(&self, bytes: usize, cancel: &Cancel<'_>) -> Result<(), Failure> {
        if self.rate == 0 {
            return Ok(());
        }
        let time = {
            let mut next = self.next.lock().unwrap();
            let begin = (*next).max(Instant::now());
            *next = begin + Duration::from_secs_f64(bytes as f64 / self.rate as f64);
            begin
        };
        tokio::select! {_ = tokio::time::sleep_until(time.into())=>Ok(()),_ = cancel.wait()=>Err(Failure::Paused)}
    }
}

pub fn cache_directory(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".resume-v1");
    path.with_file_name(name)
}
pub async fn discard(path: &Path) {
    let _ = fs::remove_file(path).await;
    cleanup(path).await;
}
pub async fn cleanup(path: &Path) {
    let directory = cache_directory(path);
    if directory.parent() == path.parent() && directory != path {
        let _ = fs::remove_dir_all(directory).await;
    }
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn receipt_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("block-{index:04}.json"))
}
async fn json_read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).await.ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
async fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), Failure> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await?;
        file.write_all(&serde_json::to_vec(value).unwrap()).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary, path).await?;
        Ok::<(), Failure>(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(temporary).await;
    }
    result
}
pub(crate) fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    if start > end || end >= total {
        return None;
    }
    Some((start, end, total))
}
fn validator(response: &Response) -> Option<String> {
    let value = response
        .headers()
        .get(header::ETAG)
        .and_then(|h| h.to_str().ok());
    if let Some(value) =
        value.filter(|value| value.len() <= 1024 && value.starts_with('"') && value.ends_with('"'))
    {
        return Some(value.into());
    }
    let modified = response
        .headers()
        .get(header::LAST_MODIFIED)?
        .to_str()
        .ok()?;
    let modified_time = httpdate::parse_http_date(modified).ok()?;
    let date =
        httpdate::parse_http_date(response.headers().get(header::DATE)?.to_str().ok()?).ok()?;
    if date.duration_since(modified_time).ok()?.as_secs() >= 60 {
        Some(modified.into())
    } else {
        None
    }
}
fn identity_encoding(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_none_or(|h| h.as_bytes().eq_ignore_ascii_case(b"identity"))
}
async fn probe(
    client: &Client,
    url: &str,
    expected: Option<u64>,
    full: bool,
    cancel: &Cancel<'_>,
) -> Result<Remote, Failure> {
    let _permit = tokio::select! {permit=CONNECTIONS.get_or_init(||Arc::new(Semaphore::new(32))).acquire()=>permit.map_err(|_|Failure::Other("连接池已停止。".into()))?,_ = cancel.wait()=>return Err(Failure::Paused)};
    let request = client.get(url).header(header::ACCEPT_ENCODING, "identity");
    let request = if full {
        request
    } else {
        request.header(header::RANGE, "bytes=0-0")
    };
    let mut response = tokio::select! {response=request.send()=>response.map_err(|_|Failure::Other("无法连接下载源。".into()))?,_ = cancel.wait()=>return Err(Failure::Paused)};
    if !identity_encoding(&response) {
        return Err(Failure::Other(
            "下载源返回了压缩字节范围，无法安全下载。".into(),
        ));
    }
    let mut remote = Remote {
        total: response.content_length(),
        validator: validator(&response),
        ranges: false,
    };
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let (start, end, total) = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(content_range)
            .ok_or(Failure::Restart)?;
        if start != 0 || end != 0 {
            return Err(Failure::Restart);
        }
        remote.total = Some(total);
        remote.ranges = true;
        let mut length = 0;
        loop {
            let chunk = tokio::select! {chunk=response.chunk()=>chunk.map_err(|_|Failure::Other("读取下载源信息失败。".into()))?,_ = cancel.wait()=>return Err(Failure::Paused)};
            let Some(chunk) = chunk else { break };
            length += chunk.len();
            if length > 1 {
                return Err(Failure::Restart);
            }
        }
        if length != 1 {
            return Err(Failure::Restart);
        }
    } else if response.status() != StatusCode::OK {
        return Err(Failure::Other(format!(
            "下载源返回 HTTP {}。",
            response.status().as_u16()
        )));
    }
    if remote.total.is_some_and(|n| n > MAX_OBJECT)
        || expected.zip(remote.total).is_some_and(|(a, b)| a != b)
    {
        return Err(Failure::Other("下载资源的大小与预期不一致。".into()));
    }
    remote.total = remote.total.or(expected);
    Ok(remote)
}
fn layout(total: Option<u64>, ranged: bool) -> (u64, Vec<Block>) {
    let Some(total) = total.filter(|n| *n > 0) else {
        return (
            0,
            vec![Block {
                index: 0,
                start: 0,
                end: None,
            }],
        );
    };
    let block_size = if ranged && total >= 1024 * 1024 {
        (total.div_ceil(32))
            .clamp(MIN_BLOCK, MAX_BLOCK)
            .max(total.div_ceil(MAX_BLOCKS))
    } else {
        total
    };
    let blocks = (0..total.div_ceil(block_size))
        .map(|index| Block {
            index: index as usize,
            start: index * block_size,
            end: Some(((index + 1) * block_size).min(total) - 1),
        })
        .collect();
    (block_size, blocks)
}
fn reusable(old: &Manifest, identity: &str, remote: &Remote, trusted: bool, ranged: bool) -> bool {
    old.version == 1
        && old.identity == identity
        && old.remote.total == remote.total
        && old.ranged == ranged
        && (trusted || (remote.validator.is_some() && old.remote.validator == remote.validator))
}
async fn prefix_hash(path: &Path, start: u64, length: u64) -> Result<Sha256, Failure> {
    let mut file = fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut hash = Sha256::new();
    let mut remaining = length;
    let mut bytes = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let wanted = remaining.min(bytes.len() as u64) as usize;
        let count = file.read(&mut bytes[..wanted]).await?;
        if count == 0 {
            return Err(Failure::Restart);
        }
        hash.update(&bytes[..count]);
        remaining -= count as u64;
    }
    Ok(hash)
}
async fn validated_receipt(path: &Path, directory: &Path, block: Block, limit: u64) -> Receipt {
    let Some(receipt) = json_read::<Receipt>(&receipt_path(directory, block.index)).await else {
        return Receipt::default();
    };
    if receipt.received > limit
        || receipt.sha256.len() != 64
        || (receipt.complete
            && block
                .length()
                .is_some_and(|length| length != receipt.received))
    {
        return Receipt::default();
    }
    if let Ok(hash) = prefix_hash(path, block.start, receipt.received).await {
        if format!("{:x}", hash.finalize()) == receipt.sha256 {
            return receipt;
        }
    }
    Receipt::default()
}
async fn checkpoint(
    file: &mut fs::File,
    directory: &Path,
    block: Block,
    received: u64,
    hash: &Sha256,
    complete: bool,
) -> Result<(), Failure> {
    file.flush().await?;
    file.sync_all().await?;
    atomic_json(
        &receipt_path(directory, block.index),
        &Receipt {
            received,
            sha256: format!("{:x}", hash.clone().finalize()),
            complete,
        },
    )
    .await
}

struct Job<'a> {
    client: &'a Client,
    url: &'a str,
    path: &'a Path,
    directory: &'a Path,
    remote: &'a Remote,
    ranged: bool,
    trusted: bool,
    cancel: &'a Cancel<'a>,
    limiter: &'a Limiter,
    current: &'a AtomicUsize,
    peak: &'a AtomicUsize,
    new_bytes: &'a AtomicU64,
}
async fn block(
    job: &Job<'_>,
    block: Block,
    receipt: Receipt,
    count: &AtomicU64,
) -> Result<(), Failure> {
    if job.cancel.stopped() {
        return Err(Failure::Paused);
    }
    if receipt.complete {
        return Ok(());
    }
    let mut received = receipt.received;
    let mut hash = if received > 0 {
        prefix_hash(job.path, block.start, received).await?
    } else {
        Sha256::new()
    };
    let mut file = fs::OpenOptions::new().write(true).open(job.path).await?;
    for attempt in 0..3 {
        if job.cancel.stopped() {
            checkpoint(&mut file, job.directory, block, received, &hash, false).await?;
            return Err(Failure::Paused);
        }
        let result = async {
            let _permit = tokio::select! {
                permit = CONNECTIONS.get_or_init(|| Arc::new(Semaphore::new(32))).acquire() =>
                    permit.map_err(|_| Failure::Other("连接池已停止。".into()))?,
                _ = job.cancel.wait() => return Err(Failure::Paused),
            };
            let active = job.current.fetch_add(1, Ordering::Relaxed) + 1;
            job.peak.fetch_max(active, Ordering::Relaxed);
            let _active = Active { current: job.current };
            let mut request = job.client.get(job.url).header(header::ACCEPT_ENCODING, "identity");
            if job.ranged {
                let end = block.end.ok_or(Failure::Restart)?;
                request = request.header(header::RANGE, format!("bytes={}-{end}", block.start + received));
                if let Some(validator) = &job.remote.validator {
                    request = request.header(header::IF_RANGE, validator);
                }
            }
            let mut response = tokio::select! {
                response = request.send() => response.map_err(|_| Failure::Other("下载连接中断。".into()))?,
                _ = job.cancel.wait() => return Err(Failure::Paused),
            };
            if !identity_encoding(&response) { return Err(Failure::Restart); }
            if job.ranged {
                if response.status() != StatusCode::PARTIAL_CONTENT {
                    return if response.status() == StatusCode::OK || response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
                        Err(Failure::Restart)
                    } else {
                        Err(Failure::Other(format!("下载源返回 HTTP {}。", response.status().as_u16())))
                    };
                }
                let range = response.headers().get(header::CONTENT_RANGE).and_then(|h| h.to_str().ok()).and_then(content_range).ok_or(Failure::Restart)?;
                if range.0 != block.start + received || Some(range.1) != block.end || Some(range.2) != job.remote.total
                    || response.content_length().is_some_and(|length| length != range.1 - range.0 + 1) {
                    return Err(Failure::Restart);
                }
                if !job.trusted && validator(&response) != job.remote.validator { return Err(Failure::Restart); }
            } else {
                if response.status() != StatusCode::OK {
                    return Err(Failure::Other(format!("下载源返回 HTTP {}。", response.status().as_u16())));
                }
                if received > 0 { return Err(Failure::Restart); }
                if job.remote.total.zip(response.content_length()).is_some_and(|(a,b)| a != b) { return Err(Failure::Restart); }
                if !job.trusted && job.remote.validator.is_some() && validator(&response) != job.remote.validator { return Err(Failure::Restart); }
            }
            file.seek(std::io::SeekFrom::Start(block.start + received)).await?;
            let mut saved = received;
            let mut last = Instant::now();
            loop {
                let chunk = tokio::select! {
                    chunk = response.chunk() => chunk.map_err(|_| Failure::Other("下载中断，分段进度已保存。".into()))?,
                    _ = job.cancel.wait() => return Err(Failure::Paused),
                };
                let Some(chunk) = chunk else { break };
                if block.length().is_some_and(|length| received + chunk.len() as u64 > length)
                    || block.start + received + chunk.len() as u64 > MAX_OBJECT { return Err(Failure::Restart); }
                job.limiter.take(chunk.len(), job.cancel).await?;
                file.write_all(&chunk).await?;
                hash.update(&chunk);
                received += chunk.len() as u64;
                count.store(received, Ordering::Relaxed);
                job.new_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                if received - saved >= 1024 * 1024 || last.elapsed() >= Duration::from_millis(500) {
                    checkpoint(&mut file, job.directory, block, received, &hash, false).await?;
                    saved = received;
                    last = Instant::now();
                }
            }
            if block.length().is_some_and(|length| received != length) {
                return Err(Failure::Other("分段尚未收完，正在重试。".into()));
            }
            checkpoint(&mut file, job.directory, block, received, &hash, true).await?;
            Ok(())
        }.await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                checkpoint(&mut file, job.directory, block, received, &hash, false).await?;
                if matches!(error, Failure::Paused | Failure::Restart)
                    || attempt == 2
                    || !job.ranged
                {
                    return Err(error);
                }
                tokio::select! {_ = tokio::time::sleep(Duration::from_millis(300*(attempt+1)))=>{},_ = job.cancel.wait()=>return Err(Failure::Paused)}
            }
        }
    }
    unreachable!()
}

pub async fn saved_bytes(path: &Path, identity: &str) -> u64 {
    let directory = cache_directory(path);
    let Some(manifest) = json_read::<Manifest>(&directory.join("manifest.json")).await else {
        return 0;
    };
    if manifest.identity != digest(identity.as_bytes()) || manifest.version != 1 {
        return 0;
    }
    let (_, blocks) = layout(manifest.remote.total, manifest.ranged);
    let mut total = 0;
    for block in blocks {
        if let Some(receipt) = json_read::<Receipt>(&receipt_path(&directory, block.index)).await {
            if !block
                .length()
                .is_some_and(|length| receipt.received > length)
            {
                total += receipt.received;
            }
        }
    }
    total
}

pub async fn download(
    client: &Client,
    url: &str,
    path: &Path,
    options: Options<'_>,
    user_cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Outcome, String> {
    let cancel = Cancel {
        user: user_cancel,
        stop: AtomicBool::new(false),
    };
    let identity = digest(options.identity.as_bytes());
    let directory = cache_directory(path);
    let data = directory.join("data.part");
    for fallback in 0..2 {
        cancel.stop.store(false, Ordering::Relaxed);
        let mut remote =
            match probe(client, url, options.expected_size, fallback > 0, &cancel).await {
                Ok(remote) => remote,
                Err(Failure::Restart) if fallback == 0 => continue,
                Err(error) => return Err(message(error)),
            };
        let trusted = options.sha256.is_some();
        let ranged = fallback == 0
            && remote.ranges
            && (remote.validator.is_some() || trusted)
            && remote.total.is_some_and(|size| size > 0);
        if fallback > 0 {
            remote.ranges = false;
            remote.validator = None;
        }
        let (block_size, blocks) = layout(remote.total, ranged);
        let old = json_read::<Manifest>(&directory.join("manifest.json")).await;
        let reuse = old.as_ref().is_some_and(|old| {
            reusable(old, &identity, &remote, trusted, ranged) && old.block_size == block_size
        });
        let legacy = if old.is_none()
            && ranged
            && remote
                .validator
                .as_ref()
                .is_some_and(|value| value.starts_with('"'))
            && fs::read_to_string(path.with_extension("etag"))
                .await
                .ok()
                .as_deref()
                == remote.validator.as_deref()
        {
            fs::metadata(path)
                .await
                .ok()
                .map(|m| m.len())
                .filter(|length| *length > 0 && Some(*length) <= remote.total)
        } else {
            None
        };
        if !reuse && legacy.is_none() {
            discard(path).await;
        }
        fs::create_dir_all(&directory)
            .await
            .map_err(|e| message(e.into()))?;
        if let Some(length) = legacy {
            fs::rename(path, &data)
                .await
                .map_err(|e| message(e.into()))?;
            let file = fs::OpenOptions::new()
                .write(true)
                .open(&data)
                .await
                .map_err(|e| message(e.into()))?;
            file.set_len(remote.total.unwrap())
                .await
                .map_err(|e| message(e.into()))?;
            drop(file);
            for block in &blocks {
                let received = length
                    .saturating_sub(block.start)
                    .min(block.length().unwrap());
                let hash = prefix_hash(&data, block.start, received)
                    .await
                    .map_err(message)?;
                atomic_json(
                    &receipt_path(&directory, block.index),
                    &Receipt {
                        received,
                        sha256: format!("{:x}", hash.finalize()),
                        complete: Some(received) == block.length(),
                    },
                )
                .await
                .map_err(message)?;
            }
        }
        atomic_json(
            &directory.join("manifest.json"),
            &Manifest {
                version: 1,
                identity: identity.clone(),
                remote: remote.clone(),
                block_size,
                ranged,
            },
        )
        .await
        .map_err(message)?;
        let completed_path = reuse && fs::metadata(path).await.is_ok();
        let working = if completed_path { path } else { &data };
        let size = fs::metadata(working).await.ok().map(|m| m.len());
        let valid_file =
            (reuse || legacy.is_some()) && size.is_some() && (!ranged || size == remote.total);
        let mut receipts = Vec::with_capacity(blocks.len());
        for block in &blocks {
            let receipt = if valid_file {
                validated_receipt(
                    working,
                    &directory,
                    *block,
                    block.length().unwrap_or(size.unwrap_or(0)),
                )
                .await
            } else {
                Receipt::default()
            };
            receipts.push(if !ranged && !receipt.complete {
                Receipt::default()
            } else {
                receipt
            });
        }
        if completed_path && receipts.iter().any(|receipt| !receipt.complete) {
            fs::rename(path, &data)
                .await
                .map_err(|e| message(e.into()))?;
        }
        let ready = receipts.iter().all(|receipt| receipt.complete);
        let resumed = receipts.iter().map(|receipt| receipt.received).sum::<u64>();
        let counts = receipts
            .iter()
            .map(|receipt| AtomicU64::new(receipt.received))
            .collect::<Vec<_>>();
        let current = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let new_bytes = AtomicU64::new(0);
        let mut final_error = None;
        if !ready {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(!valid_file || !ranged)
                .open(&data)
                .await
                .map_err(|e| message(e.into()))?;
            if ranged {
                file.set_len(remote.total.unwrap())
                    .await
                    .map_err(|e| message(e.into()))?;
            } else {
                file.set_len(0).await.map_err(|e| message(e.into()))?;
            }
            file.flush().await.map_err(|e| message(e.into()))?;
            drop(file);
            let limiter = Limiter {
                rate: options.max_bytes_per_second,
                next: Mutex::new(Instant::now()),
            };
            let job = Job {
                client,
                url,
                path: &data,
                directory: &directory,
                remote: &remote,
                ranged,
                trusted,
                cancel: &cancel,
                limiter: &limiter,
                current: &current,
                peak: &peak,
                new_bytes: &new_bytes,
            };
            let futures = stream::iter(blocks.iter().copied().zip(receipts.iter().cloned()))
                .map(|(part, receipt)| block(&job, part, receipt, &counts[part.index]))
                .buffer_unordered(if ranged {
                    options.connections.clamp(1, 16)
                } else {
                    1
                });
            futures_util::pin_mut!(futures);
            let mut ticks = tokio::time::interval(Duration::from_millis(200));
            let mut sample = Instant::now();
            let mut sample_bytes = 0;
            loop {
                tokio::select! {
                    result = futures.next() => match result {
                        Some(Err(error)) => {
                            if final_error.is_none() { final_error = Some(error); }
                            // Drain every worker before replacing files during fallback.
                            cancel.stop.store(true, Ordering::Relaxed);
                        },
                        Some(Ok(())) => {},
                        None => break,
                    },
                    _ = ticks.tick() => {
                        let fresh = new_bytes.load(Ordering::Relaxed);
                        let elapsed = sample.elapsed().as_secs_f64();
                        progress(Progress {
                            downloaded: counts.iter().map(|count| count.load(Ordering::Relaxed)).sum(),
                            total: remote.total.unwrap_or(0),
                            speed: if elapsed > 0.0 { ((fresh - sample_bytes) as f64 / elapsed) as u64 } else { 0 },
                            active_connections: current.load(Ordering::Relaxed),
                            resumed_bytes: resumed,
                            ranged,
                        });
                        sample = Instant::now();
                        sample_bytes = fresh;
                    }
                }
            }
        }
        let received = counts
            .iter()
            .map(|count| count.load(Ordering::Relaxed))
            .sum::<u64>();
        progress(Progress {
            downloaded: received,
            total: remote.total.unwrap_or(received),
            speed: 0,
            active_connections: 0,
            resumed_bytes: resumed,
            ranged,
        });
        if user_cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        if let Some(error) = final_error {
            if matches!(error, Failure::Restart) && fallback == 0 {
                discard(path).await;
                continue;
            }
            return Err(message(error));
        }
        if remote.total.is_some_and(|total| received != total) {
            return Err("下载数据长度不完整，请继续下载。".into());
        }
        let finished = if completed_path && ready { path } else { &data };
        if let Some(expected) = options.sha256 {
            let hash = prefix_hash(finished, 0, received).await.map_err(message)?;
            if format!("{:x}", hash.finalize()) != expected {
                discard(path).await;
                return Err("下载文件 SHA256 校验失败，已清理异常分段，请重试。".into());
            }
        }
        if !(completed_path && ready) {
            fs::rename(&data, path)
                .await
                .map_err(|e| message(e.into()))?;
        }
        return Ok(Outcome {
            bytes: received,
            resumed_bytes: resumed,
            peak_connections: peak.load(Ordering::Relaxed),
            ranged,
        });
    }
    Err("下载源不支持安全分段，请稍后重试。".into())
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
