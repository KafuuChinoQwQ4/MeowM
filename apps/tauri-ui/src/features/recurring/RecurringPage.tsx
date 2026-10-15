import { useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { recurringApi, type RecurringTemplate } from '../../api/recurring';
import { Dialog } from '../../components/Dialog';
import { FormError } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { errorText } from '../../shared/errors';
import { RecurringCard } from './RecurringCard';
import { RecurringForm } from './RecurringForm';
import { RecurringHistory } from './RecurringHistory';
import type { RecurringResource } from './useRecurring';
export function RecurringPage({
  accounts,
  state,
  onChanged,
}: {
  accounts: Account[];
  state: RecurringResource;
  onChanged: () => void;
}) {
  const [editing, setEditing] = useState<RecurringTemplate | 'new' | null>(null);
  const [history, setHistory] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [error, setError] = useState('');
  async function act(operation: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await operation();
      onChanged();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return (
    <div className="analytics-page">
      <div className="analytics-row">
        <h2>周期账单</h2>
        <button
          className="button button-primary"
          disabled={busy || !accounts.length}
          onClick={() => setEditing('new')}
        >
          新增周期账单
        </button>
      </div>
      <p className="field-hint">
        打开应用、返回前台或刷新时检查到期日；到期先进入待确认，不会直接扣款或改变余额。暂停不删除历史，恢复后补生成。
      </p>
      <button
        className="button button-secondary"
        disabled={busy || state.loading}
        onClick={state.reload}
      >
        {state.data?.has_more ? '继续生成到期候选' : '刷新周期账单'}
      </button>
      {state.data?.has_more && (
        <p className="field-hint">仍有到期项待处理，每次最多生成 200 个。</p>
      )}
      <LoadState {...state} />
      <FormError message={error} />
      {state.data?.templates.length === 0 && (
        <p className="analytics-empty">暂无周期账单。添加房租、订阅或工资计划。</p>
      )}
      {state.data?.templates.map((t) => (
        <RecurringCard
          key={t.id}
          template={t}
          accounts={accounts}
          busy={busy || state.loading}
          onEdit={() => setEditing(t)}
          onToggle={() => void act(() => recurringApi.enabled(t, !t.enabled))}
          onSkip={() => {
            if (window.confirm(`跳过 ${t.next_due_date} 的账单？不会生成候选，历史会保留。`))
              void act(() => recurringApi.skip(t));
          }}
          onHistory={() => setHistory(t.id)}
        />
      ))}
      {editing && (
        <Dialog
          title={editing === 'new' ? '新增周期账单' : '编辑未来账单'}
          onClose={() => setEditing(null)}
        >
          <RecurringForm
            template={editing === 'new' ? undefined : editing}
            accounts={accounts}
            onSaved={onChanged}
          />
        </Dialog>
      )}
      {history && (
        <Dialog title="周期实例" onClose={() => setHistory(null)}>
          <RecurringHistory id={history} />
        </Dialog>
      )}
    </div>
  );
}
