import { useState } from 'react';
import { accountsApi, type Account } from '../../api/accounts';
import { Field, SelectField, FormError } from '../../components/Fields';
import { useFormAction } from '../../components/Dialog';
import { platformCategories } from '../../shared/labels';
export function AccountForm({
  account,
  onSaved,
}: {
  account?: Account;
  onSaved: (id?: string) => void;
}) {
  const [name, setName] = useState(account?.name ?? '');
  const [category, setCategory] = useState(account?.category ?? 'wallet');
  let createdId: string | undefined;
  const action = useFormAction(() => onSaved(createdId));
  return (
    <form
      className="sheet-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (!name.trim()) {
          action.setError('请输入平台名称。');
          return;
        }
        void action.run(async () => {
          if (account) await accountsApi.update(account.id, name.trim(), category);
          else createdId = (await accountsApi.create(name.trim(), category)).id;
        });
      }}
    >
      <fieldset disabled={action.busy}>
        <Field
          label="平台名称"
          required
          maxLength={40}
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="例如：微信、支付宝、银行卡"
        />
        <SelectField
          label="平台分类"
          options={Object.entries(platformCategories)}
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        />
        <p className="field-hint">当前按人民币记账；分类不代表已接入行情或平台同步。</p>
      </fieldset>
      <FormError message={action.error} />
      <button className="button button-primary submit-button" disabled={action.busy}>
        {action.busy ? '正在保存…' : account ? '保存平台' : '创建平台'}
      </button>
    </form>
  );
}
