import { useCallback, useState } from 'react';
import { entriesApi, type PageEntry } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { sourceNames, kindNames } from '../../shared/labels';
import { today, moveDate } from '../../shared/format';
import { LoadState } from '../../components/LoadState';
function Matches({ entry }: { entry: PageEntry }) {
  const state = useResource(
    useCallback(async () => {
      const d = new Date(entry.occurred_at);
      const from = Number.isNaN(d.getTime())
        ? today()
        : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
      const page = await entriesApi.query({
        from_date: from,
        until_date: moveDate(from, 1),
        offset: 0,
        limit: 200,
      });
      return page.items.filter((item) => item.id !== entry.id && item.amount === entry.amount);
    }, [entry]),
  );
  return (
    <>
      <LoadState {...state} />
      {state.data && (
        <p className="field-hint">
          {state.data.length
            ? state.data
                .map(
                  (item) =>
                    `${item.merchant || item.description || kindNames[item.kind]} · ${item.status} · ${item.id}`,
                )
                .join('\n')
            : '未发现同日同金额候选。'}
        </p>
      )}
    </>
  );
}
export function DuplicateDetails({ entry }: { entry: PageEntry }) {
  const [open, setOpen] = useState(false);
  return (
    <details className="duplicate-details" onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>查看来源 / 疑似重复</summary>
      <p className="field-hint">
        来源：{entry.sources.map((source) => sourceNames[source] ?? source).join('、') || '未知'}
        。仅金额和日期相同不会自动合并。
      </p>
      {open && <Matches entry={entry} />}
    </details>
  );
}
