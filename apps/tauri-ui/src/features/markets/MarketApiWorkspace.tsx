import { useEffect, useMemo, useRef, useState } from 'react';
import {
  marketApi,
  marketRequestId,
  type MarketApiEndpoint,
  type MarketApiResponse,
  type MarketJson,
  type BinanceStreamEvent,
} from '../../api/marketApi';
import { displayAmount } from '../../shared/format';

type Provider = 'binance' | 'ifind';

function isObject(value: MarketJson): value is { [key: string]: MarketJson } {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function exampleText(value: MarketJson | undefined): string {
  if (value === undefined || value === null) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function parseValue(raw: string, type: string): MarketJson {
  const lowered = type.toLowerCase();
  if (lowered === 'object' || lowered === 'array') {
    try {
      const value = JSON.parse(raw);
      if (lowered === 'array' && !Array.isArray(value)) throw new Error();
      if (lowered === 'object' && !isObject(value)) throw new Error();
      return value as MarketJson;
    } catch {
      throw new Error('请输入有效 JSON。');
    }
  }
  if (lowered === 'boolean') {
    if (raw !== 'true' && raw !== 'false') throw new Error('布尔参数必须是 true 或 false。');
    return raw === 'true';
  }
  // Keep numbers as strings. Binance accepts decimal text and this avoids
  // rounding large IDs or price/quantity values in the browser.
  return raw;
}

function maskResponse(value: MarketJson, key = ''): MarketJson {
  if (
    (typeof value === 'string' || typeof value === 'number') &&
    /(?:amount|price|qty|quantity|balance|free|locked|fee|cost|value|rate|last)/i.test(key)
  ) {
    return displayAmount(String(value));
  }
  if (Array.isArray(value)) return value.map((item) => maskResponse(item, key));
  if (isObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([name, child]) => [name, maskResponse(child, name)]),
    );
  }
  return value;
}

export function MarketApiWorkspace({
  provider,
  configuredScopes,
}: {
  provider: Provider;
  configuredScopes?: {
    read: boolean;
    trade_demo: boolean;
    trade_live: boolean;
    withdraw: boolean;
  };
}) {
  const [catalogue, setCatalogue] = useState<MarketApiEndpoint[]>([]);
  const [catalogueError, setCatalogueError] = useState('');
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState('');
  const [selectedId, setSelectedId] = useState('');
  const [values, setValues] = useState<Record<string, string>>({});
  const [rawParams, setRawParams] = useState('');
  const [environment, setEnvironment] = useState('production');
  const [credentials, setCredentials] = useState<'read' | 'trade' | 'withdraw'>('read');
  const [confirmed, setConfirmed] = useState(false);
  const [result, setResult] = useState<MarketApiResponse | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [streamSession, setStreamSession] = useState<string | null>(null);
  const streamSessionRef = useRef<string | null>(null);
  const [streamEvents, setStreamEvents] = useState<BinanceStreamEvent[]>([]);

  useEffect(() => {
    streamSessionRef.current = streamSession;
  }, [streamSession]);

  useEffect(
    () => () => {
      const session = streamSessionRef.current;
      if (session) void marketApi.stopBinanceStream(session).catch(() => {});
    },
    [],
  );

  useEffect(() => {
    let active = true;
    const load = provider === 'binance' ? marketApi.binanceCatalogue : marketApi.ifindCatalogue;
    void load()
      .then((entries) => {
        if (!active) return;
        setCatalogue(entries);
        setSelectedId(entries[0]?.id ?? '');
      })
      .catch((cause) => active && setCatalogueError(String(cause)));
    return () => {
      active = false;
    };
  }, [provider]);

  const categories = useMemo(
    () => [...new Set(catalogue.map((entry) => entry.category))].sort(),
    [catalogue],
  );
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return catalogue.filter((entry) => {
      if (category && entry.category !== category) return false;
      if (!query) return true;
      return [entry.title, entry.path, entry.id, entry.product].some((value) =>
        value.toLowerCase().includes(query),
      );
    });
  }, [catalogue, category, search]);
  const endpoint =
    filtered.find((entry) => entry.id === selectedId) ??
    filtered[0] ??
    catalogue.find((entry) => entry.id === selectedId);

  useEffect(() => {
    if (!endpoint) return;
    setSelectedId(endpoint.id);
    const initial: Record<string, string> = {};
    for (const parameter of endpoint.params) {
      const sample = endpoint.sample?.[parameter.name] ?? parameter.example;
      if (sample !== undefined) initial[parameter.name] = exampleText(sample);
    }
    setValues(initial);
    setRawParams('');
    setConfirmed(false);
    setResult(null);
    setError('');
  }, [endpoint?.id]);

  useEffect(() => {
    if (!endpoint?.environments?.length) return;
    if (!endpoint.environments.includes(environment)) {
      setEnvironment(endpoint.environments[0]);
    }
  }, [endpoint?.id, endpoint?.environments, environment]);

  useEffect(() => {
    const session = streamSessionRef.current;
    if (!session) return;
    void marketApi.stopBinanceStream(session).catch(() => {});
    streamSessionRef.current = null;
    setStreamSession(null);
    setStreamEvents([]);
  }, [provider, endpoint?.id]);

  const submit = async () => {
    if (!endpoint) return;
    if (endpoint.mutating && !confirmed) {
      setError('写入类接口必须先确认请求。');
      return;
    }
    if (endpoint.supported === false) {
      setError(endpoint.unsupported_reason ?? '该官方接口当前不支持调用。');
      return;
    }
    if (endpoint.environments?.length && !endpoint.environments.includes(environment)) {
      setError('当前环境不支持该接口。');
      return;
    }
    setBusy(true);
    setError('');
    setResult(null);
    try {
      const params: Record<string, MarketJson> = {};
      for (const parameter of endpoint.params) {
        const raw = values[parameter.name]?.trim() ?? '';
        if (!raw) {
          if (parameter.required) throw new Error(`请填写 ${parameter.name}。`);
          continue;
        }
        params[parameter.name] = parseValue(raw, parameter.type);
      }
      if (rawParams.trim()) {
        let extra: unknown;
        try {
          extra = JSON.parse(rawParams);
        } catch {
          throw new Error('补充 JSON 参数格式无效。');
        }
        if (!extra || Array.isArray(extra) || typeof extra !== 'object') {
          throw new Error('补充 JSON 参数必须是对象。');
        }
        Object.assign(params, extra as Record<string, MarketJson>);
      }
      const request = {
        endpoint_id: endpoint.id,
        params,
        environment: provider === 'binance' ? environment : undefined,
        credentials: provider === 'binance' ? credentials : undefined,
        confirmed,
        request_id: marketRequestId(),
      };
      if (provider === 'binance' && endpoint.transport === 'stream') {
        const current = streamSessionRef.current;
        if (current) {
          const outcome = await marketApi.stopBinanceStream(current);
          streamSessionRef.current = null;
          setStreamSession(null);
          setResult({
            status: outcome.status === 'unknown' ? 'unknown' : 'success',
            code: null,
            message: null,
            data: { status: outcome.status },
            request_id: outcome.request_id,
            http_status: null,
          });
          return;
        }
        setStreamEvents([]);
        const pending: number[] = [];
        const session = await marketApi.startBinanceStream(request, (event) => {
          setStreamEvents((current) => [...current.slice(-99), event]);
          const id = streamSessionRef.current;
          if (id && event.sequence > 0) {
            void marketApi.ackBinanceStream(id, event.sequence).catch(() => {});
          } else if (event.sequence > 0) {
            pending.push(event.sequence);
          }
        });
        streamSessionRef.current = session;
        setStreamSession(session);
        for (const sequence of pending)
          void marketApi.ackBinanceStream(session, sequence).catch(() => {});
      } else {
        const response =
          provider === 'binance'
            ? endpoint.transport === 'rest'
              ? await marketApi.binanceRequest(request)
              : await marketApi.binanceSocketRequest(request)
            : await marketApi.ifindRequest(request);
        setResult(response);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <article className="provider-connection-card market-api-workspace" aria-label="官方 API 工作区">
      <header className="provider-card-header">
        <div>
          <h3>官方 API 工作区</h3>
        </div>
        <span className="connection-state configured">{catalogue.length} 个接口</span>
      </header>
      {catalogueError && <p className="form-error">目录读取失败：{catalogueError}</p>}
      <div className="provider-credential-fields">
        <label className="form-field">
          搜索接口
          <input
            aria-label="搜索接口"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </label>
        <label className="form-field">
          接口分类
          <select
            aria-label="接口分类"
            value={category}
            onChange={(event) => setCategory(event.target.value)}
          >
            <option value="">全部（{filtered.length}）</option>
            {categories.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>
        <label className="form-field">
          选择接口
          <select
            aria-label="选择接口"
            value={endpoint?.id ?? ''}
            onChange={(event) => setSelectedId(event.target.value)}
          >
            {filtered.slice(0, 300).map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.method} {entry.path} · {entry.title}
              </option>
            ))}
          </select>
          {filtered.length > 300 && <small>结果过多，请继续搜索后选择（显示前 300 条）。</small>}
        </label>
      </div>
      {endpoint && (
        <>
          <p className="field-hint">
            {endpoint.product} · {endpoint.auth} · {endpoint.transport} ·{' '}
            <a href={endpoint.doc_url} target="_blank" rel="noreferrer">
              官方文档
            </a>
          </p>
          {provider === 'binance' && (
            <div className="provider-credential-fields">
              <label className="form-field">
                币安环境
                <select
                  aria-label="币安环境"
                  value={environment}
                  onChange={(event) => setEnvironment(event.target.value)}
                >
                  {(endpoint.environments ?? ['production']).map((value) => (
                    <option key={value} value={value}>
                      {value === 'spot_testnet'
                        ? '现货 Testnet'
                        : value === 'production'
                          ? '生产环境'
                          : value}
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-field">
                凭据范围
                <select
                  aria-label="币安凭据范围"
                  value={credentials}
                  onChange={(event) =>
                    setCredentials(event.target.value as 'read' | 'trade' | 'withdraw')
                  }
                >
                  <option value="read">只读</option>
                  <option value="trade">交易</option>
                  <option value="withdraw">提现</option>
                </select>
                <small>
                  {credentials === 'read'
                    ? configuredScopes?.read
                      ? '已配置只读密钥。'
                      : '未配置只读密钥，公共接口仍可调用。'
                    : credentials === 'withdraw'
                      ? configuredScopes?.withdraw
                        ? '已配置提现密钥。'
                        : '未配置提现密钥。'
                      : configuredScopes?.trade_demo || configuredScopes?.trade_live
                        ? '已配置交易密钥。'
                        : '未配置交易密钥。'}
                </small>
              </label>
            </div>
          )}
          {endpoint.supported === false && (
            <p className="form-error" role="status">
              {endpoint.unsupported_reason ?? '该目录条目暂不可调用。'}
            </p>
          )}
          <div className="provider-credential-fields">
            {endpoint.params.map((parameter) => (
              <label className="form-field" key={`${endpoint.id}-${parameter.name}`}>
                {parameter.name} {parameter.required ? '（必填）' : '（选填）'}
                {parameter.type === 'boolean' ? (
                  <select
                    aria-label={parameter.name}
                    value={values[parameter.name] ?? ''}
                    onChange={(event) =>
                      setValues((current) => ({ ...current, [parameter.name]: event.target.value }))
                    }
                  >
                    <option value="">不填写</option>
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </select>
                ) : parameter.type === 'object' || parameter.type === 'array' ? (
                  <textarea
                    aria-label={parameter.name}
                    rows={3}
                    value={values[parameter.name] ?? ''}
                    onChange={(event) =>
                      setValues((current) => ({ ...current, [parameter.name]: event.target.value }))
                    }
                  />
                ) : (
                  <input
                    aria-label={parameter.name}
                    value={values[parameter.name] ?? ''}
                    onChange={(event) =>
                      setValues((current) => ({ ...current, [parameter.name]: event.target.value }))
                    }
                  />
                )}
                {parameter.description && <small>{parameter.description}</small>}
              </label>
            ))}
          </div>
          <label className="form-field">
            补充 JSON 参数（可选）
            <textarea
              aria-label="补充 JSON 参数"
              rows={3}
              placeholder='例如：{"symbol":"BTCUSDT"}'
              value={rawParams}
              onChange={(event) => setRawParams(event.target.value)}
            />
            <small>目录未列出的条件参数可在此补充；对象键会覆盖上方同名字段。</small>
          </label>
          {endpoint.mutating && (
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(event) => setConfirmed(event.target.checked)}
              />
              我确认该请求可能改变账户状态，并已核对参数
            </label>
          )}
          <div className="provider-card-actions">
            <button
              className="button button-primary"
              type="button"
              disabled={busy || endpoint.supported === false}
              onClick={() => void submit()}
            >
              {busy
                ? '请求中…'
                : endpoint.transport === 'stream'
                  ? streamSession
                    ? '停止实时流'
                    : '开始实时流'
                  : endpoint.transport === 'rest'
                    ? '执行请求'
                    : '连接并读取一次'}
            </button>
          </div>
        </>
      )}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {result && (
        <section className="connection-result" aria-label="接口结果">
          <strong>
            {result.status === 'success'
              ? '请求成功'
              : result.status === 'unknown'
                ? '请求状态未知：' + (result.message ?? result.code ?? '')
                : '请求失败：' + (result.message ?? result.code ?? '')}
          </strong>
          <pre>{JSON.stringify(maskResponse(result.data), null, 2)}</pre>
        </section>
      )}
      {streamEvents.length > 0 && (
        <section className="connection-result" aria-label="币安实时消息">
          <strong>{streamSession ? '实时流已连接' : '实时流消息'}</strong>
          <pre>
            {JSON.stringify(
              maskResponse(
                streamEvents.map((event) => ({
                  kind: event.kind,
                  sequence: event.sequence,
                  data: event.data,
                })) as unknown as MarketJson,
              ),
              null,
              2,
            )}
          </pre>
        </section>
      )}
    </article>
  );
}
