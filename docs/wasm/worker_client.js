export class RdsWorkerClient {
  constructor(workerUrl) {
    this.worker = new Worker(workerUrl);
    this.pending = new Map();
    this.worker.onmessage = (event) => {
      const { id, type, ...payload } = event.data || {};
      if (!id || !this.pending.has(id)) {
        return;
      }
      const handlers = this.pending.get(id);
      if (type === "error") {
        handlers.reject(new Error(payload.error || "Worker error"));
        this.pending.delete(id);
        return;
      }
      if (type === "warning" && handlers.onWarning) {
        handlers.onWarning(payload.warnings);
        return;
      }
      if (type === "progress" && handlers.onProgress) {
        handlers.onProgress(payload.phase, payload.bytes || 0);
        return;
      }
      if (type === "decompressed" || type === "parsed" || type === "extracted") {
        handlers.resolve(payload.result);
        this.pending.delete(id);
      }
    };
  }

  run(type, payload, { onProgress, onWarning, timeoutMs } = {}) {
    const id = crypto.randomUUID();
    const promise = new Promise((resolve, reject) => {
      const timeout = timeoutMs
        ? setTimeout(() => {
            this.pending.delete(id);
            reject(new Error("Worker timeout"));
          }, timeoutMs)
        : null;
      this.pending.set(id, {
        resolve: (result) => {
          if (timeout) {
            clearTimeout(timeout);
          }
          resolve(result);
        },
        reject: (err) => {
          if (timeout) {
            clearTimeout(timeout);
          }
          reject(err);
        },
        onProgress,
        onWarning,
      });
    });
    this.worker.postMessage({ id, type, payload });
    return promise;
  }

  terminate() {
    this.worker.terminate();
    this.pending.clear();
  }
}
