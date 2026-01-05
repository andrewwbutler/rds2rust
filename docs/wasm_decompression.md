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
} from "./wasm/decompress.js";

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

See `docs/wasm_gzip_troubleshooting.md` for common errors and fixes.
