import {
  decompressRds,
  sizeWarning,
  browserSupportWarnings,
} from "../../wasm/decompress.js";

let currentTask = null;

self.onmessage = async (event) => {
  const { id, type, payload } = event.data || {};
  if (!id || !type) {
    return;
  }

  if (type === "cancel") {
    currentTask = null;
    return;
  }

  currentTask = { id, type };

  try {
    if (type === "decompress") {
      const warnings = browserSupportWarnings();
      const memWarn = sizeWarning(payload.file.size, self.navigator.deviceMemory);
      if (memWarn) {
        warnings.push(memWarn);
      }
      if (warnings.length > 0) {
        self.postMessage({ id, type: "warning", warnings });
      }

      const result = await decompressRds(payload.file, {
        onProgress: (progress) => {
          if (!currentTask || currentTask.id !== id) {
            return;
          }
          self.postMessage({
            id,
            type: "progress",
            phase: "decompress",
            bytes: progress.bytesProcessed || 0,
            message: progress.message,
          });
        },
      });
      self.postMessage({ id, type: "decompressed", result });
      return;
    }

    if (type === "parse") {
      self.postMessage({ id, type: "progress", phase: "parse", bytes: 0 });
      const result = await payload.parse();
      self.postMessage({ id, type: "parsed", result });
      return;
    }

    if (type === "extract") {
      self.postMessage({ id, type: "progress", phase: "extract", bytes: 0 });
      const result = await payload.extract();
      self.postMessage({ id, type: "extracted", result });
      return;
    }
  } catch (err) {
    self.postMessage({
      id,
      type: "error",
      error: err && err.message ? err.message : String(err),
    });
  }
};
