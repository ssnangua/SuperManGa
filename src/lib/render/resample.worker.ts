/// <reference lib="webworker" />
import { resampleImageData, type ResampleKind } from "./resample";

export type ResampleWorkerRequest = {
  id: number;
  width: number;
  height: number;
  buffer: ArrayBuffer;
  targetWidth: number;
  targetHeight: number;
  kind: ResampleKind;
};

export type ResampleWorkerResponse =
  | {
      id: number;
      ok: true;
      width: number;
      height: number;
      buffer: ArrayBuffer;
    }
  | { id: number; ok: false; error: string };

self.onmessage = (event: MessageEvent<ResampleWorkerRequest>) => {
  const job = event.data;
  try {
    const source = new ImageData(
      new Uint8ClampedArray(job.buffer),
      job.width,
      job.height,
    );
    const out = resampleImageData(source, job.targetWidth, job.targetHeight, job.kind);
    const buffer = out.data.buffer.slice(
      out.data.byteOffset,
      out.data.byteOffset + out.data.byteLength,
    );
    const response: ResampleWorkerResponse = {
      id: job.id,
      ok: true,
      width: out.width,
      height: out.height,
      buffer,
    };
    (self as DedicatedWorkerGlobalScope).postMessage(response, [buffer]);
  } catch (error) {
    const response: ResampleWorkerResponse = {
      id: job.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
    (self as DedicatedWorkerGlobalScope).postMessage(response);
  }
};
