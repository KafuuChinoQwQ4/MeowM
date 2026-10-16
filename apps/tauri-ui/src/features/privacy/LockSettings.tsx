import { useState } from 'react';
import { privacyApi, type PrivacyStatus } from '../../api/privacy';
import { Field, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
export function LockSettings({
  status,
  onStatus,
}: {
  status: PrivacyStatus;
  onStatus: (v: PrivacyStatus) => void;
}) {
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  async function saveLock() {
    setError('');
    setMessage('');
    if (!status.lock_enabled && password !== confirm) {
      setError('两次密码不一致');
      return;
    }
    setBusy(true);
    try {
      onStatus(
        status.lock_enabled
          ? await privacyApi.disable(password)
          : await privacyApi.enable(password),
      );
      setMessage(status.lock_enabled ? '应用锁已关闭。' : '应用锁已启用，重启应用后默认锁定。');
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setPassword('');
      setConfirm('');
      setBusy(false);
    }
  }
  return (
    <section className="data-card" aria-label="应用锁与金额显示">
      <h3>应用锁与金额显示</h3>
      <p>应用锁密码仅在本机验证，遗失后无法找回。备份密码与应用锁密码互不通用。</p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void saveLock();
        }}
      >
        <fieldset disabled={busy}>
          <Field
            label={status.lock_enabled ? '当前应用锁密码' : '设置应用锁密码'}
            type="password"
            minLength={8}
            maxLength={256}
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {!status.lock_enabled && (
            <Field
              label="再次输入应用锁密码"
              type="password"
              minLength={8}
              maxLength={256}
              required
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          )}
          <button className="button button-primary" type="submit">
            {status.lock_enabled ? '关闭应用锁' : '启用应用锁'}
          </button>
        </fieldset>
      </form>
      {status.lock_enabled && (
        <div className="data-actions">
          <label>
            无操作后自动锁定
            <select
              value={status.auto_lock_minutes}
              disabled={busy}
              onChange={async (e) => {
                setBusy(true);
                setError('');
                try {
                  onStatus(await privacyApi.timeout(Number(e.target.value)));
                } catch (cause) {
                  setError(errorText(cause));
                } finally {
                  setBusy(false);
                }
              }}
            >
              {[1, 5, 15, 30, 60, 120].map((n) => (
                <option key={n} value={n}>
                  {n} 分钟
                </option>
              ))}
            </select>
          </label>
          <button
            className="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                onStatus(await privacyApi.lock());
              } catch (cause) {
                setError(errorText(cause));
              } finally {
                setBusy(false);
              }
            }}
          >
            立即锁定
          </button>
        </div>
      )}
      <label className="notification-toggle">
        <input
          type="checkbox"
          checked={status.hide_amounts}
          disabled={busy}
          onChange={async (e) => {
            setBusy(true);
            setError('');
            try {
              onStatus(await privacyApi.hide(e.target.checked));
            } catch (cause) {
              setError(errorText(cause));
            } finally {
              setBusy(false);
            }
          }}
        />
        隐藏金额
      </label>
      <p className="field-hint">隐藏金额只遮盖屏幕上的数值，不改变账本、导出或加密备份。</p>
      <FormError message={error} />
      {message && <p role="status">{message}</p>}
    </section>
  );
}
