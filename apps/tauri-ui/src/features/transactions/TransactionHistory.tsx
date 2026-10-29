import type { Account } from '../../api/accounts';
import type { CategoryOption } from '../../api/entries';
import type { TransactionDetail } from '../../api/transactions';
import { money } from '../../shared/format';
import { useDialog } from '../../components/Dialog';
export function TransactionHistory({
  detail,
  accounts,
  categories,
  onLink,
}: {
  detail: TransactionDetail;
  accounts: Account[];
  categories: CategoryOption[];
  onLink: (id: string) => void;
}) {
  const { busy } = useDialog();
  const tx = detail.transaction;
  const links = [
    ...new Set([
      ...detail.related_ids,
      ...[tx.original_transaction_id, detail.reversal_of, detail.reversal_id].filter(
        (id): id is string => !!id,
      ),
    ]),
  ];
  return (
    <>
      <p>
        {tx.date} · {tx.status} · {tx.kind} · ¥ {money(tx.amount)}
      </p>
      <p>账户：{accounts.find((a) => a.id === tx.account_id)?.name ?? tx.account_id}</p>
      <p>
        分类：{categories.find((c) => c.id === tx.category_id)?.name ?? '未分类'} · 商户：
        {tx.merchant ?? '—'} · 备注：{tx.description ?? '—'}
      </p>
      <section>
        <h3>关联记录</h3>
        {links.length ? (
          links.map((id) => (
            <button
              key={id}
              className="button button-secondary"
              disabled={busy}
              onClick={() => onLink(id)}
            >
              打开关联记录 {id.slice(0, 8)}
            </button>
          ))
        ) : (
          <p>没有关联记录</p>
        )}
      </section>
      <section>
        <h3>来源</h3>
        {detail.sources.map((source, i) => (
          <p key={i}>
            {source.source}
            {source.external_id && ` · ${source.external_id}`}
            {source.merchant_order_id && ` · 商户单号 ${source.merchant_order_id}`}
            {source.digest && ` · 摘要 ${source.digest}`}
          </p>
        ))}
      </section>
      <section>
        <h3>变更历史</h3>
        {detail.history.map((event) => (
          <p key={event.id}>
            {event.action} · {event.time} · {event.detail}
          </p>
        ))}
      </section>
    </>
  );
}
