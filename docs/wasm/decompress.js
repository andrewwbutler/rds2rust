export const DEFAULT_IN_MEMORY_THRESHOLD = 500 * 1024 * 1024;
export const DEFAULT_BLOB_THRESHOLD = 10 * 1024 * 1024 * 1024;

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
  const mode = recommendedMode(blob.size, navigator.deviceMemory);
  if (mode === "streaming") {
    return { mode, size: null };
  }

  const decompressor = new DecompressionStream("gzip");
  const stream = blob.stream().pipeThrough(decompressor);
  const reader = stream.getReader();
  const chunks = [];
  let total = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    chunks.push(value);
    total += value.byteLength;
    if (onProgress) {
      onProgress(total);
    }
  }

  if (total > blobThreshold) {
    return { mode: "streaming", size: total };
  }

  if (mode === "in-memory" && total <= inMemoryThreshold) {
    const buffer = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
      buffer.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return { mode, size: total, buffer };
  }

  const decompressedBlob = new Blob(chunks);
  return { mode: "blob", size: total, blob: decompressedBlob };
}
