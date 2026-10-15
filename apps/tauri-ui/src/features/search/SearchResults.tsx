import type { PageEntry } from '../../api/entries';
import { money } from '../../shared/format';
import { kindNames, sourceNames } from '../../shared/labels';
export function SearchResults({
  items,
  onOpen,
}: {
  items: PageEntry[];
  onOpen: (entry: PageEntry) => void;
}) {
  return (
    <div className="search-results">
      {items.map((entry) => (
        <article className="pending-card" key={entry.id}>
          <div className="pending-top">
            <strong>
              {entry.merchant || entry.description || kindNames[entry.kind] || entry.kind}
            </strong>
            <strong>¥ {money(entry.amount ?? '0')}</strong>
          </div>
          <p className="field-hint">
            {new Date(entry.occurred_at).toLocaleString()} ·{' '}
            {entry.status === 'confirmed'
              ? '已确认'
              : entry.status === 'pending'
                ? '待确认'
                : '已忽略'}{' '}
            · {entry.sources.map((s) => sourceNames[s] ?? s).join('、')}
          </p>
          {entry.suggestion && <p className="field-hint">建议分类：{entry.suggestion.reason}</p>}
          <button className="text-button" onClick={() => onOpen(entry)}>
            查看详情
          </button>
        </article>
      ))}
      {!items.length && <p className="analytics-empty">没有符合条件的流水。</p>}
    </div>
  );
}
