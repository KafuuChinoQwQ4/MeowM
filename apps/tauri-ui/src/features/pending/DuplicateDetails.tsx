import { useCallback, useState } from 'react';
import { entriesApi, type PageEntry } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { sourceNames, kindNames } from '../../shared/labels';
import { today, moveDate } from '../../shared/format';
import { LoadState } from '../../components/LoadState';

function isDuplicateCandidate(entry: PageEntry, candidate: PageEntry) {
  if (
    candidate.id === entry.id ||
    candidate.reversal_id ||
    candidate.reversal_of ||
    candidate.status === 'ignored'
  )
    return false;
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
  const normalize = (value: string | null) =>
    (value ?? '').replace(/\s+/g, ' ').trim().toLowerCase();
  const prefix = (merchant: string | null, description: string | null) => {
    const name = normalize(merchant);
    const text = normalize(description);
    return (
      !!name &&
      (text === name || (text.startsWith(name) && /^[-－—·:： ]/.test(text.slice(name.length))))
    );
  };
  const sameMerchant =
    (!!normalize(entry.merchant) && normalize(entry.merchant) === normalize(candidate.merchant)) ||
    prefix(entry.merchant, candidate.description) ||
    prefix(candidate.merchant, entry.description) ||
    (!normalize(entry.merchant) &&
      !normalize(candidate.merchant) &&
      !!normalize(entry.description) &&
      normalize(entry.description) === normalize(candidate.description));
  const sameOrder = (entry.merchant_order_ids ?? []).some((incoming) =>
    (candidate.merchant_order_ids ?? []).some(
      (saved) => incoming.source === saved.source && incoming.id === saved.id,
    ),
  );
  const dateOnly = (item: PageEntry) => {
    const at = new Date(item.occurred_at);
    return (
      !item.external_ids.length &&
      at.getHours() === 12 &&
      at.getMinutes() === 0 &&
      at.getSeconds() === 0
    );
  };
  return (
    entry.amount !== null &&
    candidate.amount === entry.amount &&
    candidate.kind === entry.kind &&
    (entry.kind === 'transfer' ||
      (entry.account_id && candidate.account_id === entry.account_id)) &&
    (sameOrder || sameMerchant) &&
    (sameOrder ||
      dateOnly(entry) ||
      dateOnly(candidate) ||
      Math.abs(Date.parse(candidate.occurred_at) - Date.parse(entry.occurred_at)) <= 5 * 60 * 1000)
  );
}

function Matches({ entry }: { entry: PageEntry }) {
  const state = useResource(
    useCallback(async () => {
      const d = new Date(entry.occurred_at);
      const from = Number.isNaN(d.getTime())
        ? today()
        : `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
      const matches: PageEntry[] = [];
      let offset = 0;
      for (;;) {
        const page = await entriesApi.query({
          from_date: from,
          until_date: moveDate(from, 1),
          offset,
          limit: 200,
        });
        matches.push(...page.items.filter((item) => isDuplicateCandidate(entry, item)));
        offset += page.items.length;
        if (!page.items.length || offset >= page.total) break;
      }
      return matches;
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
        。同渠道不同交易单号会排除，商户单号辅助核对。缺少交易单号时，按平台、金额、类型、商户或商品中的店名和时间提示；仅记录日期的手动账单按当天核对。请核对后决定。
      </p>
      {entry.external_ids.map((order) => (
        <p className="field-hint" key={`${order.source}:${order.id}`}>
          渠道交易单号：{order.id}
        </p>
      ))}
      {(entry.merchant_order_ids ?? []).map((order) => (
        <p className="field-hint" key={`${order.source}:${order.id}`}>
          商户单号：{order.id}
        </p>
      ))}
      {open && <Matches entry={entry} />}
    </details>
  );
}
