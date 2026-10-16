import { useCallback, useEffect, useRef, useState } from 'react';
import { privacyApi, type PrivacyStatus } from '../../api/privacy';
import { errorText } from '../../shared/errors';

export function usePrivacyGate() {
  const [status, setStatus] = useState<PrivacyStatus | null>(null);
  const [error, setError] = useState('');
  const lastTouch = useRef(0);
  const revision = useRef(0);
  const updateStatus = useCallback((next: PrivacyStatus) => {
    revision.current += 1;
    setStatus(next);
    setError('');
  }, []);
  const reload = useCallback(async () => {
    const request = ++revision.current;
    try {
      const next = await privacyApi.status();
      if (request === revision.current) {
        setStatus(next);
        setError('');
      }
    } catch (cause) {
      if (request === revision.current) setError(errorText(cause));
    }
  }, []);
  useEffect(() => {
    void reload();
    const timer = window.setInterval(() => void reload(), 5000);
    const focus = () => void reload();
    window.addEventListener('focus', focus);
    document.addEventListener('visibilitychange', focus);
    return () => {
      clearInterval(timer);
      window.removeEventListener('focus', focus);
      document.removeEventListener('visibilitychange', focus);
    };
  }, [reload]);
  useEffect(() => {
    if (!status || status.locked || !status.lock_enabled) return;
    const touch = () => {
      const now = Date.now();
      if (now - lastTouch.current < 5000) return;
      lastTouch.current = now;
      void privacyApi
        .touch()
        .then(() => void reload())
        .catch(() => void reload());
    };
    document.addEventListener('pointerdown', touch);
    document.addEventListener('keydown', touch);
    return () => {
      document.removeEventListener('pointerdown', touch);
      document.removeEventListener('keydown', touch);
    };
  }, [status?.locked, status?.lock_enabled, reload]);
  useEffect(() => {
    if (!status?.locked) return;
    document.querySelectorAll<HTMLInputElement>('input[type="password"]').forEach((input) => {
      input.value = '';
    });
  }, [status?.locked]);
  return { status, setStatus: updateStatus, error, reload };
}
