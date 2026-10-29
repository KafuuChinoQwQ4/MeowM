import { useCallback, useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { entriesApi, type PageEntry, type PendingEdit } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { useFormAction } from '../../components/Dialog';
import { Field, SelectField, FormError } from '../../components/Fields';
import { kindNames } from '../../shared/labels';
import { validDate, validAmount, money } from '../../shared/format';
export function PendingEditor({
  entry,
  accounts,
  accountId,
  onSaved,
}: {
  entry: PageEntry;
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
}) {
  const occurred = new Date(entry.occurred_at);
  const matchedOriginal = entry.refund_matches?.find(
    (match) => match.confidence === 'exact' && match.status === 'confirmed',
  )?.id;
  // A preview becomes a stored link only on confirmation or an explicit choice.
  const automaticLink = useRef(!entry.original_transaction_id && !!matchedOriginal);
  const [draft, setDraft] = useState<PendingEdit>({
    id: entry.id,
    expected_version: entry.edit_version,
    account_id: entry.account_id ?? accountId,
    date: `${occurred.getFullYear()}-${String(occurred.getMonth() + 1).padStart(2, '0')}-${String(occurred.getDate()).padStart(2, '0')}`,
    amount: entry.amount ?? '',
    kind: entry.kind,
    destination_id: entry.destination_id ?? null,
    merchant: entry.merchant ?? '',
    description: entry.description ?? '',
    category_id: entry.category_id,
    original_transaction_id: entry.original_transaction_id ?? matchedOriginal ?? null,
  });
  const categories = useResource(useCallback(() => entriesApi.categories(), []));
  const [originalQuery, setOriginalQuery] = useState({ keyword: '', page: 0 });
  const originals = useResource(
    useCallback(
      () =>
        draft.kind === 'refund'
          ? entriesApi.query({
              status: 'confirmed',
              keyword: originalQuery.keyword.trim() || undefined,
              offset: originalQuery.page * 20,
              limit: 20,
            })
          : Promise.resolve(null),
      [draft.kind, originalQuery],
    ),
  );
  const action = useFormAction(onSaved);
  function update(key: keyof PendingEdit, value: string) {
    const clearAutomatic =
      automaticLink.current && ['account_id', 'date', 'amount', 'kind'].includes(key);
    if (clearAutomatic || key === 'original_transaction_id') automaticLink.current = false;
    setDraft((d) => ({
      ...d,
      [key]: value,
      ...(clearAutomatic ? { original_transaction_id: null } : {}),
    }));
  }
  function save(confirm: boolean) {
    if (!validAmount(draft.amount.trim()) || !validDate(draft.date)) {
      action.setError('请输入有效正金额和日期');
      return;
    }
    const edit = {
      ...draft,
      amount: draft.amount.trim(),
      merchant: draft.merchant.trim(),
      description: draft.description.trim(),
      category_id: categories.data ? draft.category_id || null : entry.category_id,
      original_transaction_id:
        draft.kind === 'refund' && (confirm || !automaticLink.current)
          ? draft.original_transaction_id?.trim() || null
          : null,
    };
    void action.run(() =>
      confirm ? entriesApi.confirm([entry.id], [edit]) : entriesApi.edit(edit),
    );
  }
  const options: [string, string][] = [
    ['', '未分类'],
    ...(categories.data ?? []).map((c) => [c.id, c.name] as [string, string]),
  ];
  if (draft.category_id && !options.some(([id]) => id === draft.category_id))
    options.push([draft.category_id, '当前分类']);
  const originalOptions: [string, string][] = [
    ['', '不关联原消费（直接记账）'],
    ...(originals.data?.items ?? [])
      .filter((item) => item.kind === 'expense' && !item.reversal_of && !item.reversal_id)
      .map(
        (item) =>
          [
            item.id,
            `${new Date(item.occurred_at).toLocaleString()} · ${item.merchant || '消费'} · ¥ ${money(item.amount ?? '0')} · ${accounts.find((account) => account.id === item.account_id)?.name ?? '其他平台'}${item.description ? ` · ${item.description}` : ''}${item.external_ids?.[0] ? ` · 订单 ${item.external_ids[0].id}` : ''}`,
          ] as [string, string],
      ),
  ];
  for (const match of entry.refund_matches ?? []) {
    if (match.status === 'confirmed' && !originalOptions.some(([id]) => id === match.id))
      originalOptions.push([
        match.id,
        `${new Date(match.occurred_at).toLocaleDateString()} · ${match.merchant || '消费'} · ¥ ${money(match.amount ?? '0')}（匹配建议）`,
      ]);
  }
  if (
    draft.original_transaction_id &&
    !originalOptions.some(([id]) => id === draft.original_transaction_id)
  )
    originalOptions.push([draft.original_transaction_id, '已关联的原消费（不在当前页）']);
  const originalPages = Math.max(1, Math.ceil((originals.data?.total ?? 0) / 20));
  return (
    <form
      className="sheet-form"
      onSubmit={(e) => {
        e.preventDefault();
        save(true);
      }}
    >
      <fieldset disabled={action.busy}>
        <SelectField
          label="记入平台"
          options={accounts.map((a) => [a.id, a.name])}
          value={draft.account_id}
          onChange={(e) => update('account_id', e.target.value)}
        />
        <Field
          label="日期"
          type="date"
          value={draft.date}
          onChange={(e) => update('date', e.target.value)}
        />
        <Field
          label="金额（元）"
          value={draft.amount}
          onChange={(e) => update('amount', e.target.value)}
        />
        <SelectField
          label="类型"
          options={Object.entries(kindNames).filter(([id]) =>
            ['expense', 'income', 'refund', 'transfer'].includes(id),
          )}
          value={draft.kind}
          onChange={(e) => update('kind', e.target.value)}
        />
        {draft.kind === 'transfer' && (
          <>
            <SelectField
              label="转入平台"
              value={draft.destination_id ?? ''}
              options={[
                ['', '请选择转入平台'],
                ...accounts.map((a) => [a.id, a.name] as [string, string]),
              ]}
              onChange={(e) => update('destination_id', e.target.value)}
            />
            <p className="field-hint">
              同平台内部划转可选择相同平台；保留划转记录，平台总余额和收支不变。
            </p>
          </>
        )}
        <Field
          label="商户"
          maxLength={100}
          value={draft.merchant}
          onChange={(e) => update('merchant', e.target.value)}
        />
        <Field
          label="备注"
          maxLength={200}
          value={draft.description}
          onChange={(e) => update('description', e.target.value)}
        />
        <SelectField
          label="分类"
          options={options}
          disabled={!categories.data}
          value={draft.category_id ?? ''}
          onChange={(e) => update('category_id', e.target.value)}
        />
        {entry.suggestion &&
          !entry.category_id &&
          categories.data?.some((c) => c.id === entry.suggestion?.category_id) && (
            <div className="field-hint">
              建议分类：{categories.data.find((c) => c.id === entry.suggestion?.category_id)?.name}{' '}
              · {entry.suggestion.reason}
              <button
                type="button"
                className="text-button"
                onClick={() => update('category_id', entry.suggestion!.category_id)}
              >
                采用建议
              </button>
            </div>
          )}
        {categories.error && (
          <>
            <FormError message={categories.error} />
            <button type="button" className="button button-secondary" onClick={categories.reload}>
              重试分类
            </button>
          </>
        )}
        {draft.kind === 'refund' && (
          <>
            <p className="field-hint">
              原消费关联可选。未关联也可直接确认，退款按发生日期抵减净支出。
            </p>
            {(entry.refund_matches ?? []).map((match) => (
              <div className="field-hint" key={match.id}>
                <p>
                  {match.confidence === 'exact' ? '原订单号匹配' : '疑似原消费'}：
                  {new Date(match.occurred_at).toLocaleDateString()} · {match.merchant || '消费'} ·
                  ¥ {money(match.amount ?? '0')}
                  {' · '}
                  {match.reason}
                </p>
                {match.status === 'confirmed' ? (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => update('original_transaction_id', match.id)}
                  >
                    关联这笔消费
                  </button>
                ) : (
                  <p>
                    原消费尚待确认。可先直接确认退款，或返回列表同时勾选原消费；原订单号匹配时会自动关联。
                  </p>
                )}
              </div>
            ))}
            <Field
              label="搜索原消费（商户、备注或订单号）"
              maxLength={200}
              value={originalQuery.keyword}
              onChange={(e) => setOriginalQuery({ keyword: e.target.value, page: 0 })}
            />
            <SelectField
              label="退款原消费"
              value={draft.original_transaction_id ?? ''}
              options={originalOptions}
              disabled={originals.loading || !!originals.error}
              onChange={(e) => update('original_transaction_id', e.target.value)}
            />
            {originals.loading && <p className="field-hint">正在加载原消费…</p>}
            {originals.error && (
              <>
                <FormError message={originals.error} />
                <button
                  type="button"
                  className="button button-secondary"
                  onClick={originals.reload}
                >
                  重试原消费
                </button>
              </>
            )}
            {originals.data && (
              <>
                {originalOptions.length === 1 && (
                  <p className="field-hint">
                    本页没有可选的已确认消费，可直接确认退款，也可翻页或更换关键词查找。
                  </p>
                )}
                <div className="pending-actions">
                  <button
                    type="button"
                    className="button button-secondary"
                    aria-label="上一页原消费"
                    disabled={originalQuery.page === 0}
                    onClick={() =>
                      setOriginalQuery((query) => ({ ...query, page: query.page - 1 }))
                    }
                  >
                    上一页
                  </button>
                  <span>
                    原消费 {originalQuery.page + 1} / {originalPages} 页
                  </span>
                  <button
                    type="button"
                    className="button button-secondary"
                    aria-label="下一页原消费"
                    disabled={originalQuery.page >= originalPages - 1}
                    onClick={() =>
                      setOriginalQuery((query) => ({ ...query, page: query.page + 1 }))
                    }
                  >
                    下一页
                  </button>
                </div>
              </>
            )}
          </>
        )}
        <button type="button" className="button button-secondary" onClick={() => save(false)}>
          仅保存修改
        </button>
      </fieldset>
      <FormError message={action.error} />
      <button className="button button-primary submit-button" disabled={action.busy}>
        保存并确认
      </button>
    </form>
  );
}
