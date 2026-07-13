# WASM Decompression Helper (Phase 0)

This document describes the Phase 0 JavaScript helper for RDS decompression
and size-based mode selection in a WASM context.

## Size Thresholds

- **<500MB**: in-memory buffer (`Uint8Array`).
- **500MB–10GB**: Blob-backed storage (`Blob`).
- **>10GB**: streaming mode (future path).

## Device Memory Warning

If `file_size > device_memory * 2`, warn the user before attempting a Blob-based path.

## Usage

```javascript
import {
  decompressRds,
  decompressBlobIfNeeded,
  recommendedMode,
  sizeWarning,
  browserSupportWarnings,
} from "../wasm/decompress.js";

const warnings = browserSupportWarnings();
if (warnings.length > 0) {
  console.warn(warnings.join(" "));
}

const warning = sizeWarning(file.size, navigator.deviceMemory);
if (warning) {
  console.warn(warning);
}

const result = await decompressRds(file, {
  onProgress: (progress) => console.log(progress.message),
});

if (result.mode === "in-memory") {
  // result.buffer: Uint8Array
} else if (result.mode === "blob") {
  // result.blob: Blob
} else {
  // streaming mode (future)
}

// Decompress only (auto-detects gzip)
const decompressed = await decompressBlobIfNeeded(file, {
  filename: file.name,
});
```

## WASM Streaming Writer

The WASM bundle exposes streaming writer helpers that emit `Uint8Array` chunks.

```javascript
import {
  writeRdsWithCallback,
  writeRdsWithProgress,
  recommendedChunkSizeMb,
} from "./rds2rust_wasm.js";

const chunks = [];
const chunkSizeMb = recommendedChunkSizeMb();

writeRdsWithCallback(obj, (chunk) => {
  chunks.push(chunk);
}, chunkSizeMb);
```

Progress reports bytes written (not percent):

```javascript
writeRdsWithProgress(
  obj,
  (chunk) => chunks.push(chunk),
  (bytesWritten) => console.log(`wrote ${bytesWritten} bytes`),
  chunkSizeMb
);
```

Advanced options for `decompressBlobIfNeeded`:

- `budgetBytes`: hard cap on decompressed bytes.
- `maxRatio`: compression ratio limit for zip bomb protection.
- `ratioEstimate`: conservative estimate used for budget pre-checks.
- `timeoutMs`: override the adaptive timeout (useful for tight SLAs).
- `testDelayMs`: test-only hook to simulate slow decompression in unit tests.

## Worker Integration (Phase 5)

The worker wrapper in `docs/wasm/worker.js` posts progress updates and supports
timeouts and warnings. A minimal client wrapper is in `docs/wasm/worker_client.js`.

```javascript
import { RdsWorkerClient } from "./wasm/worker_client.js";

const client = new RdsWorkerClient(new URL("./wasm/worker.js", import.meta.url));

const result = await client.run("decompress", { file }, {
  onProgress: (phase, bytes) => console.log(phase, bytes),
  onWarning: (warnings) => console.warn(warnings.join(" ")),
  timeoutMs: 5 * 60 * 1000,
});
```

## Validation Targets (Phase 6)

| File Size | Operation | Target Time | Target Peak Memory |
| --- | --- | --- | --- |
| 1GB | Parse metadata | <10s | <400MB |
| 1GB | Extract vector | <5s | <400MB |
| 5GB | Parse metadata | <30s | <600MB |
| 5GB | Extract vector | <15s | <600MB |
| 10GB | Parse metadata | <60s | <1GB (desired) |
| 10GB | Extract vector | <30s | <1GB (desired) |

## Browser Compatibility

- Chrome/Edge 89+, Firefox 102+, Safari 16.4+.
- Mobile Safari: recommend <=2GB, reduce cache to 128MB.
- Warn when `DecompressionStream` or workers are unavailable.

## Compression Format Support

| Format | Extension | Status |
| --- | --- | --- |
| gzip | `.rds.gz`, `.rds.gzip` | Supported |
| uncompressed | `.rds` | Supported |
| bzip2 | `.rds.bz2` | Unsupported |
| xz | `.rds.xz` | Unsupported |

## Troubleshooting

Common issues:

- **`DecompressionStream is not defined`** — the browser lacks the
  `DecompressionStream` API (needs Chrome/Edge 89+, Firefox 102+, Safari 16.4+).
  Pre-decompress with `decompressBlobIfNeeded()` or fall back to an in-memory path.
- **Unsupported format error on `.rds.xz` / `.rds.bz2`** — xz and bzip2 are not
  decompressed on `wasm32`; decompress the file before handing it to the WASM
  helper. (Native builds do read xz.)
- **Out-of-memory on very large files** — ensure the size-based strategy is
  active (Blob-backed chunked reads above ~500 MB, streaming above ~10 GB) rather
  than forcing a full in-memory buffer.
