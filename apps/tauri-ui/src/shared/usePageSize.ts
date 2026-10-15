import { useEffect, useState } from 'react';
export function usePageSize(base = 330, row = 62) {
  const size = () => Math.max(1, Math.min(6, Math.floor((window.innerHeight - base) / row)));
  const [value, setValue] = useState(size);
  useEffect(() => {
    const resize = () => setValue(size());
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, [base, row]);
  return value;
}
