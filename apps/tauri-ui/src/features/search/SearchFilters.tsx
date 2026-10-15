import type { Account } from '../../api/accounts';
import type { Category } from '../../api/search';
import type { PageQuery } from '../../api/entries';
import { Field, SelectField } from '../../components/Fields';
import { sourceNames } from '../../shared/labels';
export function SearchFilters({
  query,
  accounts,
  categories,
  onChange,
}: {
  query: PageQuery;
  accounts: Account[];
  categories: Category[];
  onChange: (q: PageQuery) => void;
}) {
  const update = (key: keyof PageQuery, value: string) =>
    onChange({ ...query, [key]: value || undefined, offset: 0 });
  return (
    <div className="search-grid">
      <Field
        label="关键词"
        value={query.keyword ?? ''}
        onChange={(e) => update('keyword', e.target.value)}
      />
      <Field
        label="商户"
        value={query.merchant ?? ''}
        onChange={(e) => update('merchant', e.target.value)}
      />
      <SelectField
        label="状态"
        value={query.status ?? ''}
        options={[
          ['', '全部状态'],
          ['pending', '待确认'],
          ['confirmed', '已确认'],
          ['ignored', '已忽略'],
        ]}
        onChange={(e) => update('status', e.target.value)}
      />
      <SelectField
        label="来源"
        value={query.source ?? ''}
        options={[['', '全部来源'], ...Object.entries(sourceNames)]}
        onChange={(e) => update('source', e.target.value)}
      />
      <SelectField
        label="平台"
        value={query.account_id ?? ''}
        options={[['', '全部平台'], ...accounts.map((a) => [a.id, a.name] as [string, string])]}
        onChange={(e) => update('account_id', e.target.value)}
      />
      <SelectField
        label="分类"
        value={query.category_id ?? ''}
        options={[['', '全部分类'], ...categories.map((c) => [c.id, c.name] as [string, string])]}
        onChange={(e) => update('category_id', e.target.value)}
      />
      <Field
        label="起始日期"
        type="date"
        value={query.from_date ?? ''}
        onChange={(e) => update('from_date', e.target.value)}
      />
      <Field
        label="结束日期（不含）"
        type="date"
        value={query.until_date ?? ''}
        onChange={(e) => update('until_date', e.target.value)}
      />
      <Field
        label="最低金额（元）"
        inputMode="decimal"
        value={query.min_amount ?? ''}
        onChange={(e) => update('min_amount', e.target.value)}
      />
      <Field
        label="最高金额（元）"
        inputMode="decimal"
        value={query.max_amount ?? ''}
        onChange={(e) => update('max_amount', e.target.value)}
      />
    </div>
  );
}
