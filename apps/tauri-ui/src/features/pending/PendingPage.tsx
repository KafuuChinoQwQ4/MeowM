import { useCallback, useEffect, useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { entriesApi, type PageQuery, type PageEntry } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { money } from '../../shared/format';
import { sourceNames, kindNames } from '../../shared/labels';
import { errorText } from '../../shared/errors';
import { LoadState } from '../../components/LoadState';
import { FormError } from '../../components/Fields';
import { Pager } from '../../components/Pager';
import { PendingFilters } from './PendingFilters';
import { DuplicateDetails } from './DuplicateDetails';
export function PendingPage({
  accounts,
  initialSource,
  revision,
  onEdit,
  onImport,
  onChanged,
}: {
  accounts: Account[];
  initialSource?: PageQuery['source'];
  revision: number;
  onEdit: (entry: PageEntry) => void;
  onImport: () => void;
  onChanged: () => void;
}) {
  const [query, setQuery] = useState<PageQuery>({
    status: 'pending',
    source: initialSource,
    offset: 0,
    limit: 20,
  });
  const state = useResource(useCallback(() => entriesApi.query(query), [query, revision]));
  const [selected, setSelected] = useState(new Set<string>());
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [error, setError] = useState('');
  useEffect(() => setSelected(new Set()), [state.data]);
  const items = state.data?.items ?? [];
  function select(id: string, checked: boolean) {
    setSelected((old) => {
      const next = new Set(old);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  }
  async function act(operation: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await operation();
      setSelected(new Set());
      onChanged();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return (
    <>
      <div className="platform-heading">
        <h2>待确认收件箱</h2>
        <button className="button button-primary" disabled={busy} onClick={onImport}>
          导入账单
        </button>
      </div>
      <p className="field-hint">导入、通知和周期候选仅在确认后计入余额与报表。</p>
      <button
        className="button button-secondary"
        disabled={busy || state.loading}
        onClick={state.reload}
      >
        刷新待确认记录
      </button>
      <PendingFilters accounts={accounts} query={query} onChange={setQuery} disabled={busy} />
      <LoadState {...state} />
      <FormError message={error} />
      {state.data && (
        <>
          <div className="pending-actions">
            <input
              type="checkbox"
              aria-label="选择本页全部记录"
              disabled={busy}
              checked={items.length > 0 && items.every((item) => selected.has(item.id))}
              onChange={(e) =>
                setSelected(new Set(e.target.checked ? items.map((item) => item.id) : []))
              }
            />
            <span>
              已选 {selected.size} / 共 {state.data.total} 条
            </span>
            {query.status === 'pending' && (
              <>
                <button
                  className="button button-primary"
                  disabled={busy || !selected.size}
                  onClick={() => void act(() => entriesApi.confirm([...selected]))}
                >
                  批量确认
                </button>
                <button
                  className="button button-secondary"
                  disabled={busy || !selected.size}
                  onClick={() => void act(() => entriesApi.ignore([...selected]))}
                >
                  批量忽略
                </button>
              </>
            )}
          </div>
          {items.map((entry) => (
            <article className="pending-card" key={entry.id}>
              <div className="pending-top">
                <input
                  type="checkbox"
                  aria-label={`选择 ${entry.merchant || entry.description || entry.id}`}
                  disabled={busy}
                  checked={selected.has(entry.id)}
                  onChange={(e) => select(entry.id, e.target.checked)}
                />
                <strong>
                  {entry.merchant || entry.description || kindNames[entry.kind] || entry.kind}
                </strong>
                <strong>¥ {money(entry.amount ?? '0')}</strong>
              </div>
              <p className="field-hint">
                {kindNames[entry.kind] ?? entry.kind} ·{' '}
                {new Date(entry.occurred_at).toLocaleDateString()} ·{' '}
                {entry.sources.map((source) => sourceNames[source] ?? source).join('、')} ·{' '}
                {query.status === 'pending' ? '待确认' : '已忽略'}
              </p>
              {entry.suggestion && (
                <p className="field-hint">
                  分类建议：{entry.suggestion.reason}。打开编辑后可选择采用。
                </p>
              )}
              <div className="pending-actions">
                {query.status === 'pending' ? (
                  <>
                    <button className="text-button" disabled={busy} onClick={() => onEdit(entry)}>
                      编辑 / 确认
                    </button>
                    <button
                      className="text-button"
                      disabled={busy}
                      onClick={() => void act(() => entriesApi.ignore([entry.id]))}
                    >
                      忽略
                    </button>
                  </>
                ) : (
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() => void act(() => entriesApi.restore(entry.id))}
                  >
                    恢复待确认
                  </button>
                )}
              </div>
              <DuplicateDetails entry={entry} />
            </article>
          ))}
          {!items.length && <p className="empty-platforms">没有符合条件的记录</p>}
          <Pager
            page={Math.floor(query.offset / 20)}
            pages={Math.ceil(state.data.total / 20)}
            disabled={busy}
            onChange={(page) => setQuery((q) => ({ ...q, offset: page * 20 }))}
          />
        </>
      )}
    </>
  );
}
