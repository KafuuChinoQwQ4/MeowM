import { useRef, useState } from 'react';
import {
  cryptoApi,
  type OkxEnvironment,
  type OkxInstrumentType,
  type OkxOrderActionResult,
  type OkxOrderDraft,
  type OkxOrderSnapshot,
  type OkxOrderSubmission,
  type OkxPositionAction,
  type OkxTradeContext,
  type OkxTradeMode,
} from '../../../api/crypto';
import { Dialog, useDialog } from '../../../components/Dialog';
import { OrderReviewDialog } from './OrderReviewDialog';
import { okxError, useAction } from './shared';

type FormState = {
  environment: OkxEnvironment;
  instType: OkxInstrumentType;
  instId: string;
  side: 'buy' | 'sell';
  action: OkxPositionAction;
  orderType: 'market' | 'limit';
  size: string;
  price: string;
  slippage: string;
  marginMode: 'cross' | 'isolated';
};

type UnknownOrder = {
  environment: OkxEnvironment;
  instId: string;
  clOrdId: string;
};

type OrderOutcome = {
  result: OkxOrderSubmission;
  environment: OkxEnvironment;
};

type PendingTradeOrder = OkxTradeContext['account']['pending_orders'][number];
type CancelSelection = {
  order: PendingTradeOrder;
  environment: OkxEnvironment;
  instType: OkxInstrumentType;
  instId: string;
  revision: number;
};

const actionSide: Record<OkxPositionAction, 'buy' | 'sell'> = {
  open_long: 'buy',
  open_short: 'sell',
  close_long: 'sell',
  close_short: 'buy',
};

const actionLabel: Record<OkxPositionAction, string> = {
  open_long: '开多',
  open_short: '开空',
  close_long: '平多',
  close_short: '平空',
};

const modeLabel: Record<OkxTradeMode, string> = {
  cash: '现金',
  cross: '全仓',
  isolated: '逐仓',
};

const freshForm = (): FormState => ({
  environment: 'demo',
  instType: 'SPOT',
  instId: 'BTC-USDT',
  side: 'buy',
  action: 'open_long',
  orderType: 'market',
  size: '',
  price: '',
  slippage: '1',
  marginMode: 'cross',
});

export function TradingPanel({
  demoConfigured,
  liveConfigured,
  onConfigure,
}: {
  demoConfigured: boolean;
  liveConfigured: boolean;
  onConfigure: () => void;
}) {
  const [form, setForm] = useState(freshForm);
  const [context, setContext] = useState<OkxTradeContext | null>(null);
  const [review, setReview] = useState<{
    draft: OkxOrderDraft;
    context: OkxTradeContext;
    estimate: string;
  } | null>(null);
  const [outcome, setOutcome] = useState<OrderOutcome | null>(null);
  const [unknownOrder, setUnknownOrder] = useState<UnknownOrder | null>(null);
  const [orderDetail, setOrderDetail] = useState<OkxOrderSnapshot | null>(null);
  const [detailChecked, setDetailChecked] = useState(false);
  const [history, setHistory] = useState<OkxOrderSnapshot[] | null>(null);
  const [cancelTarget, setCancelTarget] = useState<CancelSelection | null>(null);
  const [cancelNeedsRefresh, setCancelNeedsRefresh] = useState(false);
  const [cancelOutcome, setCancelOutcome] = useState<{
    result: OkxOrderActionResult;
    environment: OkxEnvironment;
  } | null>(null);
  const formRevision = useRef(0);
  const action = useAction();
  const tradeWriteBlocked = !!unknownOrder || cancelNeedsRefresh;
  const configured = form.environment === 'demo' ? demoConfigured : liveConfigured;
  const contract = form.instType !== 'SPOT';
  const spotBase = context?.instrument.base_ccy ?? 'BTC';
  const instrumentQuote = context?.instrument.quote_ccy ?? quoteCurrencyFromId(form.instId);
  const spotQuote = instrumentQuote ?? 'USDT';
  const priceUnit = instrumentQuote ?? (contract ? '价格币种未知' : 'USDT');
  const sizeUnit = contract
    ? '张'
    : form.orderType === 'market' && form.side === 'sell'
      ? spotQuote
      : spotBase;
  const side = contract ? actionSide[form.action] : form.side;
  const instrumentPattern =
    form.instType === 'SPOT'
      ? /^[A-Z0-9]{1,32}-[A-Z0-9]{1,32}$/
      : form.instType === 'SWAP'
        ? /^[A-Z0-9]{1,32}-[A-Z0-9]{1,32}-SWAP$/
        : /^[A-Z0-9]{1,32}-[A-Z0-9]{1,32}-[0-9]{6,8}$/;
  const positiveDecimal = /^\d+(?:\.\d+)?$/;
  const draftCanPreview =
    configured &&
    instrumentPattern.test(form.instId) &&
    positiveDecimal.test(form.size) &&
    (form.orderType === 'market' || positiveDecimal.test(form.price)) &&
    (!(!contract && form.orderType === 'market') || validSlippagePercent(form.slippage)) &&
    !tradeWriteBlocked;

  const updateForm = (patch: Partial<FormState>) => {
    if (tradeWriteBlocked) return;
    formRevision.current += 1;
    setForm((current) => ({ ...current, ...patch }));
    setContext(null);
    setReview(null);
    setOutcome(null);
    setOrderDetail(null);
    setDetailChecked(false);
    setCancelOutcome(null);
    setHistory(null);
  };

  const requestContext = (
    environment = form.environment,
    instType = form.instType,
    instId = form.instId.trim().toUpperCase(),
  ) => cryptoApi.tradeContext(instType, instId, environment);

  const readContext = () =>
    void action.run(async () => {
      if (!configured) return;
      const revision = formRevision.current;
      const selection = {
        environment: form.environment,
        instType: form.instType,
        instId: form.instId.trim().toUpperCase(),
      };
      const fresh = await requestContext(
        selection.environment,
        selection.instType,
        selection.instId,
      );
      if (action.active.current && formRevision.current === revision) {
        setContext(fresh);
        setCancelNeedsRefresh(false);
      }
    });

  const createDraft = (): OkxOrderDraft => ({
    environment: form.environment,
    inst_type: form.instType,
    inst_id: form.instId.trim().toUpperCase(),
    side,
    ord_type: form.orderType,
    size: form.size,
    price: form.orderType === 'limit' ? form.price : null,
    slippage_pct: !contract && form.orderType === 'market' ? percentToRatio(form.slippage) : null,
    td_mode: contract ? form.marginMode : 'cash',
    position_action: contract ? form.action : null,
    cl_ord_id: newClientOrderId(),
  });

  const previewOrder = () =>
    void action.run(async () => {
      if (!draftCanPreview || unknownOrder) return;
      const revision = formRevision.current;
      setReview(null);
      setContext(null);
      const draft = createDraft();
      const fresh = await requestContext(draft.environment, draft.inst_type, draft.inst_id);
      if (!action.active.current || formRevision.current !== revision) return;
      setContext(fresh);
      const estimate = await estimateOrder(draft, fresh);
      if (action.active.current && formRevision.current === revision)
        setReview({ draft, context: fresh, estimate });
    });

  const confirmOrder = async (draft: OkxOrderDraft) => {
    let result: OkxOrderSubmission;
    try {
      result = await cryptoApi.placeOrder(draft);
    } catch (cause) {
      const raw = cause instanceof Error ? cause.message : String(cause);
      const reconciliationId = raw.match(
        /order_result_requires_reconciliation:([A-Za-z0-9]{1,32})/,
      )?.[1];
      if (!reconciliationId) throw cause;
      const clientOrderId = reconciliationId ?? draft.cl_ord_id;
      result = {
        status: 'unknown',
        client_order_id: clientOrderId,
        acknowledgement: null,
        error_code: raw,
      };
    }
    setOutcome({ result, environment: draft.environment });
    if (result.status === 'unknown') {
      const clOrdId = result.client_order_id || draft.cl_ord_id;
      setUnknownOrder({ environment: draft.environment, instId: draft.inst_id, clOrdId });
      setDetailChecked(false);
      setOrderDetail(null);
    }
    if (result.status === 'accepted' || result.status === 'unknown') {
      const revision = formRevision.current;
      try {
        const updated = await requestContext(draft.environment, draft.inst_type, draft.inst_id);
        if (formRevision.current === revision) setContext(updated);
      } catch {
        // Keep the exchange result visible; the user can refresh context explicitly.
      }
    }
  };

  const lookupUnknownOrder = () =>
    void action.run(async () => {
      if (!unknownOrder) return;
      const found = await cryptoApi.orderDetail(
        unknownOrder.instId,
        unknownOrder.clOrdId,
        unknownOrder.environment,
      );
      if (action.active.current) {
        setOrderDetail(found);
        setDetailChecked(true);
      }
    });

  const loadHistory = () =>
    void action.run(async () => {
      if (!configured) return;
      const revision = formRevision.current;
      const environment = form.environment;
      const instType = form.instType;
      const rows = await cryptoApi.orderHistory(instType, environment);
      if (action.active.current && formRevision.current === revision) setHistory(rows);
    });

  const confirmCancel = async (selection: CancelSelection) => {
    const { order, environment, instType, instId, revision } = selection;
    let result: OkxOrderActionResult;
    try {
      result = await cryptoApi.cancelOrder(order.inst_id, order.ord_id, environment);
    } catch (cause) {
      const raw = cause instanceof Error ? cause.message : String(cause);
      const reconciliationId = raw.match(/order_cancel_requires_reconciliation:([0-9]{1,64})/)?.[1];
      if (!reconciliationId) throw cause;
      result = {
        status: 'unknown',
        order_id: reconciliationId,
        client_order_id: null,
        code: null,
        message: null,
        error_code: raw,
      };
      setCancelNeedsRefresh(true);
      setCancelTarget(null);
      setCancelOutcome({ result, environment });
      return;
    }
    setCancelOutcome({ result, environment });
    if (result.status === 'accepted' || result.status === 'unknown') {
      try {
        const updated = await requestContext(environment, instType, instId);
        if (formRevision.current === revision) setContext(updated);
      } catch {
        // Keep the cancel result visible if refreshing the pending rows fails.
      }
    }
  };

  return (
    <div className="okx-panel okx-trading-panel">
      <section className="okx-trade-form" aria-label="交易订单表单">
        <div className="okx-controls">
          <label className="form-field">
            交易环境
            <select
              aria-label="交易环境"
              value={form.environment}
              disabled={tradeWriteBlocked}
              onChange={(event) =>
                updateForm({ environment: event.target.value as OkxEnvironment })
              }
            >
              <option value="demo">模拟盘（默认）</option>
              <option value="production">实盘</option>
            </select>
          </label>
          <label className="form-field">
            产品类型
            <select
              aria-label="产品类型"
              value={form.instType}
              disabled={!configured || tradeWriteBlocked}
              onChange={(event) =>
                updateForm({ instType: event.target.value as OkxInstrumentType })
              }
            >
              <option value="SPOT">现货</option>
              <option value="SWAP">永续合约</option>
              <option value="FUTURES">交割合约</option>
            </select>
          </label>
          <label className="form-field">
            交易品种
            <input
              aria-label="交易品种"
              value={form.instId}
              disabled={!configured || tradeWriteBlocked}
              maxLength={80}
              spellCheck={false}
              onChange={(event) => updateForm({ instId: event.target.value.toUpperCase() })}
            />
          </label>
        </div>
        {form.environment === 'production' && (
          <p className="okx-warning" role="alert">
            实盘将使用真实资产下单。请核对交易品种、数量和方向，并逐笔确认订单。
          </p>
        )}
        {!configured ? (
          <p className="okx-credential-prompt">
            请配置 OKX {form.environment === 'demo' ? '模拟盘' : '实盘'}交易
            API，再读取交易上下文或下单。{' '}
            <button className="text-button" type="button" onClick={onConfigure}>
              配置交易凭据
            </button>
          </p>
        ) : (
          <>
            <div className="okx-controls">
              {contract ? (
                <>
                  <label className="form-field">
                    合约操作
                    <select
                      aria-label="合约操作"
                      value={form.action}
                      disabled={tradeWriteBlocked}
                      onChange={(event) =>
                        updateForm({ action: event.target.value as OkxPositionAction })
                      }
                    >
                      <option value="open_long">开多</option>
                      <option value="open_short">开空</option>
                      <option value="close_long">平多</option>
                      <option value="close_short">平空</option>
                    </select>
                  </label>
                  <label className="form-field">
                    保证金模式
                    <select
                      aria-label="保证金模式"
                      value={form.marginMode}
                      disabled={tradeWriteBlocked}
                      onChange={(event) =>
                        updateForm({ marginMode: event.target.value as 'cross' | 'isolated' })
                      }
                    >
                      <option value="cross">全仓</option>
                      <option value="isolated">逐仓</option>
                    </select>
                  </label>
                  <p className="field-hint">
                    操作 {actionLabel[form.action]}，订单方向为{side === 'buy' ? '买入' : '卖出'}。
                  </p>
                </>
              ) : (
                <label className="form-field">
                  交易方向
                  <select
                    aria-label="交易方向"
                    value={form.side}
                    disabled={tradeWriteBlocked}
                    onChange={(event) => updateForm({ side: event.target.value as 'buy' | 'sell' })}
                  >
                    <option value="buy">买入</option>
                    <option value="sell">卖出</option>
                  </select>
                </label>
              )}
              <label className="form-field">
                订单类型
                <select
                  aria-label="订单类型"
                  value={form.orderType}
                  disabled={tradeWriteBlocked}
                  onChange={(event) =>
                    updateForm({ orderType: event.target.value as 'market' | 'limit' })
                  }
                >
                  <option value="market">市价</option>
                  <option value="limit">限价</option>
                </select>
              </label>
              <label className="form-field">
                委托数量（{sizeUnit}）
                <input
                  aria-label={`委托数量（${sizeUnit}）`}
                  type="text"
                  inputMode="decimal"
                  autoComplete="off"
                  spellCheck={false}
                  value={form.size}
                  disabled={tradeWriteBlocked}
                  onChange={(event) => updateForm({ size: event.target.value })}
                />
              </label>
              {form.orderType === 'limit' && (
                <label className="form-field">
                  委托价格（{priceUnit}）
                  <input
                    aria-label={`委托价格（${priceUnit}）`}
                    type="text"
                    inputMode="decimal"
                    autoComplete="off"
                    spellCheck={false}
                    value={form.price}
                    disabled={tradeWriteBlocked}
                    onChange={(event) => updateForm({ price: event.target.value })}
                  />
                </label>
              )}
              {!contract && form.orderType === 'market' && (
                <label className="form-field">
                  最大滑点（%）
                  <input
                    aria-label="最大滑点（%）"
                    type="text"
                    inputMode="decimal"
                    autoComplete="off"
                    value={form.slippage}
                    disabled={tradeWriteBlocked}
                    onChange={(event) => updateForm({ slippage: event.target.value })}
                  />
                </label>
              )}
            </div>
            {!contract && form.orderType === 'market' && (
              <p className="field-hint">
                {form.side === 'buy'
                  ? `市价买入数量表示预计收到的 ${spotBase}。`
                  : `市价卖出数量表示卖出后收到的 ${spotQuote}。`}
              </p>
            )}
            <div className="okx-actions">
              <button
                className="button"
                type="button"
                disabled={action.busy || !!unknownOrder || !instrumentPattern.test(form.instId)}
                onClick={readContext}
              >
                {action.busy ? '正在读取…' : '读取交易上下文'}
              </button>
              <button
                className="button"
                type="button"
                disabled={action.busy || !draftCanPreview}
                onClick={previewOrder}
              >
                预览订单
              </button>
              <button className="button" type="button" disabled={action.busy} onClick={loadHistory}>
                {history ? '刷新历史订单' : '读取历史订单'}
              </button>
            </div>
            <p className="field-hint">
              当前环境：{form.environment === 'demo' ? '模拟盘' : '实盘'} ·{' '}
              {context?.account.pos_mode === 'net_mode' ? '单向持仓' : '双向持仓'}
            </p>
            {action.error && <p role="alert">{action.error}</p>}
            {context && (
              <TradeContextView
                context={context}
                allowCancel={!tradeWriteBlocked}
                onCancel={(order) =>
                  setCancelTarget({
                    order,
                    environment: form.environment,
                    instType: form.instType,
                    instId: form.instId,
                    revision: formRevision.current,
                  })
                }
              />
            )}
            {outcome && <OrderOutcomeView outcome={outcome} />}
            {unknownOrder && (
              <section className="okx-unknown-order" aria-label="待核对订单">
                <p className="okx-warning" role="alert">
                  结果未确认。系统不会再次提交，请先按相同客户订单号查询 OKX 状态。
                </p>
                <label className="form-field">
                  待核对客户订单号
                  <input aria-label="待核对客户订单号" readOnly value={unknownOrder.clOrdId} />
                </label>
                <button
                  className="button"
                  type="button"
                  disabled={action.busy}
                  onClick={lookupUnknownOrder}
                >
                  查询此订单状态
                </button>
                {detailChecked &&
                  (orderDetail ? (
                    <>
                      <p role="status">
                        OKX 订单状态：<strong>{orderDetail.state}</strong> · 已成交数量{' '}
                        {orderDetail.accumulated_fill_size} / {orderDetail.sz}
                      </p>
                      <button
                        className="button"
                        type="button"
                        onClick={() => setUnknownOrder(null)}
                      >
                        已核对订单状态，允许新订单预览
                      </button>
                    </>
                  ) : (
                    <p role="status">
                      OKX 暂未查询到此订单，提交结果仍未确认；查询到订单前不能再次提交。
                    </p>
                  ))}
              </section>
            )}
            {orderDetail && !unknownOrder && (
              <p role="status">
                OKX 订单状态：<strong>{orderDetail.state}</strong> · 订单 {orderDetail.order_id}
              </p>
            )}
            {cancelOutcome && (
              <CancelOutcomeView
                outcome={cancelOutcome.result}
                environment={cancelOutcome.environment}
              />
            )}
            {cancelNeedsRefresh && (
              <section className="okx-unknown-order" aria-label="待核对撤单">
                <p role="alert">
                  撤单结果未确认，订单 {cancelOutcome?.result.order_id}
                  。请先重新读取交易上下文；确认挂单状态后再进行操作。
                </p>
                <button
                  className="button"
                  type="button"
                  disabled={action.busy || !instrumentPattern.test(form.instId)}
                  onClick={readContext}
                >
                  重新读取交易上下文
                </button>
              </section>
            )}
            {history && <OrderHistory rows={history} />}
          </>
        )}
      </section>
      {review && (
        <OrderReviewDialog
          draft={review.draft}
          instrument={review.context.instrument}
          estimate={review.estimate}
          onClose={() => setReview(null)}
          onConfirm={() => confirmOrder(review.draft)}
        />
      )}
      {cancelTarget && (
        <CancelOrderDialog
          order={cancelTarget.order}
          environment={cancelTarget.environment}
          onClose={() => setCancelTarget(null)}
          onConfirm={() => confirmCancel(cancelTarget)}
        />
      )}
    </div>
  );
}

function TradeContextView({
  context,
  allowCancel,
  onCancel,
}: {
  context: OkxTradeContext;
  allowCancel: boolean;
  onCancel: (order: OkxTradeContext['account']['pending_orders'][number]) => void;
}) {
  const { instrument, account } = context;
  return (
    <section className="okx-trade-context" aria-label="最新交易上下文">
      <h4>最新交易上下文 · {instrument.inst_id}</h4>
      <dl className="okx-values">
        <div>
          <dt>产品状态</dt>
          <dd>{instrument.state}</dd>
        </div>
        <div>
          <dt>最小数量 / 数量步长</dt>
          <dd>
            {instrument.min_sz} / {instrument.lot_sz}
          </dd>
        </div>
        <div>
          <dt>价格步长</dt>
          <dd>{instrument.tick_sz}</dd>
        </div>
        <div>
          <dt>账户模式</dt>
          <dd>
            {account.acct_lv} · {account.pos_mode}
          </dd>
        </div>
        {instrument.ct_val && (
          <div>
            <dt>合约面值</dt>
            <dd>
              {instrument.ct_val} {instrument.ct_val_ccy} · {instrument.ct_type}
            </dd>
          </div>
        )}
      </dl>
      <section aria-label="当前持仓">
        <h4>当前持仓</h4>
        {account.positions.length ? (
          <ul className="okx-trade-rows">
            {account.positions.map((position, index) => (
              <li key={`${position.inst_id}-${position.pos_side}-${index}`}>
                {position.inst_id} · {position.pos_side} · {position.quantity} 张 ·{' '}
                {modeLabel[position.margin_mode]}
              </li>
            ))}
          </ul>
        ) : (
          <p>暂无持仓。</p>
        )}
      </section>
      <section aria-label="当前挂单">
        <h4>当前挂单</h4>
        {account.pending_orders.length ? (
          <div className="okx-card-list">
            {account.pending_orders.map((order) => (
              <article className="okx-record" key={`${order.inst_id}-${order.ord_id}`}>
                <header>
                  <strong>{order.inst_id}</strong>
                  <span>
                    {order.state} · {order.side} · 剩余 {order.remaining_size}
                  </span>
                </header>
                <p>
                  订单 {order.ord_id} · {order.pos_side ?? 'net'} · {modeLabel[order.margin_mode]} ·{' '}
                  {order.reduce_only ? '只减仓' : '普通订单'}
                </p>
                {allowCancel &&
                  (order.state === 'live' || order.state === 'partially_filled') &&
                  hasPositiveRemainder(order.remaining_size) && (
                    <button className="button" type="button" onClick={() => onCancel(order)}>
                      撤单
                    </button>
                  )}
              </article>
            ))}
          </div>
        ) : (
          <p>暂无未完成订单。</p>
        )}
      </section>
      <section aria-label="杠杆信息">
        <h4>当前杠杆信息</h4>
        {account.leverage.length ? (
          <ul className="okx-trade-rows">
            {account.leverage.map((leverage, index) => (
              <li key={`${leverage.inst_id}-${leverage.margin_mode}-${index}`}>
                {leverage.inst_id} · {leverage.pos_side ?? 'net'} ·{' '}
                {modeLabel[leverage.margin_mode]} · {leverage.leverage} 倍
              </li>
            ))}
          </ul>
        ) : (
          <p>OKX 未返回杠杆信息。</p>
        )}
      </section>
    </section>
  );
}

function OrderOutcomeView({ outcome }: { outcome: OrderOutcome }) {
  const result = outcome.result;
  return (
    <section className="okx-order-outcome" aria-label="下单结果">
      {outcome.environment === 'production' && (
        <p className="okx-warning" role="alert">
          实盘订单使用真实资产，请在 OKX 账户中核对订单和成交状态。
        </p>
      )}
      {result.status === 'accepted' && <p className="okx-notice">订单已提交，尚未确认成交</p>}
      {result.status === 'rejected' && (
        <p role="alert">
          订单被 OKX 拒绝：{result.acknowledgement?.code ?? result.error_code ?? '未知错误'}{' '}
          {result.acknowledgement?.message ?? ''}
        </p>
      )}
      {result.acknowledgement && (
        <p>
          OKX 订单号 {result.acknowledgement.order_id} · 客户订单号{' '}
          {result.acknowledgement.client_order_id} · sCode {result.acknowledgement.code}
          {result.acknowledgement.message ? ` · ${result.acknowledgement.message}` : ''}
        </p>
      )}
    </section>
  );
}

type CancelOrderDialogProps = {
  order: OkxTradeContext['account']['pending_orders'][number];
  environment: OkxEnvironment;
  onClose: () => void;
  onConfirm: () => Promise<void>;
};

function CancelOrderDialog(props: CancelOrderDialogProps) {
  return (
    <Dialog title="确认撤单" onClose={props.onClose}>
      <CancelOrderDialogBody {...props} />
    </Dialog>
  );
}

function CancelOrderDialogBody({ order, environment, onConfirm }: CancelOrderDialogProps) {
  const [error, setError] = useState('');
  const { busy, setBusy, close } = useDialog();
  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      await onConfirm();
      setBusy(false);
      close();
    } catch (cause) {
      setError(okxError(cause));
      setBusy(false);
    }
  };
  return (
    <div className="okx-review">
      {environment === 'production' && (
        <p className="okx-warning" role="alert">
          实盘撤单将影响真实订单，请核对订单编号后确认。
        </p>
      )}
      <p>
        即将撤销 {order.inst_id} 的订单 {order.ord_id}。
      </p>
      {error && <p role="alert">{error}</p>}
      <div className="okx-actions">
        <button className="button" type="button" disabled={busy} onClick={close}>
          返回
        </button>
        <button
          className="button button-primary"
          type="button"
          disabled={busy}
          onClick={() => void confirm()}
        >
          {busy ? '正在撤单…' : '确认撤销订单'}
        </button>
      </div>
    </div>
  );
}

function CancelOutcomeView({
  outcome,
  environment,
}: {
  outcome: OkxOrderActionResult;
  environment: OkxEnvironment;
}) {
  if (environment === 'production')
    return (
      <p className="okx-warning" role={outcome.status === 'rejected' ? 'alert' : 'status'}>
        实盘撤单结果：{cancelStatus(outcome.status)} · 订单 {outcome.order_id}
        {outcome.code ? ` · ${outcome.code}` : ''} {outcome.message ?? ''}
      </p>
    );
  return (
    <p role={outcome.status === 'rejected' ? 'alert' : 'status'}>
      {outcome.status === 'accepted'
        ? '撤单请求已接受'
        : outcome.status === 'unknown'
          ? `撤单结果未确认，订单 ${outcome.order_id}`
          : `撤单被拒绝：${outcome.code ?? outcome.error_code ?? ''} ${outcome.message ?? ''}`}
    </p>
  );
}

function OrderHistory({ rows }: { rows: OkxOrderSnapshot[] }) {
  return (
    <section aria-label="历史订单">
      <h4>历史订单（OKX 状态）</h4>
      {rows.length ? (
        <div className="okx-card-list">
          {rows.map((order) => (
            <article className="okx-record" key={`${order.inst_id}-${order.order_id}`}>
              <header>
                <strong>{order.inst_id}</strong>
                <span>{order.state}</span>
              </header>
              <p>
                {order.side} · {order.order_type} · {order.sz} · 已成交{' '}
                {order.accumulated_fill_size} · 均价 {order.average_price || '—'}
              </p>
              <small className="field-hint">
                订单 {order.order_id} · 客户订单号 {order.client_order_id || '—'}
              </small>
            </article>
          ))}
        </div>
      ) : (
        <p>暂无历史订单。</p>
      )}
    </section>
  );
}

async function estimateOrder(draft: OkxOrderDraft, context: OkxTradeContext): Promise<string> {
  const { instrument } = context;
  if (draft.inst_type !== 'SPOT') {
    if (!instrument.ct_val || !instrument.ct_val_ccy) return '合约面值信息不可用';
    const value = multiplyDecimals(draft.size, instrument.ct_val);
    return value ? `${value} ${instrument.ct_val_ccy} 合约面值` : '合约面值暂不可估算';
  }
  if (draft.ord_type === 'limit' && draft.price) {
    const total = multiplyDecimals(draft.size, draft.price);
    return total ? `${total} ${instrument.quote_ccy ?? '报价币'}` : '限价估算暂不可用';
  }
  if (draft.side === 'sell')
    return `${draft.size} ${instrument.quote_ccy ?? '报价币'}（市价卖出收到的报价币数量估算）`;
  try {
    const ticker = (await cryptoApi.tickers()).find((row) => row.instrument === draft.inst_id);
    if (!ticker) return `${draft.size} ${instrument.base_ccy ?? '基础币'}；当前行情估算不可用`;
    const total = multiplyDecimals(draft.size, ticker.last);
    return total
      ? `约 ${total} ${instrument.quote_ccy ?? '报价币'}（公开行情参考价 ${ticker.last}）`
      : `${draft.size} ${instrument.base_ccy ?? '基础币'}；估算不可用`;
  } catch {
    return `${draft.size} ${instrument.base_ccy ?? '基础币'}；当前行情估算不可用`;
  }
}

function multiplyDecimals(left: string, right: string): string | null {
  if (!/^\d+(?:\.\d+)?$/.test(left) || !/^\d+(?:\.\d+)?$/.test(right)) return null;
  const [leftWhole, leftFraction = ''] = left.split('.');
  const [rightWhole, rightFraction = ''] = right.split('.');
  const scale = leftFraction.length + rightFraction.length;
  const product = BigInt(leftWhole + leftFraction) * BigInt(rightWhole + rightFraction);
  const digits = product.toString().padStart(scale + 1, '0');
  if (scale === 0) return digits;
  const whole = digits.slice(0, -scale);
  const fraction = digits.slice(-scale).replace(/0+$/, '');
  return fraction ? `${whole}.${fraction}` : whole;
}

function hasPositiveRemainder(value: string): boolean {
  if (!/^\d+(?:\.\d+)?$/.test(value)) return false;
  return /[1-9]/.test(value.replace('.', ''));
}

function validSlippagePercent(value: string): boolean {
  const fraction = value.split('.')[1] ?? '';
  return (
    /^\d+(?:\.\d+)?$/.test(value) && fraction.length <= 2 && compareDecimalStrings(value, '5') <= 0
  );
}

function percentToRatio(value: string): string {
  const [whole, fraction = ''] = value.split('.');
  const digits = `${whole}${fraction}`;
  const scale = fraction.length + 2;
  const padded = digits.padStart(scale + 1, '0');
  const ratioWhole = padded.slice(0, -scale).replace(/^0+(?=\d)/, '');
  const ratioFraction = padded.slice(-scale).replace(/0+$/, '');
  return ratioFraction ? `${ratioWhole}.${ratioFraction}` : ratioWhole;
}

function compareDecimalStrings(left: string, right: string): number {
  const [leftWhole, leftFraction = ''] = left.split('.');
  const [rightWhole, rightFraction = ''] = right.split('.');
  const scale = Math.max(leftFraction.length, rightFraction.length);
  const leftScaled = BigInt(leftWhole + leftFraction.padEnd(scale, '0'));
  const rightScaled = BigInt(rightWhole + rightFraction.padEnd(scale, '0'));
  return leftScaled < rightScaled ? -1 : leftScaled > rightScaled ? 1 : 0;
}

function newClientOrderId(): string {
  const source = globalThis.crypto;
  if (typeof source?.randomUUID === 'function') return source.randomUUID().replaceAll('-', '');
  return Array.from(source.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, '0'),
  ).join('');
}

function cancelStatus(status: OkxOrderActionResult['status']): string {
  return status === 'accepted' ? '请求已接受' : status === 'unknown' ? '结果未确认' : '已拒绝';
}

function quoteCurrencyFromId(instId: string): string | null {
  return instId.trim().toUpperCase().split('-')[1] || null;
}
