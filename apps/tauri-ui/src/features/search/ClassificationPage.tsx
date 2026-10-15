import { useState } from 'react';
import { searchApi, type ClassificationState, type Rule } from '../../api/search';
import { Field, FormError } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { errorText } from '../../shared/errors';
import { RuleForm } from './RuleForm';
export function ClassificationPage({
  state,
  onChanged,
}: {
  state: { data: ClassificationState | null; loading: boolean; error: string; reload: () => void };
  onChanged: () => void;
}) {
  const [editing, setEditing] = useState<Rule | 'new' | null>(null);
  const [alias, setAlias] = useState('');
  const [canonical, setCanonical] = useState('');
  const [category, setCategory] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  async function act(operation: () => Promise<unknown>, after?: () => void) {
    setBusy(true);
    setError('');
    try {
      await operation();
      after?.();
      onChanged();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="classification-page">
      <LoadState {...state} />
      <FormError message={error} />
      {state.data && (
        <>
          <div className="analytics-row">
            <h3>分类</h3>
          </div>
          <div className="pending-actions">
            <Field
              label="新分类名称"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            />
            <button
              className="button button-secondary"
              disabled={busy || !category.trim()}
              onClick={() =>
                void act(
                  () => searchApi.category(category, true),
                  () => setCategory(''),
                )
              }
            >
              新增分类
            </button>
          </div>
          {state.data.categories.map((c) => (
            <div className="search-list-row" key={c.id}>
              <span>
                {c.name} · {c.enabled ? '启用' : '停用'}
              </span>
              <button
                className="text-button"
                disabled={busy}
                onClick={() => void act(() => searchApi.category(c.name, !c.enabled, c.id))}
              >
                {c.enabled ? '停用' : '启用'}
              </button>
            </div>
          ))}
          <div className="analytics-row">
            <h3>商户别名</h3>
          </div>
          <p className="field-hint">原始商户名保留在流水中；别名只用于搜索和规则建议。</p>
          <div className="search-grid">
            <Field label="商户别名" value={alias} onChange={(e) => setAlias(e.target.value)} />
            <Field
              label="规范商户"
              value={canonical}
              onChange={(e) => setCanonical(e.target.value)}
            />
          </div>
          <button
            className="button button-secondary"
            disabled={busy || !alias.trim() || !canonical.trim()}
            onClick={() =>
              void act(
                () => searchApi.alias(alias, canonical),
                () => {
                  setAlias('');
                  setCanonical('');
                },
              )
            }
          >
            保存别名
          </button>
          {state.data.aliases.map((a) => (
            <div className="search-list-row" key={a.alias}>
              <span>
                {a.alias} → {a.canonical}
              </span>
              <button
                className="text-button"
                disabled={busy}
                onClick={() => void act(() => searchApi.deleteAlias(a.alias))}
              >
                删除别名
              </button>
            </div>
          ))}
          <div className="analytics-row">
            <h3>分类规则</h3>
            <button
              className="button button-primary"
              disabled={!state.data.categories.some((c) => c.enabled)}
              onClick={() => setEditing('new')}
            >
              新增规则
            </button>
          </div>
          <p className="field-hint">
            优先级数字越小越先匹配；需先预览受影响的待确认候选，再确认保存。
          </p>
          {state.data.rules.map((r) => (
            <div className="search-list-row" key={r.id}>
              <span>
                {r.enabled ? '启用' : '停用'} · {r.priority} · {r.merchant_contains || '任意商户'} ·{' '}
                {r.description_contains || '任意备注'} →{' '}
                {state.data?.categories.find((c) => c.id === r.category_id)?.name ?? '原分类'}
              </span>
              <button className="text-button" onClick={() => setEditing(r)}>
                编辑规则
              </button>
            </div>
          ))}
          {editing && (
            <RuleForm
              key={editing === 'new' ? 'new' : editing.id}
              categories={state.data.categories}
              rule={editing === 'new' ? undefined : editing}
              onCancel={() => setEditing(null)}
              onSaved={() => {
                setEditing(null);
                onChanged();
              }}
            />
          )}
        </>
      )}
    </div>
  );
}
