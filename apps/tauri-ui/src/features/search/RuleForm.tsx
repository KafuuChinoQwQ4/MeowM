import { useState } from 'react';
import {
  searchApi,
  type Category,
  type Rule,
  type RuleInput,
  type RulePreview,
} from '../../api/search';
import { Field, SelectField, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
import { displayAmount } from '../../shared/format';
export function RuleForm({
  categories,
  rule,
  onSaved,
  onCancel,
}: {
  categories: Category[];
  rule?: Rule;
  onSaved: () => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState<RuleInput>(
    rule ?? {
      priority: 100,
      merchant_contains: null,
      description_contains: null,
      category_id: categories.find((c) => c.enabled)?.id ?? '',
      enabled: true,
    },
  );
  const [preview, setPreview] = useState<RulePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  function update(change: Partial<RuleInput>) {
    setDraft((old) => ({ ...old, ...change }));
    setPreview(null);
  }
  async function inspect() {
    setBusy(true);
    setError('');
    try {
      setPreview(await searchApi.preview(draft));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    if (!preview) return;
    setBusy(true);
    setError('');
    try {
      await searchApi.rule(draft);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="search-rule-form">
      <Field
        label="规则优先级（数字越小越先）"
        type="number"
        min="0"
        max="10000"
        value={draft.priority}
        onChange={(e) => update({ priority: Number(e.target.value) })}
      />
      <Field
        label="规则商户包含"
        value={draft.merchant_contains ?? ''}
        onChange={(e) => update({ merchant_contains: e.target.value || null })}
      />
      <Field
        label="规则备注包含"
        value={draft.description_contains ?? ''}
        onChange={(e) => update({ description_contains: e.target.value || null })}
      />
      <SelectField
        label="目标分类"
        value={draft.category_id}
        options={categories
          .filter((c) => c.enabled || c.id === draft.category_id)
          .map((c) => [c.id, c.name])}
        onChange={(e) => update({ category_id: e.target.value })}
      />
      <label className="search-check">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => update({ enabled: e.target.checked })}
        />{' '}
        启用规则
      </label>
      <div className="pending-actions">
        <button className="button button-secondary" disabled={busy} onClick={() => void inspect()}>
          预览规则
        </button>
        <button
          className="button button-primary"
          disabled={busy || !preview}
          onClick={() => void save()}
        >
          确认保存规则
        </button>
        <button className="text-button" onClick={onCancel}>
          取消
        </button>
      </div>
      {preview && (
        <div role="status">
          <p>
            命中 {preview.match_count} 条，影响 {preview.pending_count} 条未手工分类的待确认记录
          </p>
          {preview.pending_samples.length > 0 && (
            <div>
              <p>影响候选示例（最多 20 条）</p>
              <ul>
                {preview.pending_samples.map((sample) => (
                  <li key={sample.id}>
                    {sample.merchant || sample.description || '未命名记录'}
                    {sample.amount && ` · ¥${displayAmount(sample.amount)}`}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
      <p className="field-hint">保存后只更新未手工分类候选的建议，不批量修改已确认流水。</p>
      <FormError message={error} />
    </div>
  );
}
