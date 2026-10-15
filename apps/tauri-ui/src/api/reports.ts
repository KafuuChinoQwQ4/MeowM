import { invoke } from './client';
export interface ReportQuery {
  from_date: string;
  until_date: string;
  account_id?: string;
  category_id?: string;
  merchant?: string;
  merchant_offset: number;
  merchant_limit: number;
}
export interface Report {
  income: string;
  expense: string;
  net: string;
  transaction_count: number;
  categories: { id: string | null; name: string; expense: string; share_percent: number }[];
  merchants: { name: string; income: string; expense: string }[];
  merchant_total: number;
  trend: { month: string; income: string; expense: string }[];
  balances: { account_id: string; name: string; amount: string | null }[];
}
export const reportsApi = {
  summary: (query: ReportQuery) => invoke<Report>('report_summary', { query }),
};
