import { useCallback, useState } from 'react';
import { displayAmount } from '../../shared/format';
import type { Account } from '../../api/accounts';
import { reconciliationApi } from '../../api/reconciliation';
import { SelectField } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { useResource } from '../../shared/useResource';
import { StartForm } from './StartForm';
import { SessionDetail } from './SessionDetail';
import { localTime, statusLabel } from './display';
export function ReconciliationPage({
  accounts,
  revision,
  onChanged,
  onReference,
  onPending,
  onEntry,
}: {
  accounts: Account[];
  revision: number;
  onChanged: () => void;
  onReference: (id: string) => void;
  onPending: () => void;
  onEntry: (id: string) => void;
}) {
  const [selected, setSelected] = useState(accounts[0]?.id ?? '');
  const accountId = accounts.some((a) => a.id === selected) ? selected : (accounts[0]?.id ?? '');
  return (
    <div className="analytics-page reconciliation-page">
      <h2>账户对账</h2>
      <p className="field-hint">
        逐项核实平台流水与实际余额。历史快照保留核对时的数据，之后记账不会改写它。
      </p>
      <SelectField
        label="对账平台"
        value={accountId}
        options={accounts.map((a) => [a.id, a.name])}
        onChange={(e) => setSelected(e.target.value)}
      />
      {accountId ? (
        <Workspace
          key={accountId}
          accountId={accountId}
          revision={revision}
          onChanged={onChanged}
          onReference={() => onReference(accountId)}
          onPending={onPending}
          onEntry={onEntry}
        />
      ) : (
        <p>请先创建平台。</p>
      )}
    </div>
  );
}
function Workspace({
  accountId,
  revision,
  onChanged,
  onReference,
  onPending,
  onEntry,
}: {
  accountId: string;
  revision: number;
  onChanged: () => void;
  onReference: () => void;
  onPending: () => void;
  onEntry: (id: string) => void;
}) {
  const [id, setId] = useState('');
  const loader = useCallback(() => reconciliationApi.list(accountId), [accountId, revision]);
  const sessions = useResource(loader);
  return (
    <>
      <StartForm
        accountId={accountId}
        onReference={onReference}
        onStarted={(id) => {
          setId(id);
          onChanged();
        }}
      />
      <div className="analytics-row">
        <h3>历史对账</h3>
        <button
          className="button button-secondary"
          disabled={sessions.loading}
          onClick={sessions.reload}
        >
          刷新历史
        </button>
      </div>
      <LoadState {...sessions} />
      {sessions.data?.length === 0 && <p className="field-hint">暂无对账记录。</p>}
      <div className="reconciliation-history">
        {sessions.data?.map((r) => (
          <button
            key={r.id}
            aria-pressed={id === r.id}
            className={'reconciliation-history-item' + (id === r.id ? ' selected' : '')}
            onClick={() => setId(r.id)}
          >
            <span>
              {r.date} · {statusLabel[r.status]}
            </span>
            <span>
              实际 ¥{displayAmount(r.actual)} · 差额 ¥{displayAmount(r.difference)}
            </span>
            <small>创建于 {localTime(r.created_at)}</small>
          </button>
        ))}
      </div>
      {id && (
        <SessionDetail
          key={id}
          id={id}
          revision={revision}
          onChanged={onChanged}
          onPending={onPending}
          onEntry={onEntry}
        />
      )}
    </>
  );
}
