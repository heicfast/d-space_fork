/// <reference lib="webworker" />
/**
 * Hex preview worker — formats bounded byte slices into offset/hex/ascii rows
 * off the main thread (grid #37/#38). Input is capped by the connector
 * contract, never by the UI. Row formatting itself lives in the shared
 * lib/hex-format module (single implementation for worker + fallback).
 */
import { formatHexRows, type HexRow } from '@/lib/hex-format';

export type { HexRow };

interface HexRequest {
  bytes: ArrayBuffer;
  baseOffset: number;
}

// Module-scoped: this file IS a dedicated worker context. The app's tsconfig
// also loads the DOM lib (whose `self` is Window), so the worker global is
// declared here as the precise boundary type instead of cast.
declare const self: DedicatedWorkerGlobalScope;

self.onmessage = (e: MessageEvent<HexRequest>) => {
  const { bytes, baseOffset } = e.data;
  const rows = formatHexRows(new Uint8Array(bytes), baseOffset);
  self.postMessage(rows);
};
