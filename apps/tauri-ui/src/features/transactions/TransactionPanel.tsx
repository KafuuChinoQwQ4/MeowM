import { useCallback, useState } from 'react';
import { accountsApi } from '../../api/accounts';
import { entriesApi } from '../../api/entries';
import { transactionsApi } from '../../api/transactions';
import { useResource } from '../../shared/useResource';
import { LoadState } from '../../components/LoadState';
import { useFormAction } from '../../components/Dialog';
import { FormError } from '../../components/Fields';
import { TransactionHistory } from './TransactionHistory';
import { TransactionEditor } from './TransactionEditor';
export function TransactionPanel({
  id,
  accountId,
  date,
  onLink,
  onSaved,
}: {
  id?: string;
  accountId: string;
  date: string;
  onLink: (id: string) => void;
  onSaved: () => void;
}) {
  const state = useResource(
    useCallback(async () => {
      const [accounts, categories, detail] = await Promise.all([
        accountsApi.list(),
        entriesApi.categories(),
        id ? transactionsApi.detail(id) : Promise.resolve(null),
      ]);
      return { accounts, categories, detail };
    }, [id]),
  );
  const [editing, setEditing] = useState(false);
  const action = useFormAction(onSaved);
  if (!state.data) return <LoadState {...state} />;
  const { accounts, categories, detail } = state.data;
  const canEdit =
    !!detail &&
    detail.transaction.status === 'confirmed' &&
    !detail.reversal_id &&
    !detail.reversal_of;
  return (
    <div className="sheet-form">
      {detail && (
        <TransactionHistory
          detail={detail}
          accounts={accounts}
          categories={categories}
          onLink={(link) => {
            setEditing(false);
            onLink(link);
          }}
        />
      )}
      {detail && <p className="sheet-description">编辑会按版本号保存。导入来源字段保持不变。</p>}
      {(!detail || (canEdit && editing)) && (
        <TransactionEditor
          key={id ?? 'new'}
          detail={detail}
          accounts={accounts}
          categories={categories}
          accountId={accountId}
          date={date}
          onSaved={onSaved}
        />
      )}
      {canEdit && (
        <>
          <button
            type="button"
            className="button button-secondary"
            disabled={action.busy}
            onClick={() => setEditing(true)}
          >
            编辑交易
          </button>
          <button
            type="button"
            className="button button-danger"
            disabled={action.busy}
            onClick={() => {
              const reason = window.prompt('填写撤销原因');
              if (!reason?.trim()) return;
              if (!window.confirm('确认撤销？原交易会保留，并创建可追溯的反向记录。')) return;
              void action.run(() => transactionsApi.reverse(detail.transaction.id, reason.trim()));
            }}
          >
            撤销交易
          </button>
        </>
      )}
      {detail && !canEdit && <p className="field-hint">此记录不可编辑。</p>}
      <FormError message={action.error} />
    </div>
  );
}
