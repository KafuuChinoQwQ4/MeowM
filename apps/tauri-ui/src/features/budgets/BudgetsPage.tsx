import { useCallback, useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { budgetsApi, type Budget } from '../../api/budgets';
import { Dialog } from '../../components/Dialog';
import { FormError } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { errorText } from '../../shared/errors';
import { useResource } from '../../shared/useResource';
import { BudgetCard } from './BudgetCard';
import { BudgetForm } from './BudgetForm';
export function BudgetsPage({
  accounts,
  revision,
  onChanged,
}: {
  accounts: Account[];
  revision: number;
  onChanged: () => void;
}) {
  const state = useResource(useCallback(() => budgetsApi.list(), [revision]));
  const [editing, setEditing] = useState<Budget | 'new' | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  async function archive(budget: Budget) {
    if (running.current || !window.confirm('停用此预算？历史进度会保留。')) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await budgetsApi.archive(budget.id);
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
        <h2>预算管理</h2>
        <button className="button button-primary" disabled={busy} onClick={() => setEditing('new')}>
          新增预算
        </button>
      </div>
      <p className="field-hint">按月份、分类或平台设置额度，也可以自定义日期。</p>
      <LoadState {...state} />
      <FormError message={error} />
      {state.data?.length === 0 && (
        <p className="analytics-empty">暂无预算。设置第一个预算，了解支出进度。</p>
      )}
      {state.data?.map((b) => (
        <BudgetCard
          key={b.id}
          budget={b}
          accounts={accounts}
          busy={busy}
          onEdit={() => setEditing(b)}
          onArchive={() => void archive(b)}
        />
      ))}
      {editing && (
        <Dialog
          title={editing === 'new' ? '新增预算' : '编辑预算'}
          onClose={() => setEditing(null)}
        >
          <BudgetForm
            budget={editing === 'new' ? undefined : editing}
            accounts={accounts}
            onSaved={onChanged}
          />
        </Dialog>
      )}
    </div>
  );
}
