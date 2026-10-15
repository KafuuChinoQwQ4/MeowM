import { useCallback } from 'react';
import { accountsApi } from '../../api/accounts';
import { useResource } from '../../shared/useResource';
export function useAccounts() {
  return useResource(useCallback(() => accountsApi.list(), []));
}
