#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::{HashMap, VecDeque};
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::{spawn_local, JsFuture};
#[cfg(target_arch = "wasm32")]
use web_sys::Blob;

#[cfg(target_arch = "wasm32")]
use crate::wasm::{AsyncRdsInput, AsyncReadFuture};
#[cfg(target_arch = "wasm32")]
use crate::{Error, Result};

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct CacheConfig {
    pub max_bytes: usize,
    pub chunk_size: usize,
    pub prefetch_distance: usize,
    pub adaptive: bool,
}

#[cfg(target_arch = "wasm32")]
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
            chunk_size: 4 * 1024 * 1024,
            prefetch_distance: 2,
            adaptive: true,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub prefetches: u64,
    pub bytes_read: u64,
}

#[cfg(target_arch = "wasm32")]
struct CacheState {
    map: HashMap<u64, Vec<u8>>,
    lru: VecDeque<u64>,
    current_bytes: usize,
    config: CacheConfig,
}

#[cfg(target_arch = "wasm32")]
impl CacheState {
    fn new(config: CacheConfig) -> Self {
        Self {
            map: HashMap::new(),
            lru: VecDeque::new(),
            current_bytes: 0,
            config,
        }
    }

    fn get(&mut self, key: u64) -> Option<Vec<u8>> {
        let chunk = self.map.get(&key)?.clone();
        self.lru.retain(|id| *id != key);
        self.lru.push_back(key);
        Some(chunk)
    }

    fn insert(&mut self, key: u64, buf: Vec<u8>) -> u64 {
        let mut evictions = 0;
        if buf.len() > self.config.max_bytes {
            self.map.clear();
            self.lru.clear();
            self.current_bytes = 0;
            return 0;
        }

        while self.current_bytes.saturating_add(buf.len()) > self.config.max_bytes {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(old) = self.map.remove(&oldest) {
                self.current_bytes = self.current_bytes.saturating_sub(old.len());
                evictions += 1;
            }
        }

        self.current_bytes += buf.len();
        self.map.insert(key, buf);
        self.lru.retain(|id| *id != key);
        self.lru.push_back(key);
        evictions
    }

    fn contains(&self, key: u64) -> bool {
        self.map.contains_key(&key)
    }
}

#[cfg(target_arch = "wasm32")]
struct BlobChunkedSourceInner {
    blob: Blob,
    cache: RefCell<CacheState>,
    metrics: RefCell<CacheMetrics>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct BlobChunkedSource {
    inner: Rc<BlobChunkedSourceInner>,
}

#[cfg(target_arch = "wasm32")]
impl BlobChunkedSource {
    pub fn new(blob: Blob, config: CacheConfig) -> Self {
        Self {
            inner: Rc::new(BlobChunkedSourceInner {
                blob,
                cache: RefCell::new(CacheState::new(config)),
                metrics: RefCell::new(CacheMetrics::default()),
            }),
        }
    }

    pub fn cache_metrics(&self) -> CacheMetrics {
        *self.inner.metrics.borrow()
    }

    pub fn cache_config(&self) -> CacheConfig {
        self.inner.cache.borrow().config
    }

    async fn read_at_inner(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let blob_len = self.inner.blob.size() as u64;
        let end = offset.saturating_add(len as u64);
        if end > blob_len {
            return Err(Error::UnexpectedEofDetail {
                position: offset as usize,
                needed: len,
                available: blob_len.saturating_sub(offset) as usize,
            });
        }

        let chunk_size = self.inner.cache.borrow().config.chunk_size;
        let start_chunk = (offset as usize) / chunk_size;
        let end_chunk = ((end - 1) as usize) / chunk_size;
        let mut out = vec![0u8; len];

        for chunk_idx in start_chunk..=end_chunk {
            let chunk_start = chunk_idx * chunk_size;
            let chunk_end = std::cmp::min(chunk_start + chunk_size, blob_len as usize);
            let read_start = std::cmp::max(chunk_start, offset as usize);
            let read_end = std::cmp::min(chunk_end, end as usize);
            let out_start = read_start - offset as usize;
            let out_end = read_end - offset as usize;
            let slice_start = read_start - chunk_start;
            let slice_end = read_end - chunk_start;

            let cached = self.get_cached_chunk(chunk_idx as u64);
            if let Some(chunk) = cached {
                out[out_start..out_end].copy_from_slice(&chunk[slice_start..slice_end]);
                continue;
            }

            let chunk = self
                .load_chunk(chunk_idx as u64, chunk_start, chunk_end)
                .await?;
            out[out_start..out_end].copy_from_slice(&chunk[slice_start..slice_end]);
            self.insert_chunk(chunk_idx as u64, chunk);
        }

        self.prefetch_after(end_chunk as u64);
        Ok(out)
    }

    fn get_cached_chunk(&self, idx: u64) -> Option<Vec<u8>> {
        let mut cache = self.inner.cache.borrow_mut();
        let Some(chunk) = cache.get(idx) else {
            self.inner.metrics.borrow_mut().misses += 1;
            return None;
        };
        self.inner.metrics.borrow_mut().hits += 1;
        Some(chunk)
    }

    fn insert_chunk(&self, idx: u64, buf: Vec<u8>) {
        let mut cache = self.inner.cache.borrow_mut();
        let evicted = cache.insert(idx, buf);
        if evicted > 0 {
            self.inner.metrics.borrow_mut().evictions += evicted as u64;
        }
    }

    async fn load_chunk(&self, _idx: u64, start: usize, end: usize) -> Result<Vec<u8>> {
        let slice = self
            .inner
            .blob
            .slice_with_f64_and_f64(start as f64, end as f64)
            .map_err(map_js_error)?;
        let array_buffer = JsFuture::from(slice.array_buffer())
            .await
            .map_err(map_js_error)?;
        let array = Uint8Array::new(&array_buffer);
        let mut buf = vec![0u8; array.length() as usize];
        array.copy_to(&mut buf);

        let mut metrics = self.inner.metrics.borrow_mut();
        metrics.bytes_read += buf.len() as u64;
        metrics.prefetches += 0;
        drop(metrics);

        Ok(buf)
    }

    fn prefetch_after(&self, idx: u64) {
        let config = self.inner.cache.borrow().config;
        if config.prefetch_distance == 0 {
            return;
        }

        for distance in 1..=config.prefetch_distance {
            let next_idx = idx + distance as u64;
            if self.inner.cache.borrow().contains(next_idx) {
                continue;
            }
            let cloned = self.clone();
            spawn_local(async move {
                if let Some((start, end)) = cloned.chunk_bounds(next_idx) {
                    if let Ok(chunk) = cloned.load_chunk(next_idx, start, end).await {
                        cloned.insert_chunk(next_idx, chunk);
                        cloned.inner.metrics.borrow_mut().prefetches += 1;
                    }
                }
            });
        }
    }

    fn chunk_bounds(&self, idx: u64) -> Option<(usize, usize)> {
        let len = self.inner.blob.size() as u64;
        let chunk_size = self.inner.cache.borrow().config.chunk_size as u64;
        let start = idx.saturating_mul(chunk_size);
        if start >= len {
            return None;
        }
        let end = std::cmp::min(start + chunk_size, len);
        Some((start as usize, end as usize))
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncRdsInput for BlobChunkedSource {
    fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a> {
        Box::pin(async move { self.read_at_inner(offset, len).await })
    }

    fn len(&self) -> Option<u64> {
        Some(self.inner.blob.size() as u64)
    }
}

#[cfg(target_arch = "wasm32")]
fn map_js_error(err: JsValue) -> Error {
    Error::Unsupported(format!("wasm blob error: {:?}", err))
}
