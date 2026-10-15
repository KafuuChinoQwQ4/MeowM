import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import { budgetsApi, type Budget } from '../../api/budgets';
import { entriesApi } from '../../api/entries';
import { useFormAction } from '../../components/Dialog';
import { Field, SelectField, FormError } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { monthRange } from '../../shared/calendar';
import { today, validDate } from '../../shared/format';
import { useResource } from '../../shared/useResource';
export function BudgetForm({
  budget,
  accounts,
  onSaved,
}: {
  budget?: Budget;
  accounts: Account[];
  onSaved: () => void;
}) {
  const [custom, setCustom] = useState(budget?.custom_period ?? false);
  const [month, setMonth] = useState(budget?.from_date.slice(0, 7) ?? today().slice(0, 7));
  const defaults = monthRange(month);
  const [from, setFrom] = useState(budget?.from_date ?? defaults[0]);
  const [until, setUntil] = useState(budget?.until_date ?? defaults[1]);
  const [category, setCategory] = useState(budget?.category_id ?? '');
  const [account, setAccount] = useState(budget?.account_id ?? '');
  const [amount, setAmount] = useState(budget?.amount ?? '');
  const [enabled, setEnabled] = useState(budget?.enabled ?? true);
  const categories = useResource(useCallback(() => entriesApi.categories(), []));
  const action = useFormAction(onSaved);
  const categoryOptions: [string, string][] = [
    ['', '全部分类'],
    ...(categories.data ?? []).map((c) => [c.id, c.name] as [string, string]),
  ];
  if (budget?.category_id && !categoryOptions.some(([id]) => id === budget.category_id))
    categoryOptions.push([budget.category_id, budget.category_name + '（已停用，保留原范围）']);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const [start, end] = custom ? [from, until] : monthRange(month);
        if (!validDate(start) || !validDate(end) || start >= end) {
          action.setError('请选择有效日期范围');
          return;
        }
        if (!/^\d+(\.\d{1,2})?$/.test(amount)) {
          action.setError('额度必须为非负金额，最多两位小数');
          return;
        }
        void action.run(() =>
          budgetsApi.save({
            id: budget?.id,
            from_date: start,
            until_date: end,
            account_id: account || null,
            category_id: category || null,
            amount,
            enabled,
            custom_period: custom,
          }),
        );
      }}
    >
      <fieldset disabled={action.busy} className="budget-fields">
        <SelectField
          label="预算周期"
          value={custom ? 'custom' : 'month'}
          options={[
            ['month', '自然月'],
            ['custom', '自定义日期'],
          ]}
          onChange={(e) => setCustom(e.target.value === 'custom')}
        />
        {custom ? (
          <>
            <Field
              label="预算开始日期"
              type="date"
              required
              value={from}
              onChange={(e) => setFrom(e.target.value)}
            />
            <Field
              label="预算结束日期（不含）"
              type="date"
              required
              value={until}
              onChange={(e) => setUntil(e.target.value)}
            />
          </>
        ) : (
          <Field
            label="预算月份"
            type="month"
            required
            value={month}
            onChange={(e) => setMonth(e.target.value)}
          />
        )}
        <LoadState {...categories} />
        <SelectField
          label="预算分类"
          value={category}
          options={categoryOptions}
          disabled={categories.loading || !!categories.error}
          onChange={(e) => setCategory(e.target.value)}
        />
        <SelectField
          label="预算平台"
          value={account}
          options={[['', '全部平台'], ...accounts.map((a) => [a.id, a.name] as [string, string])]}
          onChange={(e) => setAccount(e.target.value)}
        />
        <Field
          label="预算额度（元）"
          inputMode="decimal"
          required
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          placeholder="0.00"
        />
        <SelectField
          label="预算状态"
          value={enabled ? 'on' : 'off'}
          options={[
            ['on', '启用'],
            ['off', '停用'],
          ]}
          onChange={(e) => setEnabled(e.target.value === 'on')}
        />
        <p className="field-hint">
          已确认支出抵减退款后计算用量，达到 80% 时提示。停用后保留预算及历史进度。
        </p>
        <FormError message={action.error} />
        <button
          className="button button-primary submit-button"
          disabled={categories.loading || !!categories.error}
          type="submit"
        >
          {action.busy ? '正在保存…' : '保存预算'}
        </button>
      </fieldset>
    </form>
  );
}
