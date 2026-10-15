import { useCallback, useEffect, useState } from 'react';
import { errorText } from './errors';
// Callers supply a memoized loader. Cleanup rejects responses from old parameters/unmounted views.
export function useResource<T>(load: () => Promise<T>, retainData = false) {
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{ data: T | null; loading: boolean; error: string }>({
    data: null,
    loading: true,
    error: '',
  });
  useEffect(() => {
    let active = true;
    setState((previous) => ({ data: retainData ? previous.data : null, loading: true, error: '' }));
    load().then(
      (data) => {
        if (active) setState({ data, loading: false, error: '' });
      },
      (cause) => {
        if (active)
          setState((previous) => ({
            data: retainData ? previous.data : null,
            loading: false,
            error: errorText(cause),
          }));
      },
    );
    return () => {
      active = false;
    };
  }, [load, revision, retainData]);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  return { ...state, reload };
}
