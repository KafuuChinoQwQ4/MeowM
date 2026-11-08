import { useEffect, useMemo, useRef, useState } from 'react';
import {
  assertSafeOkxJsonNumbers,
  okxAdvancedApi,
  okxRequestId,
  type Json,
  type OkxApiParameter,
  type OkxApiRequest,
  type OkxApiResponse,
  type OkxCredentialScope,
  type OkxEnvironment,
  type OkxRestEndpoint,
} from '../../../api/okxAdvanced';
import { EnvironmentControls, InfoDetails, ReviewOperation, useCatalogue } from './AdvancedCommon';
import { displayAmount } from '../../../shared/format';
import {
  hasPendingOperations,
  PendingOperations,
  rememberOperation,
  updateOperation,
} from './PendingOperations';

type ObjectValue = { [key: string]: Json };

function topLevelParameters(endpoint: OkxRestEndpoint): OkxApiParameter[] {
  const grouped = new Map<string, OkxApiParameter[]>();
  for (const parameter of endpoint.parameters) {
    if (parameter.name.trimStart().startsWith('>')) continue;
    grouped.set(parameter.name, [...(grouped.get(parameter.name) ?? []), parameter]);
  }
  return [...grouped.values()].map((variants) => ({
    ...variants[0],
    required: variants.every((item) => item.required && !item.conditional),
    conditional:
      variants.some((item) => item.conditional) ||
      (variants.some((item) => item.required) && variants.some((item) => !item.required)),
  }));
}

function fieldKind(parameter: OkxApiParameter): 'json' | 'boolean' | 'number' | 'string' {
  const type = parameter.type.toLowerCase();
  if (type.includes('array') || type.includes('object')) return 'json';
  if (type.includes('bool')) return 'boolean';
  if (/\b(int|integer|long|number|float|double)\b/.test(type)) return 'number';
  return 'string';
}

function parseField(parameter: OkxApiParameter, raw: string): Json {
  const kind = fieldKind(parameter);
  if (kind === 'string') return raw;
  if (kind === 'boolean') return raw === 'true';
  if (kind === 'number') {
    if (!/^-?(?:0|[1-9]\d*)$/.test(raw.trim())) throw new Error(`${parameter.name} 需要整数。`);
    // Rust converts the decimal text to an integer after crossing the IPC boundary.
    return raw.trim();
  }
  let value: Json;
  try {
    value = JSON.parse(raw);
    const type = parameter.type.toLowerCase();
    if (type.includes('array') && !Array.isArray(value)) throw new Error();
    if (
      type.includes('object') &&
      !type.includes('array') &&
      (!value || Array.isArray(value) || typeof value !== 'object')
    )
      throw new Error();
  } catch {
    throw new Error(`${parameter.name} 需要有效的 ${parameter.type} JSON。`);
  }
  assertSafeOkxJsonNumbers(value);
  return value;
}

function requestParams(
  endpoint: OkxRestEndpoint,
  values: Record<string, string>,
  batch: string,
): ObjectValue | ObjectValue[] {
  if (endpoint.body_kind === 'array') {
    let parsed: Json;
    try {
      parsed = JSON.parse(batch);
      if (
        !Array.isArray(parsed) ||
        !parsed.length ||
        parsed.some((row) => !row || Array.isArray(row) || typeof row !== 'object')
      )
        throw new Error();
    } catch {
      throw new Error('批量参数必须是包含对象的 JSON 数组。');
    }
    assertSafeOkxJsonNumbers(parsed);
    return parsed as ObjectValue[];
  }
  const params: ObjectValue = {};
  for (const parameter of topLevelParameters(endpoint)) {
    const raw = values[parameter.name] ?? '';
    if (!raw.trim()) {
      if (parameter.required) throw new Error(`请填写 ${parameter.name}。`);
      continue;
    }
    params[parameter.name] = parseField(parameter, raw);
  }
  return params;
}

function ParameterField({
  parameter,
  value,
  onChange,
}: {
  parameter: OkxApiParameter;
  value: string;
  onChange: (value: string) => void;
}) {
  const kind = fieldKind(parameter);
  return (
    <label className="form-field okx-api-field">
      <span>
        {parameter.name}{' '}
        {parameter.required ? '（必填）' : parameter.conditional ? '（条件必填）' : '（选填）'}
      </span>
      {kind === 'boolean' ? (
        <select
          aria-label={parameter.name}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">不填写</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      ) : kind === 'json' ? (
        <textarea
          aria-label={parameter.name}
          value={value}
          rows={3}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <input
          aria-label={parameter.name}
          value={value}
          inputMode={kind === 'number' ? 'decimal' : 'text'}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
      <small>{parameter.type}</small>
    </label>
  );
}

export function RestExplorer() {
  const { catalogue, error: catalogueError } = useCatalogue();
  const [category, setCategory] = useState('');
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState('');
  const [environment, setEnvironment] = useState<OkxEnvironment>('demo');
  const [credentials, setCredentials] = useState<OkxCredentialScope>('trade');
  const [values, setValues] = useState<Record<string, string>>({});
  const [batch, setBatch] = useState('[{}]');
  const [expiresAt, setExpiresAt] = useState('');
  const [review, setReview] = useState<OkxApiRequest | null>(null);
  const [result, setResult] = useState<OkxApiResponse | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const active = useRef(true);
  const running = useRef(false);
  useEffect(
    () => () => {
      active.current = false;
    },
    [],
  );

  const endpoints = catalogue?.rest ?? [];
  const categories = useMemo(() => [...new Set(endpoints.map((row) => row.category))], [endpoints]);
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return endpoints.filter(
      (row) =>
        (!category || row.category === category) &&
        (!query ||
          `${row.title} ${row.group} ${row.path} ${row.method}`.toLowerCase().includes(query)),
    );
  }, [endpoints, category, search]);
  const endpoint = endpoints.find((row) => row.id === selected);

  function choose(id: string) {
    const next = endpoints.find((row) => row.id === id);
    setSelected(id);
    setValues({});
    setBatch('[{}]');
    setExpiresAt('');
    setResult(null);
    setError('');
    setEnvironment(next?.auth === 'withdraw' || next?.auth === 'public' ? 'production' : 'demo');
    setCredentials(next?.auth === 'withdraw' ? 'withdraw' : 'trade');
  }

  async function send(request: OkxApiRequest, operationTitle?: string) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    setResult(null);
    try {
      if (operationTitle)
        rememberOperation(request.request_id, operationTitle, request.environment, request.params);
      const response = await okxAdvancedApi.request(request);
      if (operationTitle)
        updateOperation(
          request.request_id,
          response.status === 'unknown' ? 'unknown' : active.current ? 'remove' : response.status,
        );
      if (active.current) setResult(response);
    } catch (cause) {
      if (operationTitle) updateOperation(request.request_id, 'unknown');
      if (active.current) setError(`请求未完成：${String(cause)}`);
    } finally {
      running.current = false;
      if (active.current) setBusy(false);
    }
  }

  function submit() {
    if (!endpoint || running.current) return;
    setError('');
    try {
      if (endpoint.mutating && hasPendingOperations())
        throw new Error('请先核对已有的待核对操作，再提交新操作。');
      if (expiresAt && !/^\d{1,20}$/.test(expiresAt))
        throw new Error('有效截止时间必须是毫秒时间戳。');
      const request: OkxApiRequest = {
        endpoint_id: endpoint.id,
        environment: endpoint.auth === 'withdraw' ? 'production' : environment,
        credentials: endpoint.auth === 'withdraw' ? 'withdraw' : credentials,
        params: requestParams(endpoint, values, batch),
        confirmed: false,
        request_id: okxRequestId(),
        exp_time: expiresAt || null,
      };
      if (endpoint.mutating) setReview(request);
      else void send(request);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : '参数格式不正确。');
    }
  }

  return (
    <section className="okx-panel okx-api-explorer" aria-label="OKX 接口目录">
      <PendingOperations />
      {catalogueError && <p role="alert">{catalogueError}</p>}
      {!catalogue && !catalogueError && <p role="status">正在读取接口目录…</p>}
      {catalogue && (
        <>
          <div className="okx-controls">
            <label className="form-field">
              功能分类
              <select
                aria-label="功能分类"
                value={category}
                onChange={(event) => {
                  setCategory(event.target.value);
                  choose('');
                }}
              >
                <option value="">全部分类</option>
                {categories.map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
            </label>
            <label className="form-field">
              搜索接口
              <input
                aria-label="搜索接口"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  choose('');
                }}
                placeholder="名称或 API 路径"
              />
            </label>
          </div>
          <label className="form-field">
            选择接口（{filtered.length} 项）
            <select
              aria-label="选择接口"
              value={selected}
              onChange={(event) => choose(event.target.value)}
            >
              <option value="">请选择接口</option>
              {filtered.map((row) => (
                <option key={row.id} value={row.id}>
                  {row.category} · {row.group} · {row.title} ({row.method})
                </option>
              ))}
            </select>
          </label>
          {endpoint && (
            <>
              <div className="okx-quote-summary">
                <strong>{endpoint.title}</strong>
                <code>
                  {endpoint.method} {endpoint.path}
                </code>
                {endpoint.rate_limit && <span>{endpoint.rate_limit}</span>}
                <a href={endpoint.doc_url} target="_blank" rel="noreferrer">
                  查看 OKX 官方文档
                </a>
              </div>
              <EnvironmentControls
                mode={endpoint.auth}
                environment={environment}
                credentials={credentials}
                onEnvironment={setEnvironment}
                onCredentials={setCredentials}
              />
              {endpoint.body_kind === 'array' ? (
                <label className="form-field">
                  批量参数 JSON
                  <textarea
                    aria-label="批量参数 JSON"
                    rows={7}
                    value={batch}
                    onChange={(event) => setBatch(event.target.value)}
                  />
                  <small>请填写 JSON 对象数组；金额等字符串字段请保留引号。</small>
                </label>
              ) : (
                topLevelParameters(endpoint).map((parameter) => (
                  <ParameterField
                    key={parameter.name}
                    parameter={parameter}
                    value={values[parameter.name] ?? ''}
                    onChange={(value) =>
                      setValues((current) => ({ ...current, [parameter.name]: value }))
                    }
                  />
                ))
              )}
              {endpoint.supports_exp_time && (
                <label className="form-field">
                  有效截止时间（毫秒时间戳，可选）
                  <input
                    aria-label="有效截止时间"
                    value={expiresAt}
                    inputMode="numeric"
                    onChange={(event) => setExpiresAt(event.target.value)}
                  />
                </label>
              )}
              {endpoint.parameters.length > 0 && (
                <InfoDetails>
                  <dl>
                    {endpoint.parameters.map((parameter, index) => (
                      <div key={`${parameter.name}-${index}`}>
                        <dt>
                          {parameter.name} · {parameter.type}
                        </dt>
                        <dd>{parameter.description}</dd>
                      </div>
                    ))}
                  </dl>
                </InfoDetails>
              )}
              <button
                type="button"
                className="button button-primary"
                disabled={busy}
                onClick={submit}
              >
                {busy ? '处理中…' : endpoint.mutating ? '预览操作' : '查询'}
              </button>
            </>
          )}
        </>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {result && (
        <section aria-label="接口结果">
          {result.status === 'unknown' && (
            <p className="okx-warning" role="alert">
              操作结果未确认。请用请求编号和对应查询接口核对，勿重复提交。请求编号：
              {result.request_id}
            </p>
          )}
          {result.status === 'partial_failure' && (
            <p className="okx-warning">部分操作失败，请检查每一项结果。</p>
          )}
          {result.status === 'rejected' && <p className="okx-warning">OKX 拒绝了本次请求。</p>}
          <strong>返回结果 · {result.status}</strong>
          <pre className="okx-json">{displayAmount(JSON.stringify(result, null, 2))}</pre>
        </section>
      )}
      {review && endpoint && (
        <ReviewOperation
          title={endpoint.title}
          environment={review.environment}
          details={{
            request_id: review.request_id,
            endpoint: endpoint.path,
            params: review.params,
            exp_time: review.exp_time,
          }}
          onClose={() => setReview(null)}
          onConfirm={async () => {
            await send({ ...review, confirmed: true }, endpoint.title);
          }}
        />
      )}
    </section>
  );
}
