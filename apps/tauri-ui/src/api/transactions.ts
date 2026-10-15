import { invoke } from './client';
export interface TransactionDto {
  id: string;
  kind: string;
  status: 'pending' | 'confirmed' | 'ignored';
  amount: string;
  date: string;
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
  sources: { source: string; external_id?: string | null; digest?: string | null }[];
  history: { id: string; action: string; time: string; detail: string }[];
}
interface RawTransactionDetail extends Omit<TransactionDto, 'to_account_id'> {
  destination_id: string | null;
  reversal_of: string | null;
  reversal_id: string | null;
  linked_ids: string[];
  sources: { source: string; external_id?: string | null; digest?: string | null }[];
  history: [string, number][];
}
export interface TransactionWrite {
  id?: string;
  expected_version: number;
  kind: string;
  amount: string;
  date: string;
  account_id: string;
  to_account_id?: string | null;
  category_id?: string | null;
  merchant?: string;
  description?: string;
  original_transaction_id?: string | null;
}
export const transactionsApi = {
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
      },
    }),
  reverse: (id: string, reason: string) => invoke<string>('reverse_transaction', { id, reason }),
};
