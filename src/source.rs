#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{HashMap, VecDeque};

#[cfg(not(target_arch = "wasm32"))]
use flate2::read::GzDecoder;
#[cfg(not(target_arch = "wasm32"))]
use memmap2::Mmap;
#[cfg(not(target_arch = "wasm32"))]
use tempfile::NamedTempFile;

#[cfg(not(target_arch = "wasm32"))]
use crate::Result;

#[cfg(not(target_arch = "wasm32"))]
pub struct MmapRdsSource {
    _temp: Option<NamedTempFile>,
    _file: Option<File>,
    mmap: Mmap,
}

#[cfg(not(target_arch = "wasm32"))]
impl MmapRdsSource {
    pub fn from_path(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 2];
        let read = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;

        if read == 2 && magic == [0x1f, 0x8b] {
            Self::from_gzip(file)
        } else {
            Self::from_file(file)
        }
    }

    fn from_file(file: File) -> Result<Self> {
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            _temp: None,
            _file: Some(file),
            mmap,
        })
    }

    fn from_gzip(file: File) -> Result<Self> {
        let mut temp = NamedTempFile::new()?;
        let mut decoder = GzDecoder::new(file);
        std::io::copy(&mut decoder, temp.as_file_mut())?;
        temp.as_file_mut().flush()?;

        let mmap = unsafe { Mmap::map(temp.as_file())? };
        Ok(Self {
            _temp: Some(temp),
            _file: None,
            mmap,
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub trait RdsInput {
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>;
    fn len(&self) -> Option<u64>;
}

#[cfg(not(target_arch = "wasm32"))]
impl RdsInput for MmapRdsSource {
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = offset as usize;
        let end = start.saturating_add(len);
        if end > self.mmap.len() {
            return Err(crate::Error::UnexpectedEofDetail {
                position: start,
                needed: len,
                available: self.mmap.len().saturating_sub(start),
            });
        }
        Ok(self.mmap[start..end].to_vec())
    }

    fn len(&self) -> Option<u64> {
        Some(self.mmap.len() as u64)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct ChunkedRdsSource {
    _temp: Option<NamedTempFile>,
    file: std::sync::Mutex<File>,
    len: u64,
    chunk_size: usize,
    cache: std::sync::Mutex<ChunkCacheState>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkedRdsSource {
    pub fn from_path(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 2];
        let read = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;

        if read == 2 && magic == [0x1f, 0x8b] {
            Self::from_gzip(file)
        } else {
            Self::from_file(file)
        }
    }

    fn from_file(file: File) -> Result<Self> {
        let len = file.metadata()?.len();
        Ok(Self {
            _temp: None,
            file: std::sync::Mutex::new(file),
            len,
            chunk_size: DEFAULT_CHUNK_SIZE,
            cache: std::sync::Mutex::new(ChunkCacheState::new(DEFAULT_CACHE_MAX_BYTES)),
        })
    }

    fn from_gzip(file: File) -> Result<Self> {
        let mut temp = NamedTempFile::new()?;
        let mut decoder = GzDecoder::new(file);
        std::io::copy(&mut decoder, temp.as_file_mut())?;
        temp.as_file_mut().flush()?;

        let len = temp.as_file().metadata()?.len();
        let file = temp.as_file().try_clone()?;
        Ok(Self {
            _temp: Some(temp),
            file: std::sync::Mutex::new(file),
            len,
            chunk_size: DEFAULT_CHUNK_SIZE,
            cache: std::sync::Mutex::new(ChunkCacheState::new(DEFAULT_CACHE_MAX_BYTES)),
        })
    }

    pub fn cache_metrics(&self) -> ChunkedCacheMetrics {
        let state = self.cache.lock().unwrap();
        state.metrics
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RdsInput for ChunkedRdsSource {
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let start = offset as usize;
        let end = start.saturating_add(len);
        let total_len = self.len as usize;
        if end > total_len {
            return Err(crate::Error::UnexpectedEofDetail {
                position: start,
                needed: len,
                available: total_len.saturating_sub(start),
            });
        }

        let mut out = vec![0u8; len];
        let first_chunk = start / self.chunk_size;
        let last_chunk = (end - 1) / self.chunk_size;

        for chunk_idx in first_chunk..=last_chunk {
            let chunk_start = chunk_idx * self.chunk_size;
            let chunk_end = std::cmp::min(chunk_start + self.chunk_size, total_len);
            let read_start = std::cmp::max(start, chunk_start);
            let read_end = std::cmp::min(end, chunk_end);
            let out_start = read_start - start;
            let out_end = read_end - start;
            let slice_start = read_start - chunk_start;
            let slice_end = read_end - chunk_start;

            if self.copy_cached_slice(
                chunk_idx as u64,
                slice_start,
                slice_end,
                &mut out[out_start..out_end],
            ) {
                continue;
            }

            let chunk = self.read_chunk_from_file(chunk_start, chunk_end - chunk_start)?;
            self.record_cache_miss(chunk.len());
            out[out_start..out_end].copy_from_slice(&chunk[slice_start..slice_end]);
            self.insert_chunk(chunk_idx as u64, chunk);

            if chunk_idx == last_chunk {
                self.prefetch_next_chunk(chunk_idx as u64, total_len);
            }
        }

        Ok(out)
    }

    fn len(&self) -> Option<u64> {
        Some(self.len)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkedRdsSource {
    fn copy_cached_slice(
        &self,
        chunk_idx: u64,
        start: usize,
        end: usize,
        out: &mut [u8],
    ) -> bool {
        let mut state = self.cache.lock().unwrap();
        {
            let Some(chunk) = state.map.get(&chunk_idx) else {
                return false;
            };
            out.copy_from_slice(&chunk[start..end]);
        }
        state.metrics.hits += 1;
        state.lru.retain(|id| *id != chunk_idx);
        state.lru.push_back(chunk_idx);
        true
    }

    fn read_chunk_from_file(&self, start: usize, len: usize) -> Result<Vec<u8>> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(start as u64))?;
        let mut buf = vec![0u8; len];
        let mut read = 0;
        while read < len {
            let n = file.read(&mut buf[read..])?;
            if n == 0 {
                return Err(crate::Error::UnexpectedEofDetail {
                    position: start.saturating_add(read),
                    needed: len,
                    available: read,
                });
            }
            read += n;
        }

        Ok(buf)
    }

    fn insert_chunk(&self, chunk_idx: u64, buf: Vec<u8>) {
        let mut state = self.cache.lock().unwrap();
        if buf.len() > state.max_bytes {
            state.map.clear();
            state.lru.clear();
            state.current_bytes = 0;
            return;
        }

        state.evict_if_needed(buf.len());
        state.current_bytes += buf.len();
        state.map.insert(chunk_idx, buf);
        state.lru.retain(|id| *id != chunk_idx);
        state.lru.push_back(chunk_idx);
    }

    fn record_cache_miss(&self, bytes_read: usize) {
        let mut state = self.cache.lock().unwrap();
        state.metrics.misses += 1;
        state.metrics.bytes_read += bytes_read as u64;
    }

    fn record_prefetch(&self, bytes_read: usize) {
        let mut state = self.cache.lock().unwrap();
        state.metrics.prefetches += 1;
        state.metrics.bytes_read += bytes_read as u64;
    }

    fn is_chunk_cached(&self, chunk_idx: u64) -> bool {
        let state = self.cache.lock().unwrap();
        state.map.contains_key(&chunk_idx)
    }

    fn prefetch_next_chunk(&self, chunk_idx: u64, total_len: usize) {
        let next_idx = chunk_idx + 1;
        let chunk_start = next_idx as usize * self.chunk_size;
        if chunk_start >= total_len {
            return;
        }
        if self.is_chunk_cached(next_idx) {
            return;
        }

        let chunk_end = std::cmp::min(chunk_start + self.chunk_size, total_len);
        let chunk_len = chunk_end - chunk_start;
        let chunk = match self.read_chunk_from_file(chunk_start, chunk_len) {
            Ok(chunk) => chunk,
            Err(_) => return,
        };
        self.record_prefetch(chunk.len());
        self.insert_chunk(next_idx, chunk);
    }
}

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_CHUNK_SIZE: usize = 4 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct ChunkedCacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub bytes_read: u64,
    pub prefetches: u64,
}

#[cfg(not(target_arch = "wasm32"))]
struct ChunkCacheState {
    map: HashMap<u64, Vec<u8>>,
    lru: VecDeque<u64>,
    current_bytes: usize,
    max_bytes: usize,
    metrics: ChunkedCacheMetrics,
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkCacheState {
    fn new(max_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            lru: VecDeque::new(),
            current_bytes: 0,
            max_bytes,
            metrics: ChunkedCacheMetrics::default(),
        }
    }

    fn evict_if_needed(&mut self, incoming: usize) {
        while self.current_bytes.saturating_add(incoming) > self.max_bytes {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(buf) = self.map.remove(&oldest) {
                self.current_bytes = self.current_bytes.saturating_sub(buf.len());
            }
        }
    }
}
