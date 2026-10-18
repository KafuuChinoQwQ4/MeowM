import { useCallback, useEffect, useState } from 'react';
import type { Account } from '../../api/accounts';
import { notificationsApi } from '../../api/notifications';
import { LoadState } from '../../components/LoadState';
import { FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
import { useResource } from '../../shared/useResource';
import { NotificationSourceCard } from './NotificationSourceCard';
import { ScreenCaptureCard } from './ScreenCaptureCard';
export function NotificationsPage({
  accounts,
  onPending,
}: {
  accounts: Account[];
  onPending: () => void;
}) {
  const [consent, setConsent] = useState(false);
  const [error, setError] = useState('');
  const [opening, setOpening] = useState(false);
  const state = useResource(
    useCallback(async () => {
      const [settings, access] = await Promise.all([
        notificationsApi.state(),
        notificationsApi.access(),
      ]);
      return { ...settings, access };
    }, []),
    true,
  );
  useEffect(() => {
    function resume() {
      if (!document.hidden) state.reload();
    }
    window.addEventListener('focus', resume);
    document.addEventListener('visibilitychange', resume);
    const timer = window.setInterval(resume, 5000);
    return () => {
      window.removeEventListener('focus', resume);
      document.removeEventListener('visibilitychange', resume);
      clearInterval(timer);
    };
  }, [state.reload]);
  async function openSettings() {
    setOpening(true);
    setError('');
    try {
      await notificationsApi.openSettings();
      state.reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setOpening(false);
    }
  }
  const data = state.data;
  return (
    <section className="notification-settings" aria-label="通知采集设置">
      <h2>通知采集</h2>
      <ScreenCaptureCard accounts={accounts} />
      <p>
        仅在 Android
        上读取你授权的微信、支付宝系统通知，用于生成待确认记录。原始正文仅在内存中处理，不保存或写入日志；核对确认后才计入余额。
      </p>
      <p className="field-hint">
        系统可能隐藏内容或限制后台服务，通知采集不能保证账单完整。关闭来源后停止处理新通知，已有候选仍可在收件箱中忽略。
      </p>
      <LoadState {...state} />
      <FormError message={error} />
      {data && (
        <>
          <p role="status">
            {!data.access.android
              ? '当前设备不支持 Android 通知采集'
              : !data.access.granted
                ? '未授予通知访问权限'
                : data.access.connected
                  ? '已授权，监听服务已连接'
                  : '已授权，监听服务尚未连接'}
          </p>
          {data.access.android && (
            <button
              className="button button-secondary"
              disabled={opening}
              onClick={() => void openSettings()}
            >
              打开系统通知访问设置
            </button>
          )}
          <button className="text-button" onClick={state.reload}>
            刷新采集状态
          </button>
          <p className="field-hint">
            授权后仍未连接时，可在系统设置中关闭再开启本应用的通知访问，并检查后台运行限制。
          </p>
          <label className="notification-toggle">
            <input
              type="checkbox"
              checked={consent}
              onChange={(e) => setConsent(e.target.checked)}
            />
            我已了解并同意通知采集用途
          </label>
          {data.sources.map((source) => (
            <NotificationSourceCard
              key={`${source.package_name}:${source.enabled}:${source.account_id}`}
              source={source}
              latest={data.latest.find((s) => s.package_name === source.package_name)}
              accounts={accounts}
              consent={consent}
              onSaved={state.reload}
            />
          ))}
          <button className="button button-secondary" onClick={onPending}>
            查看待确认收件箱
          </button>
        </>
      )}
    </section>
  );
}
