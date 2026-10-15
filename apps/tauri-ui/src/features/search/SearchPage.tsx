import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import { entriesApi, type PageEntry, type PageQuery } from '../../api/entries';
import { searchApi } from '../../api/search';
import { Pager } from '../../components/Pager';
import { LoadState } from '../../components/LoadState';
import { FormError } from '../../components/Fields';
import { useResource } from '../../shared/useResource';
import { errorText } from '../../shared/errors';
import { ClassificationPage } from './ClassificationPage';
import { SearchFilters } from './SearchFilters';
import { SearchResults } from './SearchResults';
import { exportSearch } from './exportCsv';
export function SearchPage({
  accounts,
  revision,
  onOpen,
}: {
  accounts: Account[];
  revision: number;
  onOpen: (entry: PageEntry) => void;
}) {
  const [tab, setTab] = useState<'entries' | 'rules'>('entries');
  const [draft, setDraft] = useState<PageQuery>({ offset: 0, limit: 20 });
  const [query, setQuery] = useState<PageQuery>({ offset: 0, limit: 20 });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const state = useResource(useCallback(() => entriesApi.query(query), [query, revision]));
  const classification = useResource(useCallback(() => searchApi.state(), [revision]));
  async function download() {
    setBusy(true);
    setError('');
    try {
      await exportSearch(query);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="analytics-page">
      <div className="analytics-row">
        <h2>搜索与分类</h2>
        <div className="pending-actions">
          <button className="text-button" onClick={() => setTab('entries')}>
            明细搜索
          </button>
          <button className="text-button" onClick={() => setTab('rules')}>
            分类规则
          </button>
        </div>
      </div>
      {tab === 'rules' ? (
        <ClassificationPage state={classification} onChanged={classification.reload} />
      ) : (
        <>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              setQuery({ ...draft, offset: 0 });
            }}
          >
            <SearchFilters
              query={draft}
              accounts={accounts}
              categories={classification.data?.categories ?? []}
              onChange={setDraft}
            />
            <div className="pending-actions">
              <button className="button button-primary" type="submit">
                搜索流水
              </button>
              <button
                type="button"
                className="button button-secondary"
                onClick={() => {
                  const empty = { offset: 0, limit: 20 };
                  setDraft(empty);
                  setQuery(empty);
                }}
              >
                清除筛选
              </button>
              <button
                type="button"
                className="button button-secondary"
                disabled={busy || state.loading}
                onClick={() => void download()}
              >
                导出当前结果
              </button>
            </div>
          </form>
          <p className="field-hint">跨日期搜索全部流水；导出使用当前已提交的筛选条件。</p>
          <FormError message={error} />
          <LoadState {...state} />
          {state.data && (
            <>
              <p className="field-hint">共 {state.data.total} 条</p>
              <SearchResults items={state.data.items} onOpen={onOpen} />
              <Pager
                page={Math.floor(query.offset / query.limit)}
                pages={Math.ceil(state.data.total / query.limit)}
                onChange={(page) => setQuery((q) => ({ ...q, offset: page * q.limit }))}
              />
            </>
          )}
        </>
      )}
    </div>
  );
}
