import { useEffect, useMemo, useRef, useState } from 'react';
import {
  assertSafeOkxJsonNumbers,
  okxAdvancedApi,
  okxRequestId,
  type Json,
  type OkxCredentialScope,
  type OkxEnvironment,
  type OkxStreamEvent,
  type OkxStreamRequest,
} from '../../../api/okxAdvanced';
import { EnvironmentControls, InfoDetails, ReviewOperation, useCatalogue } from './AdvancedCommon';
import { displayAmount } from '../../../shared/format';
import {
  hasPendingOperations,
  PendingOperations,
  rememberOperation,
  updateOperation,
} from './PendingOperations';

export function StreamExplorer() {
  const { catalogue, error: catalogueError } = useCatalogue();
  const [category, setCategory] = useState('');
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState('');
  const [environment, setEnvironment] = useState<OkxEnvironment>('demo');
  const [credentials, setCredentials] = useState<OkxCredentialScope>('trade');
  const [args, setArgs] = useState('[{}]');
  const [expiresAt, setExpiresAt] = useState('');
  const [events, setEvents] = useState<OkxStreamEvent[]>([]);
  const [review, setReview] = useState<OkxStreamRequest | null>(null);
  const [error, setError] = useState('');
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const session = useRef<string | null>(null);
  const generation = useRef(0);
  const active = useRef(true);
  const inProgress = useRef(false);

  function closeSession(id: string) {
    void okxAdvancedApi
      .stopStream(id)
      .then((outcome) => {
        if (outcome?.request_id)
          updateOperation(outcome.request_id, outcome.status === 'unknown' ? 'unknown' : 'closed');
      })
      .catch(() => {});
  }

  function stop() {
    generation.current += 1;
    const current = session.current;
    session.current = null;
    inProgress.current = false;
    if (active.current) {
      setConnected(false);
      setBusy(false);
    }
    if (current) closeSession(current);
  }
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      stop();
    };
  }, []);

  const endpoints = catalogue?.websocket ?? [];
  const categories = useMemo(() => [...new Set(endpoints.map((row) => row.category))], [endpoints]);
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return endpoints.filter(
      (row) =>
        (!category || row.category === category) &&
        (!query ||
          `${row.title} ${row.group} ${row.service} ${row.operation} ${row.channels?.join(' ')}`
            .toLowerCase()
            .includes(query)),
    );
  }, [endpoints, category, search]);
  const endpoint = endpoints.find((row) => row.id === selected);
  const mode = !endpoint?.requires_login ? 'public' : endpoint.mutating ? 'trade' : 'read';

  function choose(id: string) {
    stop();
    const next = endpoints.find((row) => row.id === id);
    setSelected(id);
    setEvents([]);
    setError('');
    setEnvironment(next && !next.requires_login ? 'production' : 'demo');
    setCredentials('trade');
    setExpiresAt('');
    setArgs(
      JSON.stringify(
        next?.sample_args?.length ? next.sample_args : [{ channel: next?.channels?.[0] ?? '' }],
        null,
        2,
      ),
    );
  }

  async function start(request: OkxStreamRequest) {
    if (inProgress.current) return;
    inProgress.current = true;
    setBusy(true);
    setError('');
    setEvents([]);
    const currentGeneration = ++generation.current;
    let startedId: string | null = null;
    let ended = false;
    const pendingAcks: number[] = [];
    const acknowledge = (id: string, sequence: number) => {
      void okxAdvancedApi.ackStream(id, sequence).catch(() => {
        if (active.current && generation.current === currentGeneration) {
          setError('实时消息确认失败，连接已停止。');
          stop();
        }
      });
    };
    try {
      if (endpoint?.mutating)
        rememberOperation(request.request_id, endpoint.title, request.environment, request.args);
      const id = await okxAdvancedApi.startStream(request, (event) => {
        if (!active.current || generation.current !== currentGeneration) return;
        setEvents((current) => [...current.slice(-99), event]);
        if (event.kind === 'closed' || event.kind === 'error') {
          ended = true;
          if (startedId && session.current === startedId) session.current = null;
          setConnected(false);
          if (
            event.kind === 'closed' &&
            event.data &&
            typeof event.data === 'object' &&
            !Array.isArray(event.data)
          ) {
            updateOperation(
              request.request_id,
              event.data.status === 'unknown' ? 'unknown' : 'closed',
            );
          }
        }
        if (
          endpoint?.mutating &&
          event.kind === 'message' &&
          event.data &&
          typeof event.data === 'object' &&
          !Array.isArray(event.data) &&
          event.data.id === request.request_id &&
          (event.data.op === endpoint.operation || event.data.event === 'error')
        ) {
          updateOperation(request.request_id, 'remove');
        }
        if (typeof event.sequence === 'number') {
          if (startedId) acknowledge(startedId, event.sequence);
          else pendingAcks.push(event.sequence);
        }
      });
      startedId = id;
      if (!active.current || generation.current !== currentGeneration) {
        closeSession(id);
        return;
      }
      session.current = ended ? null : id;
      for (const sequence of pendingAcks) acknowledge(id, sequence);
      setConnected(!ended);
    } catch (cause) {
      if (endpoint?.mutating) updateOperation(request.request_id, 'unknown');
      if (active.current && generation.current === currentGeneration)
        setError(`无法启动实时连接：${String(cause)}`);
    } finally {
      if (active.current && generation.current === currentGeneration) {
        setBusy(false);
        inProgress.current = false;
      }
    }
  }

  function submit() {
    if (!endpoint || inProgress.current || connected) return;
    try {
      if (endpoint.mutating && hasPendingOperations()) {
        setError('请先核对已有的待核对操作，再提交新操作。');
        return;
      }
      const parsed: Json = JSON.parse(args);
      if (
        !Array.isArray(parsed) ||
        !parsed.length ||
        parsed.some((row) => !row || Array.isArray(row) || typeof row !== 'object')
      )
        throw new Error();
      assertSafeOkxJsonNumbers(parsed);
      if (expiresAt && !/^\d{1,20}$/.test(expiresAt)) {
        setError('有效截止时间必须是毫秒时间戳。');
        return;
      }
      const request: OkxStreamRequest = {
        endpoint_id: endpoint.id,
        environment,
        credentials,
        args: parsed as { [key: string]: Json }[],
        request_id: okxRequestId(),
        confirmed: false,
        exp_time: expiresAt || null,
      };
      setError('');
      if (endpoint.mutating) setReview(request);
      else void start(request);
    } catch (cause) {
      setError(
        cause instanceof Error && cause.message.startsWith('JSON 中')
          ? cause.message
          : '频道参数必须是包含对象的 JSON 数组。',
      );
    }
  }

  return (
    <section className="okx-panel okx-api-explorer" aria-label="OKX 实时频道">
      <PendingOperations />
      {catalogueError && <p role="alert">{catalogueError}</p>}
      {!catalogue && !catalogueError && <p role="status">正在读取频道目录…</p>}
      {catalogue && (
        <>
          <div className="okx-controls">
            <label className="form-field">
              功能分类
              <select
                aria-label="频道分类"
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
              搜索频道
              <input
                aria-label="搜索频道"
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  choose('');
                }}
                placeholder="名称或频道"
              />
            </label>
          </div>
          <label className="form-field">
            选择频道（{filtered.length} 项）
            <select
              aria-label="选择频道"
              value={selected}
              onChange={(event) => choose(event.target.value)}
            >
              <option value="">请选择频道</option>
              {filtered.map((row) => (
                <option key={row.id} value={row.id}>
                  {row.category} · {row.group} · {row.title}
                </option>
              ))}
            </select>
          </label>
          {endpoint && (
            <>
              <div className="okx-quote-summary">
                <strong>{endpoint.title}</strong>
                <span>
                  {endpoint.service} · {endpoint.operation}
                </span>
                <a href={endpoint.doc_url} target="_blank" rel="noreferrer">
                  查看 OKX 官方文档
                </a>
              </div>
              {endpoint.service === 'public-sbe' && (
                <p className="field-hint">
                  SBE
                  行情同时显示原始数据与支持格式的解码结果，暂不重建订单簿；不同频道有账户等级要求。
                </p>
              )}
              <EnvironmentControls
                mode={mode}
                environment={environment}
                credentials={credentials}
                onEnvironment={setEnvironment}
                onCredentials={setCredentials}
              />
              <label className="form-field">
                频道参数 JSON
                <textarea
                  aria-label="频道参数 JSON"
                  rows={7}
                  value={args}
                  disabled={connected || busy}
                  onChange={(event) => setArgs(event.target.value)}
                />
                <small>填写 JSON 对象数组，字段含义见下方说明。</small>
              </label>
              {endpoint.parameters.some((parameter) => parameter.name === 'expTime') && (
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
              <div className="okx-actions">
                <button
                  className="button button-primary"
                  type="button"
                  disabled={busy || connected}
                  onClick={submit}
                >
                  {busy ? '连接中…' : endpoint.mutating ? '预览操作' : '开始订阅'}
                </button>
                {(busy || connected) && (
                  <button className="button" type="button" onClick={stop}>
                    断开连接
                  </button>
                )}
              </div>
            </>
          )}
        </>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {events.length > 0 && (
        <section aria-label="实时消息">
          <strong>实时消息（最近 {events.length} 条）</strong>
          <pre className="okx-json">{displayAmount(JSON.stringify(events, null, 2))}</pre>
        </section>
      )}
      {review && endpoint && (
        <ReviewOperation
          title={endpoint.title}
          environment={review.environment}
          details={{
            request_id: review.request_id,
            operation: endpoint.operation,
            args: review.args,
            exp_time: review.exp_time,
          }}
          onClose={() => setReview(null)}
          onConfirm={async () => {
            await start({ ...review, confirmed: true });
          }}
        />
      )}
    </section>
  );
}
