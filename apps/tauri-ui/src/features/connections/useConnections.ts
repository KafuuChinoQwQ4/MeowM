import { useCallback, useEffect, useState } from 'react';
import {
  connectionsApi,
  type ConnectionStatus,
  type ConnectionTestRequest,
  type ConnectionTestResult,
  type CredentialInput,
  type ProviderId,
} from '../../api/connections';

export function useConnections() {
  const [statuses, setStatuses] = useState<ConnectionStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  const reload = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      setStatuses(await connectionsApi.status());
    } catch {
      setError('无法读取系统安全凭据状态。');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const save = useCallback(async (input: CredentialInput) => {
    const status = await connectionsApi.save(input);
    setStatuses((current) => replaceStatus(current, status));
  }, []);

  const remove = useCallback(async (provider: ProviderId) => {
    const status = await connectionsApi.remove(provider);
    setStatuses((current) => replaceStatus(current, status));
  }, []);

  const test = useCallback(
    async (provider: ProviderId, options: Omit<ConnectionTestRequest, 'provider'>) => {
      return connectionsApi.test({ provider, ...options });
    },
    [],
  );

  return { statuses, loading, error, reload, save, remove, test };
}

function replaceStatus(current: ConnectionStatus[], next: ConnectionStatus) {
  return [...current.filter((item) => item.provider !== next.provider), next];
}
