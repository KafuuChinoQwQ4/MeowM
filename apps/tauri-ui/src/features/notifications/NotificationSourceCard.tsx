import { useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import {
  notificationsApi,
  type CaptureStatus,
  type NotificationSource,
} from '../../api/notifications';
import { SelectField, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
const resultNames: Record<string, string> = {
  pending: '已加入待确认',
  needs_confirmation: '需补全信息后确认',
  duplicate: '重复通知，保留已有记录',
  ignored: '未识别为可入箱支付通知',
  error: '采集失败，请检查来源和系统状态',
};
export function NotificationSourceCard({
  source,
  latest,
  accounts,
  consent,
  onSaved,
}: {
  source: NotificationSource;
  latest?: CaptureStatus;
  accounts: Account[];
  consent: boolean;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useState(source);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [error, setError] = useState('');
  const name = source.package_name === 'com.tencent.mm' ? '微信' : '支付宝';
  async function save() {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await notificationsApi.save(draft, consent);
      onSaved();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return (
    <form
      className="notification-card"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <fieldset aria-label={`${name}通知来源`} disabled={busy}>
        <legend>{name}</legend>
        <p className="field-hint">{source.enabled ? '来源已启用' : '来源已关闭'}</p>
        <label className="notification-toggle">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(e) => setDraft((d) => ({ ...d, enabled: e.target.checked }))}
          />
          启用采集
        </label>
        <SelectField
          label="默认记入平台"
          value={draft.account_id ?? ''}
          options={[
            ['', '每次手动选择'],
            ...accounts.map((a) => [a.id, a.name] as [string, string]),
          ]}
          onChange={(e) => setDraft((d) => ({ ...d, account_id: e.target.value || null }))}
        />
        <p className="field-hint">
          {latest
            ? `最近结果：${resultNames[latest.code] ?? '结果未知'} · ${new Date(latest.received_at_ms).toLocaleString()}`
            : '尚无采集结果'}
        </p>
        <FormError message={error} />
        <button
          className="button button-primary"
          disabled={busy || (draft.enabled && !source.enabled && !consent)}
        >
          保存来源
        </button>
      </fieldset>
    </form>
  );
}
