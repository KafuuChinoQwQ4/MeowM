import { useState } from 'react';
import type { Entry } from '../../api/ledger';
import { Pager } from '../../components/Pager';
import { kindNames } from '../../shared/labels';
import { money } from '../../shared/format';
import { usePageSize } from '../../shared/usePageSize';
export function RecordsPage({
  entries,
  onOpen,
}: {
  entries: Entry[];
  onOpen: (id: string) => void;
}) {
  const [requested, setPage] = useState(0);
  const size = usePageSize();
  const pages = Math.max(1, Math.ceil(entries.length / size));
  const page = Math.min(requested, pages - 1);
  return (
    <section className="records">
      <div className="records-heading">
        <h2>
          当日明细<span className="count-badge">{entries.length}</span>
        </h2>
      </div>
      {!entries.length ? (
        <div className="empty-state entries-empty">
          <span className="empty-art">≋</span>
          <h3>暂无记录</h3>
        </div>
      ) : (
        <>
          <ul className="entry-list">
            {entries.slice(page * size, (page + 1) * size).map((entry) => {
              const income = !entry.amount.startsWith('-');
              const label = kindNames[entry.kind] ?? entry.kind;
              return (
                <li
                  key={entry.id}
                  className="entry"
                  tabIndex={0}
                  role="button"
                  onClick={() => onOpen(entry.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onOpen(entry.id);
                    }
                  }}
                >
                  <span
                    aria-hidden="true"
                    className={'entry-icon ' + (income ? 'income' : 'expense')}
                  >
                    {income ? '↙' : '↗'}
                  </span>
                  <div className="entry-body">
                    <h3>{entry.note || label}</h3>
                    <p>
                      {label} · {entry.time}
                    </p>
                  </div>
                  <span className={'entry-amount ' + (income ? 'income-text' : '')}>
                    {income ? '+' : '−'} ¥{money(entry.amount.replace(/^-/, ''))}
                  </span>
                </li>
              );
            })}
          </ul>
          <Pager page={page} pages={pages} onChange={setPage} />
        </>
      )}
    </section>
  );
}
