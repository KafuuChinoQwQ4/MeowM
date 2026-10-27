import { useCallback, useState } from 'react';
import { entriesApi, type PageEntry } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { sourceNames, kindNames } from '../../shared/labels';
import { today, moveDate } from '../../shared/format';
import { LoadState } from '../../components/LoadState';

function isDuplicateCandidate(entry: PageEntry, candidate: PageEntry) {
  if (candidate.id === entry.id) return false;
  if (
    entry.external_ids.some((incoming) =>
      candidate.external_ids.some(
        (saved) => incoming.source === saved.source && incoming.id === saved.id,
      ),
    )
  )
    return true;
  if (
    entry.external_ids.some((incoming) =>
      candidate.external_ids.some(
        (saved) => incoming.source === saved.source && incoming.id !== saved.id,
      ),
    )
  )
    return false;
  return (
    entry.amount !== null &&
    candidate.amount === entry.amount &&
    candidate.kind === entry.kind &&
    Boolean(entry.merchant?.trim()) &&
    candidate.merchant === entry.merchant &&
    Math.abs(Date.parse(candidate.occurred_at) - Date.parse(entry.occurred_at)) <= 5 * 60 * 1000
  );
}

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
      return page.items.filter((item) => isDuplicateCandidate(entry, item));
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
            : '未发现符合条件的疑似重复记录。'}
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
        。同平台不同交易号会排除；缺少可比交易号时，需商户、金额、类型一致且相隔不超过 5 分钟。
      </p>
      {open && <Matches entry={entry} />}
    </details>
  );
}
