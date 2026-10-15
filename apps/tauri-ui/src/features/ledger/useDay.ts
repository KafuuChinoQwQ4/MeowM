import { useCallback } from 'react';
import { ledgerApi } from '../../api/ledger';
import { useResource } from '../../shared/useResource';
export function useDay(accountId: string, date: string, revision: number) {
  return useResource(
    useCallback(
      () => (accountId ? ledgerApi.day(accountId, date) : Promise.resolve(null)),
      [accountId, date, revision],
    ),
  );
}
