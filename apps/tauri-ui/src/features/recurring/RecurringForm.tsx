import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import { entriesApi } from '../../api/entries';
import { recurringApi, type Cadence, type RecurringTemplate } from '../../api/recurring';
import { useFormAction } from '../../components/Dialog';
import { Field, FormError, SelectField } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { today, validDate } from '../../shared/format';
import { useResource } from '../../shared/useResource';
export function RecurringForm({
  template,
  accounts,
  onSaved,
}: {
  template?: RecurringTemplate;
  accounts: Account[];
  onSaved: () => void;
}) {
  const [account, setAccount] = useState(template?.account_id ?? accounts[0]?.id ?? '');
  const [kind, setKind] = useState<'income' | 'expense'>(template?.kind ?? 'expense');
  const [amount, setAmount] = useState(template?.amount ?? '');
  const [category, setCategory] = useState(template?.category_id ?? '');
  const [merchant, setMerchant] = useState(template?.merchant ?? '');
  const [description, setDescription] = useState(template?.description ?? '');
  const [cadence, setCadence] = useState<Cadence>(template?.cadence ?? 'monthly');
  const [interval, setInterval] = useState(String(template?.interval ?? 1));
  const [next, setNext] = useState(template?.next_due_date ?? today());
  const [end, setEnd] = useState(template?.end_date ?? '');
  const [enabled, setEnabled] = useState(template?.enabled ?? true);
  const categories = useResource(useCallback(() => entriesApi.categories(), []));
  const action = useFormAction(onSaved);
  const options: [string, string][] = [
    ['', '未分类'],
    ...(categories.data ?? []).map((c) => [c.id, c.name] as [string, string]),
  ];
  if (template?.category_id && !options.some(([id]) => id === template.category_id))
    options.push([template.category_id, template.category_name + '（已停用）']);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!/^\d+(\.\d{1,2})?$/.test(amount) || !/[1-9]/.test(amount)) {
          action.setError('金额必须大于零，最多两位小数');
          return;
        }
        if (
          !validDate(next) ||
          (end && (!validDate(end) || (!template && end < next))) ||
          !Number.isInteger(Number(interval)) ||
          Number(interval) < 1 ||
          Number(interval) > 120 ||
          !account
        ) {
          action.setError('请选择有效平台、日期及 1 至 120 的重复间隔');
          return;
        }
        void action.run(() =>
          recurringApi.save({
            id: template?.id,
            expected_version: template?.version,
            account_id: account,
            kind,
            amount,
            category_id: category || null,
            merchant,
            description,
            cadence,
            interval: Number(interval),
            next_due_date: next,
            end_date: end || null,
            enabled,
          }),
        );
      }}
    >
      <fieldset className="budget-fields" disabled={action.busy}>
        <SelectField
          label="周期平台"
          value={account}
          options={accounts.map((a) => [a.id, a.name])}
          onChange={(e) => setAccount(e.target.value)}
        />
        <SelectField
          label="周期类型"
          value={kind}
          options={[
            ['expense', '支出'],
            ['income', '收入'],
          ]}
          onChange={(e) => setKind(e.target.value as 'income' | 'expense')}
        />
        <Field
          label="周期金额（元）"
          inputMode="decimal"
          required
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
        <LoadState {...categories} />
        <SelectField
          label="周期分类"
          value={category}
          options={options}
          disabled={categories.loading || !!categories.error}
          onChange={(e) => setCategory(e.target.value)}
        />
        <Field
          label="周期商户"
          maxLength={100}
          value={merchant}
          onChange={(e) => setMerchant(e.target.value)}
        />
        <Field
          label="周期备注"
          maxLength={200}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
        <SelectField
          label="重复周期"
          value={cadence}
          options={[
            ['weekly', '每周'],
            ['monthly', '每月'],
            ['yearly', '每年'],
          ]}
          onChange={(e) => setCadence(e.target.value as Cadence)}
        />
        <Field
          label="重复间隔"
          type="number"
          min={1}
          max={120}
          step={1}
          required
          value={interval}
          onChange={(e) => setInterval(e.target.value)}
        />
        <Field
          label="下一次到期日"
          type="date"
          min="1900-01-01"
          max="9998-12-31"
          required
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
        <Field
          label="结束日期（含当天，可留空）"
          type="date"
          min="1900-01-01"
          max="9998-12-31"
          value={end}
          onChange={(e) => setEnd(e.target.value)}
        />
        <SelectField
          label="周期状态"
          value={enabled ? 'on' : 'off'}
          options={[
            ['on', '启用'],
            ['off', '暂停'],
          ]}
          onChange={(e) => setEnabled(e.target.value === 'on')}
        />
        <p className="field-hint">
          月底与闰日自动适配短月。修改仅影响后续账单，已有候选保持原样；改变日期或周期会以新的到期日重新起算。恢复后会补生成暂停期间的到期候选。
        </p>
        <FormError message={action.error} />
        <button
          className="button button-primary submit-button"
          type="submit"
          disabled={!accounts.length || categories.loading || !!categories.error}
        >
          {action.busy ? '正在保存…' : '保存周期账单'}
        </button>
      </fieldset>
    </form>
  );
}
