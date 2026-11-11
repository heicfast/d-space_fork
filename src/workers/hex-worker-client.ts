/**
 * Hex worker client — spins up the module worker with a graceful in-line
 * fallback for environments without Worker (jsdom tests). Both paths share
 * the exact same row formatter (lib/hex-format); the worker path is the
 * default in browser and WebView2.
 */
import { formatHexRows, type HexRow } from '@/lib/hex-format';

export type { HexRow };

export async function hexWorker(bytes: Uint8Array, baseOffset: number): Promise<HexRow[]> {
  if (typeof Worker !== 'undefined') {
    return new Promise<HexRow[]>((resolve, reject) => {
      const worker = new Worker(new URL('./hex.worker.ts', import.meta.url), {
        type: 'module',
      });
      worker.onmessage = (e: MessageEvent<HexRow[]>) => {
        resolve(e.data);
        worker.terminate();
      };
      worker.onerror = (e) => {
        reject(new Error(e.message || 'hex worker failed'));
        worker.terminate();
      };
      // Copy into a transferable buffer (the Uint8Array may be a view over a larger buffer).
      const copy = bytes.slice().buffer;
      worker.postMessage({ bytes: copy, baseOffset }, [copy]);
    });
  }
  return formatHexRows(bytes, baseOffset);
}
