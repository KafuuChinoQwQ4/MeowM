import { useState } from 'react';
import type { OkxInstrumentSpec, OkxOrderDraft } from '../../../api/crypto';
import { Dialog, useDialog } from '../../../components/Dialog';
import { okxError } from './shared';

const actionNames = {
  open_long: '开多',
  open_short: '开空',
  close_long: '平多',
  close_short: '平空',
} as const;

type OrderReviewDialogProps = {
  draft: OkxOrderDraft;
  instrument: OkxInstrumentSpec;
  estimate: string;
  onClose: () => void;
  onConfirm: () => Promise<void>;
};

export function OrderReviewDialog(props: OrderReviewDialogProps) {
  return (
    <Dialog title="确认 OKX 订单" onClose={props.onClose}>
      <OrderReviewBody {...props} />
    </Dialog>
  );
}

function OrderReviewBody({ draft, instrument, estimate, onConfirm }: OrderReviewDialogProps) {
  const [error, setError] = useState('');
  const { busy, setBusy, close } = useDialog();
  const baseUnit = instrument.base_ccy ?? 'base currency';
  const quoteUnit =
    instrument.quote_ccy ??
    quoteCurrencyFromId(draft.inst_id) ??
    (draft.inst_type === 'SPOT' ? 'quote currency' : '价格币种未知');
  const sizeUnit =
    draft.inst_type !== 'SPOT'
      ? '张'
      : draft.ord_type === 'market' && draft.side === 'sell'
        ? quoteUnit
        : baseUnit;
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
      {draft.environment === 'production' && (
        <p className="okx-warning" role="alert">
          实盘订单会使用真实资产下单，请核对全部字段后再确认。
        </p>
      )}
      <dl className="okx-values">
        <div>
          <dt>交易环境</dt>
          <dd>{draft.environment === 'demo' ? '模拟盘' : '实盘'}</dd>
        </div>
        <div>
          <dt>交易品种</dt>
          <dd>
            {draft.inst_id} · {draft.inst_type}
          </dd>
        </div>
        <div>
          <dt>方向 / 操作</dt>
          <dd>
            {draft.side === 'buy' ? '买入' : '卖出'}
            {draft.position_action ? ` · ${actionNames[draft.position_action]}` : ''}
          </dd>
        </div>
        <div>
          <dt>订单类型</dt>
          <dd>{draft.ord_type === 'market' ? '市价' : '限价'}</dd>
        </div>
        <div>
          <dt>数量及单位</dt>
          <dd>
            {draft.size} {sizeUnit}
          </dd>
        </div>
        <div>
          <dt>价格 / 滑点</dt>
          <dd>
            {draft.ord_type === 'limit' ? `${draft.price} ${quoteUnit}` : '市价执行'}
            {draft.slippage_pct ? ` · 滑点上限 ${ratioAsPercent(draft.slippage_pct)}%` : ''}
          </dd>
        </div>
        <div>
          <dt>保证金模式</dt>
          <dd>
            {draft.td_mode === 'cash'
              ? '现货现金账户'
              : draft.td_mode === 'cross'
                ? '全仓'
                : draft.td_mode === 'isolated'
                  ? '逐仓'
                  : '—'}
          </dd>
        </div>
        <div>
          <dt>产品规则</dt>
          <dd>
            状态 {instrument.state} · 最小数量 {instrument.min_sz} · 数量步长 {instrument.lot_sz} ·
            价格步长 {instrument.tick_sz}
          </dd>
        </div>
        {instrument.ct_val && (
          <div>
            <dt>合约面值</dt>
            <dd>
              {instrument.ct_val} {instrument.ct_val_ccy}
            </dd>
          </div>
        )}
      </dl>
      <p className="okx-estimate">估算（参考，不代表成交价格）：{estimate}</p>
      {error && <p role="alert">{error}</p>}
      <div className="okx-actions">
        <button className="button" type="button" disabled={busy} onClick={close}>
          返回修改
        </button>
        <button
          className="button button-primary"
          type="button"
          disabled={busy}
          onClick={() => void confirm()}
        >
          {busy ? '正在提交…' : '确认并下单'}
        </button>
      </div>
    </div>
  );
}

function ratioAsPercent(value: string): string {
  const [whole, fraction = ''] = value.split('.');
  const scale = fraction.length;
  const scaled = (BigInt(whole + fraction) * 100n).toString().padStart(scale + 1, '0');
  if (scale === 0) return scaled;
  const percentWhole = scaled.slice(0, -scale);
  const percentFraction = scaled.slice(-scale).replace(/0+$/, '');
  return percentFraction ? `${percentWhole}.${percentFraction}` : percentWhole;
}

function quoteCurrencyFromId(instId: string): string | null {
  return instId.trim().toUpperCase().split('-')[1] || null;
}
