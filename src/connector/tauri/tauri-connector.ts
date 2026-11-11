/**
 * Real IPC adapter — STUB.
 *
 * This adapter will be wired in the engine-integration session. Until then
 * every method throws NotImplementedError with a pointer to the contract, so a
 * misconfigured build fails loudly and truthfully instead of pretending to
 * work (SOP: no fallback that masks failure).
 */

import type { RecoveryConnector } from '../contract';
import { connNotImplemented } from './not-implemented';

export function createTauriConnector(): RecoveryConnector {
  const stub = <T extends keyof RecoveryConnector>(domain: T): RecoveryConnector[T] => {
    return new Proxy({} as RecoveryConnector[T], {
      get(_target, prop: string) {
        return (...args: unknown[]): never => {
          throw connNotImplemented(`${String(domain)}.${String(prop)}`, args.length);
        };
      },
    });
  };

  return {
    devices: stub('devices'),
    scan: stub('scan'),
    results: stub('results'),
    preview: stub('preview'),
    sessions: stub('sessions'),
    recovery: stub('recovery'),
    health: stub('health'),
    repair: stub('repair'),
    system: stub('system'),
  };
}
