import { resampleImageData, type ResampleKind } from "./resample";
import type { ResampleWorkerRequest, ResampleWorkerResponse } from "./resample.worker";

type Job = {
  id: number;
  source: ImageData;
  targetWidth: number;
  targetHeight: number;
  kind: ResampleKind;
  resolve: (value: ImageData) => void;
  reject: (reason?: unknown) => void;
};

let worker: Worker | null = null;
let workerFailed = false;
let nextId = 1;
const pending = new Map<number, Job>();
const queue: Job[] = [];
let busy = false;

function ensureWorker(): Worker | null {
  if (workerFailed) return null;
  if (worker) return worker;
  try {
    worker = new Worker(new URL("./resample.worker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = (event: MessageEvent<ResampleWorkerResponse>) => {
      const response = event.data;
      const job = pending.get(response.id);
      pending.delete(response.id);
      busy = false;
      if (!job) {
        pump();
        return;
      }
      if (!response.ok) {
        // Fall back to main-thread resample for this job.
        try {
          job.resolve(
            resampleImageData(job.source, job.targetWidth, job.targetHeight, job.kind),
          );
        } catch (error) {
          job.reject(error);
        }
        pump();
        return;
      }
      job.resolve(
        new ImageData(
          new Uint8ClampedArray(response.buffer),
          response.width,
          response.height,
        ),
      );
      pump();
    };
    worker.onerror = () => {
      workerFailed = true;
      worker?.terminate();
      worker = null;
      // Drain queue on main thread.
      const stuck = [...pending.values(), ...queue];
      pending.clear();
      queue.length = 0;
      busy = false;
      for (const job of stuck) {
        try {
          job.resolve(
            resampleImageData(job.source, job.targetWidth, job.targetHeight, job.kind),
          );
        } catch (error) {
          job.reject(error);
        }
      }
    };
    return worker;
  } catch {
    workerFailed = true;
    return null;
  }
}

function pump() {
  if (busy) return;
  const job = queue.shift();
  if (!job) return;
  const w = ensureWorker();
  if (!w) {
    try {
      job.resolve(resampleImageData(job.source, job.targetWidth, job.targetHeight, job.kind));
    } catch (error) {
      job.reject(error);
    }
    pump();
    return;
  }
  busy = true;
  pending.set(job.id, job);
  // Copy pixels so the caller can keep using `source` if needed; transfer the copy.
  const copy = job.source.data.slice();
  const message: ResampleWorkerRequest = {
    id: job.id,
    width: job.source.width,
    height: job.source.height,
    buffer: copy.buffer,
    targetWidth: job.targetWidth,
    targetHeight: job.targetHeight,
    kind: job.kind,
  };
  w.postMessage(message, [copy.buffer]);
}

/**
 * Classic canvas resampling off the UI thread (single worker queue).
 * Falls back to sync main-thread resample if the worker cannot start.
 */
export function resampleImageDataAsync(
  source: ImageData,
  targetWidth: number,
  targetHeight: number,
  kind: ResampleKind,
): Promise<ImageData> {
  if (workerFailed) {
    return Promise.resolve(resampleImageData(source, targetWidth, targetHeight, kind));
  }
  return new Promise<ImageData>((resolve, reject) => {
    queue.push({
      id: nextId++,
      source,
      targetWidth,
      targetHeight,
      kind,
      resolve,
      reject,
    });
    ensureWorker();
    pump();
  });
}
