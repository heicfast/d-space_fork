export function connNotImplemented(method: string, _arity: number): Error {
  const err = new Error(
    `${method}() is not implemented yet: the real IPC bridge arrives in the engine-integration session. ` +
      `The UI is contract-complete against src/connector/contract.ts — see docs/CONNECTOR-CONTRACT.md. ` +
      `Run with VITE_CONNECTOR=mock (the default) for the deterministic simulator.`,
  );
  (err as { code?: string }).code = 'not-implemented';
  return err;
}
