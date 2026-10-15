import type { Account } from '../../api/accounts';
import type { CategoryOption } from '../../api/entries';
import { Field, SelectField } from '../../components/Fields';
export interface Filters {
  period: string;
  month: string;
  from: string;
  until: string;
  account: string;
  category: string;
  merchant: string;
}
export function ReportFilters({
  value,
  accounts,
  categories,
  onChange,
}: {
  value: Filters;
  accounts: Account[];
  categories: CategoryOption[];
  onChange: (value: Filters) => void;
}) {
  const set = (key: keyof Filters, next: string) => onChange({ ...value, [key]: next });
  return (
    <div className="report-filters">
      <SelectField
        label="报表周期"
        value={value.period}
        options={[
          ['month', '自然月'],
          ['custom', '自定义日期'],
        ]}
        onChange={(e) => set('period', e.target.value)}
      />
      {value.period === 'month' ? (
        <Field
          label="报表月份"
          type="month"
          value={value.month}
          onChange={(e) => set('month', e.target.value)}
        />
      ) : (
        <>
          <Field
            label="开始日期"
            type="date"
            value={value.from}
            onChange={(e) => set('from', e.target.value)}
          />
          <Field
            label="结束日期（不含）"
            type="date"
            value={value.until}
            onChange={(e) => set('until', e.target.value)}
          />
        </>
      )}
      <SelectField
        label="报表平台"
        value={value.account}
        options={[['', '全部平台'], ...accounts.map((a) => [a.id, a.name] as [string, string])]}
        onChange={(e) => set('account', e.target.value)}
      />
      <SelectField
        label="报表分类"
        value={value.category}
        options={[['', '全部分类'], ...categories.map((c) => [c.id, c.name] as [string, string])]}
        onChange={(e) => set('category', e.target.value)}
      />
      <Field
        label="商户筛选"
        type="search"
        value={value.merchant}
        onChange={(e) => set('merchant', e.target.value)}
        placeholder="输入商户名称"
      />
    </div>
  );
}
