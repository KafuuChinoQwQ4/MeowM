import { useEffect, useState, type ReactNode } from 'react';
import { Dialog, useDialog } from '../../../components/Dialog';
import type {
  Json,
  OkxApiCatalogue,
  OkxCredentialScope,
  OkxEnvironment,
} from '../../../api/okxAdvanced';
import { okxAdvancedApi } from '../../../api/okxAdvanced';

export function useCatalogue() {
  const [catalogue, setCatalogue] = useState<OkxApiCatalogue | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    okxAdvancedApi.catalogue().then(
      (value) => {
        if (active) setCatalogue(value);
      },
      () => {
        if (active) setError('无法读取 OKX 接口目录，请重新打开页面。');
      },
    );
    return () => {
      active = false;
    };
  }, []);
  return { catalogue, error };
}

export function EnvironmentControls({
  environment,
  credentials,
  onEnvironment,
  onCredentials,
  mode,
}: {
  environment: OkxEnvironment;
  credentials: OkxCredentialScope;
  onEnvironment: (value: OkxEnvironment) => void;
  onCredentials: (value: OkxCredentialScope) => void;
  mode: 'public' | 'read' | 'trade' | 'withdraw';
}) {
  return (
    <div className="okx-controls">
      {mode !== 'withdraw' && (
        <label className="form-field">
          交易环境
          <select
            aria-label="交易环境"
            value={environment}
            onChange={(event) => {
              const value = event.target.value as OkxEnvironment;
              onEnvironment(value);
              if (mode === 'read' && value === 'demo') onCredentials('trade');
            }}
          >
            <option value="demo">模拟盘</option>
            <option value="production">实盘</option>
          </select>
        </label>
      )}
      {mode === 'read' && (
        <label className="form-field">
          使用凭据
          <select
            aria-label="使用凭据"
            value={credentials}
            onChange={(event) => onCredentials(event.target.value as OkxCredentialScope)}
          >
            {environment === 'production' && <option value="read">只读 API</option>}
            <option value="trade">{environment === 'demo' ? '模拟盘' : '实盘'}交易 API</option>
          </select>
        </label>
      )}
      {mode === 'withdraw' && (
        <p className="okx-warning">提币操作使用单独的提币 API 凭据，连接实盘。</p>
      )}
      {mode === 'trade' && (
        <p className="field-hint">
          使用{environment === 'demo' ? '模拟盘' : '实盘'}交易 API 凭据。
        </p>
      )}
      {mode === 'public' && <p className="field-hint">该功能无需 API 凭据。</p>}
    </div>
  );
}

export function ReviewOperation({
  title,
  environment,
  details,
  onClose,
  onConfirm,
}: {
  title: string;
  environment: OkxEnvironment;
  details: Json;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}) {
  return (
    <Dialog title="确认 OKX 操作" onClose={onClose}>
      <ReviewBody title={title} environment={environment} details={details} onConfirm={onConfirm} />
    </Dialog>
  );
}

function ReviewBody({
  title,
  environment,
  details,
  onConfirm,
}: {
  title: string;
  environment: OkxEnvironment;
  details: Json;
  onConfirm: () => Promise<void>;
}) {
  const { busy, setBusy, close } = useDialog();
  const [error, setError] = useState('');
  async function confirm() {
    if (busy) return;
    setBusy(true);
    try {
      await onConfirm();
      setBusy(false);
      close();
    } catch (cause) {
      setError(String(cause));
      setBusy(false);
    }
  }
  return (
    <div className="okx-review">
      <p className={environment === 'production' ? 'okx-warning' : 'okx-notice'}>
        {environment === 'production'
          ? '实盘操作可能改变真实资产或账户，请仔细核对。'
          : '模拟盘操作，请核对参数。'}
      </p>
      <strong>{title}</strong>
      <pre className="okx-json">{JSON.stringify(details, null, 2)}</pre>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      <button
        className="button button-primary"
        type="button"
        disabled={busy}
        onClick={() => void confirm()}
      >
        {busy ? '提交中…' : '确认提交'}
      </button>
    </div>
  );
}

export function InfoDetails({ children }: { children: ReactNode }) {
  return (
    <details className="okx-api-help">
      <summary>查看参数说明</summary>
      {children}
    </details>
  );
}
