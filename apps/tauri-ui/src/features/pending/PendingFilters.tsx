import type { Account } from '../../api/accounts';
import type { PageQuery } from '../../api/entries';
import { Field, SelectField } from '../../components/Fields';
import { sourceNames } from '../../shared/labels';
export function PendingFilters({
  query,
  accounts,
  onChange,
  disabled,
}: {
  query: PageQuery;
  accounts: Account[];
  onChange: (query: PageQuery) => void;
  disabled: boolean;
}) {
  function update(key: keyof PageQuery, value: string) {
    onChange({ ...query, [key]: value || undefined, offset: 0 });
  }
  return (
    <fieldset className="pending-filters" disabled={disabled}>
      <SelectField
        label="状态"
        options={[
          ['pending', '待确认'],
          ['ignored', '已忽略'],
        ]}
        value={query.status}
        onChange={(e) => update('status', e.target.value)}
      />
      <SelectField
        label="来源"
        options={[['', '全部来源'], ...Object.entries(sourceNames)]}
        value={query.source ?? ''}
        onChange={(e) => update('source', e.target.value)}
      />
      <SelectField
        label="平台"
        options={[['', '全部平台'], ...accounts.map((a) => [a.id, a.name] as [string, string])]}
        value={query.account_id ?? ''}
        onChange={(e) => update('account_id', e.target.value)}
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
    </fieldset>
  );
}
