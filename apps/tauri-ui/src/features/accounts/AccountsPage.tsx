import { useState } from 'react';
import type { Account } from '../../api/accounts';
import { Pager } from '../../components/Pager';
import { PlatformBadge } from '../../components/PlatformBadge';
import { platformCategories } from '../../shared/labels';
import { usePageSize } from '../../shared/usePageSize';
export function AccountsPage({
  accounts,
  onEdit,
  onEnter,
}: {
  accounts: Account[];
  onEdit: (account?: Account) => void;
  onEnter: (id: string) => void;
}) {
  const [filter, setFilter] = useState('');
  const [requestedPage, setPage] = useState(0);
  const size = usePageSize(270, 84);
  const filtered = accounts.filter((a) => !filter || a.category === filter);
  const pages = Math.max(1, Math.ceil(filtered.length / size));
  const page = Math.min(requestedPage, pages - 1);
  return (
    <>
      <div className="platform-heading">
        <h2>平台管理</h2>
        <button className="button button-primary" onClick={() => onEdit()}>
          ＋ 新建平台
        </button>
      </div>
      <select
        className="platform-filter"
        aria-label="筛选平台分类"
        value={filter}
        onChange={(e) => {
          setFilter(e.target.value);
          setPage(0);
        }}
      >
        <option value="">全部分类</option>
        {Object.entries(platformCategories).map(([value, text]) => (
          <option key={value} value={value}>
            {text}
          </option>
        ))}
      </select>
      <div className="platform-list">
        {filtered.slice(page * size, (page + 1) * size).map((account) => (
          <div className="platform-row" key={account.id}>
            <div className="platform-info">
              <PlatformBadge name={account.name} category={account.category} />
              <span>{platformCategories[account.category] ?? '其他'}</span>
            </div>
            <div className="platform-row-actions">
              <button className="text-button" onClick={() => onEdit(account)}>
                编辑
              </button>
              <button className="text-button" onClick={() => onEnter(account.id)}>
                进入
              </button>
            </div>
          </div>
        ))}
        {!filtered.length && <p className="empty-platforms">暂无平台</p>}
      </div>
      <Pager page={page} pages={pages} onChange={setPage} />
    </>
  );
}
