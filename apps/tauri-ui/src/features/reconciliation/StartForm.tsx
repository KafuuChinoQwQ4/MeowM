import { useRef, useState } from 'react';
import { reconciliationApi } from '../../api/reconciliation';
import { Field, FormError } from '../../components/Fields';
import { today } from '../../shared/format';
import { errorText } from '../../shared/errors';
export function StartForm({
  accountId,
  onStarted,
  onReference,
}: {
  accountId: string;
  onStarted: (id: string) => void;
  onReference: () => void;
}) {
  const [date, setDate] = useState(today);
  const [actual, setActual] = useState('');
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [error, setError] = useState('');
  return (
    <form
      className="reconciliation-start"
      onSubmit={async (e) => {
        e.preventDefault();
        if (running.current) return;
        running.current = true;
        setBusy(true);
        setError('');
        try {
          const s = await reconciliationApi.start(accountId, date, actual);
          onStarted(s.record.id);
        } catch (cause) {
          setError(errorText(cause));
        } finally {
          running.current = false;
          setBusy(false);
        }
      }}
    >
      <h3>新建对账</h3>
      <p className="field-hint">
        以本地截止日期当日结束时的余额为准，支持负余额。请先为平台设置参考余额。
      </p>
      <Field
        label="截止日期"
        type="date"
        required
        value={date}
        disabled={busy}
        onChange={(e) => setDate(e.target.value)}
      />
      <Field
        label="实际余额（元）"
        type="text"
        inputMode="text"
        required
        maxLength={24}
        value={actual}
        placeholder="例如 -100.00"
        disabled={busy}
        onChange={(e) => setActual(e.target.value)}
      />
      <FormError message={error} />
      <div className="reconciliation-actions">
        <button className="button button-primary" disabled={busy || !accountId}>
          {busy ? '保存快照中…' : '开始对账'}
        </button>
        <button
          className="button button-secondary"
          type="button"
          disabled={busy}
          onClick={onReference}
        >
          去设置参考余额
        </button>
      </div>
    </form>
  );
}
