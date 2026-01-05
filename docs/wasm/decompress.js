export const DEFAULT_IN_MEMORY_THRESHOLD = 500 * 1024 * 1024;
export const DEFAULT_BLOB_THRESHOLD = 10 * 1024 * 1024 * 1024;
export const DEFAULT_MAX_RATIO = 1000;

export function recommendedMode(fileSize, deviceMemoryGb) {
  const memoryGb = deviceMemoryGb || 4;
  const maxBlobSize = DEFAULT_BLOB_THRESHOLD;

  if (fileSize < DEFAULT_IN_MEMORY_THRESHOLD) {
    return "in-memory";
  }
  if (fileSize <= maxBlobSize) {
    return "blob";
  }
  return "streaming";
}

export function sizeWarning(fileSize, deviceMemoryGb) {
  const memoryGb = deviceMemoryGb || 4;
  const fileGb = fileSize / (1024 * 1024 * 1024);

  if (fileGb > memoryGb * 2) {
    return `File size ${fileGb.toFixed(1)}GB is large for ${memoryGb}GB RAM. Consider streaming mode.`;
  }
  return null;
}

export function browserSupportWarnings() {
  const issues = [];
  if (typeof DecompressionStream === "undefined") {
    issues.push("DecompressionStream not available.");
  }
  if (typeof Worker === "undefined") {
    issues.push("Web Workers not available.");
  }
  return issues;
}

export async function detectCompression(blob) {
  const header = await blob.slice(0, 4).arrayBuffer();
  const view = new Uint8Array(header);
  if (view[0] === 0x1f && view[1] === 0x8b) {
    return "gzip";
  }
  if (view[0] === 0x42 && view[1] === 0x5a) {
    return "bzip2";
  }
  if (view[0] === 0xfd && view[1] === 0x37) {
    return "xz";
  }
  if (view[0] === 0x58 && view[1] === 0x0a) {
    return "rds";
  }
  return "unknown";
}

function estimateDecompressedSize(compressedBytes, ratioEstimate = 3) {
  return compressedBytes * ratioEstimate;
}

function calculateTimeoutMs(compressedBytes) {
  const sizeGb = compressedBytes / (1024 ** 3);
  const timeout = sizeGb * 60000;
  return Math.max(30000, Math.min(600000, timeout));
}

async function decompressWithTimeout(promise, timeoutMs) {
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      setTimeout(() => {
        reject(
          new Error(
            `Decompression timeout after ${Math.round(timeoutMs / 1000)}s.`
          )
        );
      }, timeoutMs);
    }),
  ]);
}

export async function decompressBlobIfNeeded(blob, options = {}) {
  const {
    filename,
    onProgress,
    budgetBytes,
    maxRatio = DEFAULT_MAX_RATIO,
    ratioEstimate = 3,
    timeoutMs,
    testDelayMs,
  } = options;

  if (typeof DecompressionStream === "undefined") {
    throw new Error(
      "DecompressionStream not available. Use Chrome 89+, Firefox 102+, or Safari 16.4+."
    );
  }

  const compression = await detectCompression(blob);

  if (filename) {
    const lower = filename.toLowerCase();
    if (lower.endsWith(".gz") && compression !== "gzip") {
      throw new Error(
        `File "${filename}" has .gz extension but is not gzip compressed.`
      );
    }
    if (lower.endsWith(".rds") && compression !== "rds" && compression !== "gzip") {
      throw new Error(
        `File "${filename}" does not appear to be a valid RDS file.`
      );
    }
  }

  if (compression === "rds") {
    return blob;
  }
  if (compression === "bzip2") {
    throw new Error("bzip2 not supported in WASM. Please decompress first.");
  }
  if (compression === "xz") {
    throw new Error("xz not supported in WASM. Please decompress first.");
  }
  if (compression !== "gzip") {
    throw new Error("Unrecognized file format. Expected gzip or RDS.");
  }

  if (budgetBytes) {
    const estimated = estimateDecompressedSize(blob.size, ratioEstimate);
    if (estimated > budgetBytes) {
      throw new Error(
        `Estimated decompressed size exceeds budget (${Math.round(estimated)} > ${budgetBytes}).`
      );
    }
  }

  const decompressor = new DecompressionStream("gzip");
  const stream = blob.stream().pipeThrough(decompressor);
  const reader = stream.getReader();
  const chunks = [];
  let total = 0;
  const maxBytes = blob.size * maxRatio;
  const effectiveTimeoutMs =
    typeof timeoutMs === "number" ? timeoutMs : calculateTimeoutMs(blob.size);

  const decompressPromise = (async () => {
    if (typeof testDelayMs === "number" && testDelayMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, testDelayMs));
    }
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      total += value.byteLength;
      if (total > maxBytes) {
        throw new Error(
          `Compression ratio exceeded safety limit (${maxRatio}:1).`
        );
      }
      if (budgetBytes && total > budgetBytes) {
        throw new Error(
          `Decompressed size exceeds budget (${total} > ${budgetBytes}).`
        );
      }
      chunks.push(value);
      if (onProgress) {
        onProgress({
          phase: "decompressing",
          bytesProcessed: total,
          message: `Decompressed ${(total / (1024 ** 2)).toFixed(1)}MB`,
        });
      }
    }
    return new Blob(chunks);
  })();

  return decompressWithTimeout(decompressPromise, effectiveTimeoutMs);
}

export async function decompressRds(
  blob,
  {
    onProgress,
    inMemoryThreshold = DEFAULT_IN_MEMORY_THRESHOLD,
    blobThreshold = DEFAULT_BLOB_THRESHOLD,
  } = {},
) {
  if (typeof DecompressionStream === "undefined") {
    throw new Error("DecompressionStream not available.");
  }
  const decompressed = await decompressBlobIfNeeded(blob, {
    onProgress,
  });
  const decompressedSize = decompressed.size;
  const mode = recommendedMode(decompressedSize, navigator.deviceMemory);
  if (mode === "streaming") {
    return { mode, size: decompressedSize };
  }

  if (mode === "in-memory" && decompressedSize <= inMemoryThreshold) {
    const buffer = new Uint8Array(await decompressed.arrayBuffer());
    return { mode, size: decompressedSize, buffer };
  }

  if (decompressedSize > blobThreshold) {
    return { mode: "streaming", size: decompressedSize };
  }

  return { mode: "blob", size: decompressedSize, blob: decompressed };
}
