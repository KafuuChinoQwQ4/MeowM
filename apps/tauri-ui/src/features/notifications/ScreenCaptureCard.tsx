import { useCallback, useEffect, useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import {
  screenCaptureApi,
  type ScreenCaptureSource,
  type ScreenCaptureStatus,
} from '../../api/screenCapture';
import { FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
const labels: Record<string, string> = {
  'com.tencent.mm': '微信支付页面',
  'com.eg.android.AlipayGphone': '支付宝支付页面',
};
const results: Record<string, string> = {
  pending: '已生成待确认记录',
  needs_confirmation: '已生成待补全记录',
  duplicate: '已关联重复采集',
  ignored: '未识别到可用支付信息',
  error: '识别失败，请停止后重新授权',
  permission_revoked: '屏幕捕获已结束，请重新授权',
  usage_revoked: '使用情况访问权限已撤销',
  notification_required: '通知权限已关闭，请允许通知后重新授权',
  screen_locked: '锁屏已结束捕获，请重新授权',
};
export function ScreenCaptureCard({ accounts }: { accounts: Account[] }) {
  const dirty = useRef(false);
  const [latest, setLatest] = useState<string[]>([]);
  const [loadError, setLoadError] = useState('');
  const [sources, setSources] = useState<ScreenCaptureSource[]>([]);
  const [status, setStatus] = useState<ScreenCaptureStatus | null>(null);
  const [consent, setConsent] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const reload = useCallback(async () => {
    try {
      const [state, current] = await Promise.all([
        screenCaptureApi.state(),
        screenCaptureApi.status(),
      ]);
      if (!dirty.current) setSources(state.sources);
      setStatus(current);
      setLatest(
        state.latest.map(
          (r) => `${labels[r.package_name]}：${results[r.code] ?? '识别结果待检查'}`,
        ),
      );
      setLoadError('');
    } catch (cause) {
      setLoadError(errorText(cause));
    }
  }, []);
  useEffect(() => {
    void reload();
    const timer = window.setInterval(() => void reload(), 3000);
    return () => window.clearInterval(timer);
  }, [reload]);
  function update(package_name: string, change: Partial<ScreenCaptureSource>) {
    const current = sources.find((source) => source.package_name === package_name);
    if (!current) return;
    const next = { ...current, ...change };
    if (next.enabled && !consent) {
      setError('请先同意屏幕识别用途');
      return;
    }
    setError('');
    dirty.current = true;
    setSources((all) =>
      all.map((source) => (source.package_name === package_name ? next : source)),
    );
    void saveSource(next);
  }
  async function saveSource(source: ScreenCaptureSource) {
    if (source.enabled && !consent) {
      setError('请先同意屏幕识别用途');
      return;
    }
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      await screenCaptureApi.save(source, consent);
      dirty.current = false;
      setMessage('屏幕识别来源已自动保存。');
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }
  async function start() {
    setBusy(true);
    setError('');
    try {
      await screenCaptureApi.start(consent);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }
  async function stop() {
    setBusy(true);
    try {
      await screenCaptureApi.stop();
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }
  return (
    <article className="screen-capture-card" aria-label="支付页面识别">
      <h3>支付页面识别</h3>
      <p>
        授权后在本机识别微信和支付宝支付成功页面。服务使用系统常驻通知；原始截图和 OCR
        原文不保存、不上传，识别结果只进入待确认收件箱。同额记录可能来自同一笔支付，请核对疑似重复提示。
      </p>
      <p role="status">
        {!status
          ? '正在读取屏幕识别状态…'
          : !status.android
            ? '当前设备不支持屏幕识别'
            : !status.usage_granted
              ? '尚未授予使用情况访问权限'
              : status.running
                ? '识别服务运行中，系统会显示常驻通知'
                : '识别服务未运行'}
      </p>
      <FormError message={error || loadError} />
      {status && results[status.code] && <p role="status">{results[status.code]}</p>}
      {latest.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <label className="notification-toggle">
        <input
          type="checkbox"
          checked={consent}
          onChange={(event) => setConsent(event.target.checked)}
        />
        我已了解屏幕捕获和本地识别用途
      </label>
      {sources.map((source) => (
        <fieldset className="screen-capture-source" key={source.package_name} disabled={busy}>
          <legend>{labels[source.package_name] ?? source.package_name}</legend>
          <label className="notification-toggle">
            <input
              type="checkbox"
              checked={source.enabled}
              onChange={(event) => update(source.package_name, { enabled: event.target.checked })}
            />
            启用识别
          </label>
          <label>
            默认记入平台
            <select
              value={source.account_id ?? ''}
              onChange={(event) =>
                update(source.package_name, { account_id: event.target.value || null })
              }
            >
              <option value="">请选择人民币平台</option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
        </fieldset>
      ))}
      <div className="screen-capture-actions">
        <button
          className="button button-secondary"
          disabled={busy || !status?.android}
          onClick={() =>
            void screenCaptureApi.usageSettings().catch((cause) => setError(errorText(cause)))
          }
        >
          打开使用情况访问设置
        </button>
        {status?.running ? (
          <button className="button button-secondary" disabled={busy} onClick={() => void stop()}>
            停止屏幕识别
          </button>
        ) : (
          <button
            className="button button-secondary"
            disabled={
              busy ||
              !status?.android ||
              !status.usage_granted ||
              !consent ||
              dirty.current ||
              !sources.some((s) => s.enabled)
            }
            onClick={() => void start()}
          >
            授权并开始识别
          </button>
        )}
      </div>
      {message && <p role="status">{message}</p>}
      <p className="field-hint">
        首次启动需允许本应用发送通知。使用情况访问权限用于判断当前是否为所选支付应用；切换到其他应用时暂停识别；锁屏、应用锁定、撤销权限或系统回收服务后，需重新授权。请在支付成功页面停留约两秒；时间默认使用识别时刻。
      </p>
    </article>
  );
}
