import { invoke } from './client';
export interface TransactionDto {
  id: string;
  kind: string;
  status: 'pending' | 'confirmed' | 'ignored';
  amount: string;
  currency_code: string;
  report_amount_cny?: string | null;
  fx_snapshot?: {
    rate: string;
    effective_date: string;
    source: string;
    provider?: string | null;
    provider_sources: string[];
  } | null;
  date: string;
  time?: string;
  account_id: string;
  to_account_id?: string | null;
  category_id?: string | null;
  merchant?: string | null;
  description?: string | null;
  original_transaction_id?: string | null;
  edit_version: number;
}
export interface TransactionDetail {
  transaction: TransactionDto;
  edit_version: number;
  reversal_of: string | null;
  reversal_id: string | null;
  related_ids: string[];
  sources: {
    source: string;
    external_id?: string | null;
    merchant_order_id?: string | null;
    digest?: string | null;
  }[];
  history: { id: string; action: string; time: string; detail: string }[];
}
interface RawTransactionDetail extends Omit<TransactionDto, 'to_account_id'> {
  destination_id: string | null;
  reversal_of: string | null;
  reversal_id: string | null;
  linked_ids: string[];
  sources: {
    source: string;
    external_id?: string | null;
    merchant_order_id?: string | null;
    digest?: string | null;
  }[];
  history: [string, number][];
}
export interface ForeignPreview {
  amount: string;
  currency_code: string;
  report_amount_cny: string;
  rate: string;
  source: 'reference' | 'manual';
  effective_date: string;
  fx_snapshot: NonNullable<TransactionDto['fx_snapshot']>;
}
export interface TransactionWrite {
  id?: string;
  expected_version: number;
  kind: string;
  amount: string;
  date: string;
  time?: string;
  source?: string;
  transaction_number?: string;
  merchant_order_id?: string;
  account_id: string;
  to_account_id?: string | null;
  category_id?: string | null;
  merchant?: string;
  description?: string;
  original_transaction_id?: string | null;
  manual_fx_rate?: string;
}
export const transactionsApi = {
  previewForeign: (amount: string, quoteCurrency: string, date: string, manualFxRate?: string) =>
    invoke<ForeignPreview>('preview_foreign_transaction', {
      amount,
      quoteCurrency,
      date,
      manualFxRate: manualFxRate || null,
    }),
  detail: async (id: string): Promise<TransactionDetail> => {
    const raw = await invoke<RawTransactionDetail>('transaction_detail', { id });
    return {
      transaction: { ...raw, to_account_id: raw.destination_id },
      edit_version: raw.edit_version,
      reversal_of: raw.reversal_of,
      reversal_id: raw.reversal_id,
      related_ids: raw.linked_ids,
      sources: raw.sources,
      history: raw.history.map(([action, micros], index) => ({
        id: String(index),
        action,
        time: new Date(micros / 1000).toISOString(),
        detail: action,
      })),
    };
  },
  save: (payload: TransactionWrite) =>
    invoke<string>('write_transaction', {
      request: {
        edit: {
          id: payload.id ?? '',
          expected_version: payload.expected_version,
          account_id: payload.account_id,
          date: payload.date,
          amount: payload.amount,
          kind: payload.kind,
          merchant: payload.merchant ?? '',
          description: payload.description ?? '',
          category_id: payload.category_id ?? null,
          original_transaction_id: payload.original_transaction_id ?? null,
        },
        destination_id: payload.to_account_id ?? null,
        source: payload.source || null,
        transaction_number: payload.transaction_number?.trim() || null,
        merchant_order_id: payload.merchant_order_id?.trim() || null,
        time: payload.time || null,
        manual_fx_rate: payload.manual_fx_rate || null,
      },
    }),
  reverse: (id: string, reason: string) => invoke<string>('reverse_transaction', { id, reason }),
};
