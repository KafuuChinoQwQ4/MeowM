import { useCallback, useEffect } from 'react';
import { recurringApi } from '../../api/recurring';
import { useResource } from '../../shared/useResource';
// Startup, app foreground and an open-app timer share the same idempotent command.
export function useRecurring(revision: number, onGenerated: () => void) {
  const load = useCallback(async () => {
    const result = await recurringApi.state();
    if (result.generated_count > 0) onGenerated();
    return result;
  }, [revision, onGenerated]);
  const state = useResource(load, true);
  useEffect(() => {
    const refresh = () => {
      if (document.visibilityState === 'visible') state.reload();
    };
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', refresh);
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      window.removeEventListener('focus', refresh);
      document.removeEventListener('visibilitychange', refresh);
      window.clearInterval(timer);
    };
  }, [state.reload]);
  return state;
}
export type RecurringResource = ReturnType<typeof useRecurring>;
