import { useSearchParams } from 'react-router';
import { useDevices } from '@/queries/devices';
import type { Device } from '@/connector/contract';

/**
 * Physical devices (drives/cards) for pickers — excludes the "Specify a
 * location" virtual targets (Recycle Bin / Desktop), which are scan
 * entries, not hardware surfaces. Supports `?device=` preselection and
 * falls back to the first physical device.
 */
export function usePhysicalDevices(): {
  devices: Device[];
  selectedId: string;
  setSelectedId: (id: string) => void;
  isLoading: boolean;
  isError: boolean;
  refetch: () => void;
} {
  const [params, setParams] = useSearchParams();
  const query = useDevices();
  const devices = (query.data ?? []).filter((d) => d.kind !== 'virtual');
  const paramId = params.get('device');
  const selectedId =
    paramId && devices.some((d) => d.id === paramId) ? paramId : (devices[0]?.id ?? '');
  return {
    devices,
    selectedId,
    setSelectedId: (id) => {
      const next = new URLSearchParams(params);
      next.set('device', id);
      setParams(next, { replace: true });
    },
    isLoading: query.isLoading,
    isError: query.isError,
    refetch: () => void query.refetch(),
  };
}
