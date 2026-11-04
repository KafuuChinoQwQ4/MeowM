import { useState } from 'react';
import type { ConnectionTestResult, CredentialInput, ProviderId } from '../../api/connections';

export interface CredentialFieldConfig {
  name: string;
  label: string;
  autocomplete?: string;
  multiline?: boolean;
  secret?: boolean;
}

export function ProviderConnectionCard({
  provider,
  title,
  description,
  fields,
  configured,
  showCredentials = true,
  operation,
  onSave,
  onRemove,
  onTest,
}: {
  provider: ProviderId;
  title: string;
  description: string;
  fields: CredentialFieldConfig[];
  configured: boolean;
  showCredentials?: boolean;
  operation: 'bill' | 'balances' | 'quote';
  onSave: (input: CredentialInput) => Promise<void>;
  onRemove: (provider: ProviderId) => Promise<void>;
  onTest: (
    provider: ProviderId,
    options: { bill_date?: string; symbol?: string },
  ) => Promise<ConnectionTestResult>;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [billDate, setBillDate] = useState(previousDay());
  const [symbol, setSymbol] = useState('000001.SZ');
  const [result, setResult] = useState<ConnectionTestResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const submit = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (cause) {
      setError(safeError(cause));
    } finally {
      setBusy(false);
    }
  };

  const save = () => {
    const input = Object.fromEntries([
      ['provider', provider],
      ...fields.map((field) => [field.name, values[field.name] ?? '']),
    ]) as CredentialInput;
    return submit(async () => {
      await onSave(input);
      setValues({});
      setResult(null);
    });
  };

  return (
    <article className="provider-connection-card" data-provider={provider}>
      <header className="provider-card-header">
        <div>
          <h3>{title}</h3>
          <p>{description}</p>
        </div>
        <span className={configured ? 'connection-state configured' : 'connection-state'}>
          {configured ? '已配置' : '尚未配置'}
        </span>
      </header>
      {showCredentials && configured && (
        <p className="credential-saved">已安全保存，凭据不会在此页面回显。</p>
      )}
      {showCredentials && (
        <div className="provider-credential-fields">
          {fields.map((field) => (
            <label className="form-field" key={field.name}>
              {field.label}
              {field.multiline ? (
                <textarea
                  className={field.secret ? 'credential-secret-multiline' : undefined}
                  aria-label={field.label}
                  autoComplete={field.autocomplete ?? 'off'}
                  spellCheck={false}
                  value={values[field.name] ?? ''}
                  onChange={(event) =>
                    setValues((current) => ({ ...current, [field.name]: event.target.value }))
                  }
                />
              ) : (
                <input
                  aria-label={field.label}
                  type="password"
                  autoComplete={field.autocomplete ?? 'off'}
                  spellCheck={false}
                  value={values[field.name] ?? ''}
                  onChange={(event) =>
                    setValues((current) => ({ ...current, [field.name]: event.target.value }))
                  }
                />
              )}
            </label>
          ))}
        </div>
      )}
      {operation === 'bill' && (
        <label className="form-field">
          账单日期
          <input
            aria-label="账单日期"
            type="date"
            max={previousDay()}
            value={billDate}
            onChange={(event) => setBillDate(event.target.value)}
          />
          <small className="field-hint">账单接口只读取该日期的商户账单，不会导入账本。</small>
        </label>
      )}
      {operation === 'quote' && (
        <label className="form-field">
          行情标的
          <input
            aria-label="行情标的"
            value={symbol}
            maxLength={20}
            onChange={(event) => setSymbol(event.target.value.toUpperCase())}
          />
          <small className="field-hint">iFinD 行情数据测试，默认标的为 000001.SZ。</small>
        </label>
      )}
      <div className="provider-card-actions">
        {showCredentials && (
          <button
            className="button button-primary"
            type="button"
            disabled={busy || fields.some((field) => !values[field.name]?.trim())}
            onClick={() => void save()}
          >
            {busy ? '处理中…' : '保存凭据'}
          </button>
        )}
        <button
          className="button"
          type="button"
          disabled={busy || !configured}
          onClick={() =>
            void submit(async () => {
              setResult(null);
              setResult(
                await onTest(provider, {
                  bill_date: operation === 'bill' ? billDate : undefined,
                  symbol: operation === 'quote' ? symbol.trim() : undefined,
                }),
              );
            })
          }
        >
          {operation === 'bill'
            ? '测试账单读取'
            : operation === 'quote'
              ? '测试行情读取'
              : '测试只读余额'}
        </button>
        {showCredentials && (
          <button
            className="button"
            type="button"
            disabled={busy || !configured}
            onClick={() =>
              void submit(async () => {
                await onRemove(provider);
                setResult(null);
                setValues({});
              })
            }
          >
            断开连接
          </button>
        )}
      </div>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {result && <ConnectionResult result={result} />}
    </article>
  );
}

export function ConnectionResult({ result }: { result: ConnectionTestResult }) {
  if (result.outcome === 'failed')
    return (
      <p className="form-error" role="status">
        连接失败：{errorLabel(result.error_code)}
      </p>
    );
  if (!result.summary)
    return (
      <p className="connection-result" role="status">
        读取成功。
      </p>
    );
  if (result.summary.kind === 'bill_read') {
    const bill = result.summary.data;
    return (
      <p className="connection-result" role="status">
        {bill.bill_date} 账单读取成功，共 {bill.downloaded_bytes} 字节
        {bill.hash_verified === true ? '，摘要校验通过。' : '。'}
      </p>
    );
  }
  if (result.summary.kind === 'ifind_quote') {
    const quote = result.summary.data.quote;
    return (
      <p className="connection-result" role="status">
        行情读取成功：{quote.symbol}，最新报价 {quote.last}
        {quote.quote_time ? `（${quote.quote_time}）` : ''}
      </p>
    );
  }
  const balances = result.summary.data.balances;
  return (
    <div className="connection-result" role="status">
      {balances.length === 0 ? (
        <p>连接成功，当前没有非零余额。</p>
      ) : (
        <ul>
          {balances.map((balance) => (
            <li key={balance.asset}>
              <strong>{balance.asset}</strong> 可用 {balance.free}，冻结 {balance.locked}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function previousDay(): string {
  const date = new Date();
  date.setDate(date.getDate() - 1);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function safeError(cause: unknown): string {
  const message = cause instanceof Error ? cause.message : String(cause);
  return errorLabel(message);
}

function errorLabel(code: string | null | undefined): string {
  const labels: Record<string, string> = {
    invalid_credentials: '凭据无效，请检查填写内容。',
    permission_denied: '权限不足，请为 API Key 开启只读权限。',
    bill_unavailable: '该日期暂无可读取的账单。',
    data_unavailable: '平台暂无可读取的数据。',
    rate_limited: '平台请求频率受限，请稍后重试。',
    timeout: '请求超时，请检查网络后重试。',
    network_failure: '网络连接失败，请稍后重试。',
    invalid_response: '平台返回的数据无法验证。',
    hash_mismatch: '账单摘要校验失败，未接受该账单。',
    secure_storage_unavailable: '系统安全凭据存储不可用。',
    unsupported_provider: '该平台暂不支持此项操作。',
    invalid_request: '请求参数无效，请检查日期或标的代码。',
    provider_failure: '平台暂时无法完成请求。',
  };
  return labels[code ?? ''] ?? '操作失败，请重试。';
}
