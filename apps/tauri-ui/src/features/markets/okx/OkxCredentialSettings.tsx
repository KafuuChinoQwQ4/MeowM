import { useState } from 'react';
import type { CredentialInput, ProviderId } from '../../../api/connections';

type OkxCredentialProvider = 'okx' | 'okx_trade_demo' | 'okx_trade_live';
type Credential = Pick<
  Extract<CredentialInput, { provider: 'okx' }>,
  'api_key' | 'secret_key' | 'passphrase'
>;

const groups: {
  provider: OkxCredentialProvider;
  title: string;
  permission: string;
  hint: string;
}[] = [
  {
    provider: 'okx',
    title: '只读 API',
    permission: 'Read',
    hint: '用于读取资产、账单和历史成交，不需要 Trade 或 Withdraw 权限。',
  },
  {
    provider: 'okx_trade_demo',
    title: '模拟盘交易 API',
    permission: 'Read + Trade',
    hint: '只连接 OKX 模拟盘。请不要开启 Withdraw 权限。',
  },
  {
    provider: 'okx_trade_live',
    title: '实盘交易 API',
    permission: 'Read + Trade',
    hint: '用于真实下单。请不要开启 Withdraw 权限，并建议在 OKX 设置 IP 白名单。',
  },
];

export function OkxCredentialSettings({
  configured,
  onSave,
  onRemove,
  onBack,
}: {
  configured: (provider: ProviderId) => boolean;
  onSave: (input: CredentialInput) => Promise<void>;
  onRemove: (provider: ProviderId) => Promise<void>;
  onBack: () => void;
}) {
  return (
    <section className="okx-credential-settings" aria-label="OKX API 凭据设置">
      <button className="button" type="button" onClick={onBack}>
        返回 OKX 行情
      </button>
      <p className="field-hint">
        API 凭据仅保存在本机系统安全凭据存储中，保存后不会回显。公共行情不需要 API Key。
      </p>
      <div className="okx-credential-groups">
        {groups.map((group) => (
          <CredentialGroup
            key={group.provider}
            {...group}
            configured={configured(group.provider)}
            onSave={onSave}
            onRemove={onRemove}
          />
        ))}
      </div>
    </section>
  );
}

function CredentialGroup({
  provider,
  title,
  permission,
  hint,
  configured,
  onSave,
  onRemove,
}: {
  provider: OkxCredentialProvider;
  title: string;
  permission: string;
  hint: string;
  configured: boolean;
  onSave: (input: CredentialInput) => Promise<void>;
  onRemove: (provider: ProviderId) => Promise<void>;
}) {
  const [values, setValues] = useState<Credential>({ api_key: '', secret_key: '', passphrase: '' });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const submit = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
      setValues({ api_key: '', secret_key: '', passphrase: '' });
    } catch {
      setError('操作失败，请检查 API 凭据和系统安全存储后重试。');
    } finally {
      setBusy(false);
    }
  };
  const save = () => {
    const input = { provider, ...values } as CredentialInput;
    return submit(() => onSave(input));
  };
  const remove = () => submit(() => onRemove(provider));

  return (
    <article className="provider-connection-card okx-credential-card" data-provider={provider}>
      <header className="provider-card-header">
        <div>
          <h3>{title}</h3>
          <p>{hint}</p>
          <small className="field-hint">所需权限：{permission}。不要开启 Withdraw。</small>
        </div>
        <span className={configured ? 'connection-state configured' : 'connection-state'}>
          {configured ? '已配置' : '尚未配置'}
        </span>
      </header>
      {configured && <p className="credential-saved">已安全保存，凭据不会在此页面回显。</p>}
      <div className="provider-credential-fields">
        {(
          [
            ['api_key', 'API Key'],
            ['secret_key', 'Secret Key'],
            ['passphrase', 'Passphrase'],
          ] as const
        ).map(([name, label]) => (
          <label className="form-field" key={name}>
            {label}
            <input
              type="password"
              aria-label={label}
              autoComplete="off"
              spellCheck={false}
              value={values[name]}
              onChange={(event) =>
                setValues((current) => ({ ...current, [name]: event.target.value }))
              }
            />
          </label>
        ))}
      </div>
      <div className="provider-card-actions">
        <button
          className="button button-primary"
          type="button"
          disabled={busy || Object.values(values).some((value) => !value.trim())}
          onClick={() => void save()}
        >
          {busy ? '处理中…' : '保存凭据'}
        </button>
        {configured && (
          <button className="button" type="button" disabled={busy} onClick={() => void remove()}>
            断开连接
          </button>
        )}
      </div>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </article>
  );
}
