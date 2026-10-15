import { invoke } from './client';
export interface BudgetInput {
  id?: string;
  from_date: string;
  until_date: string;
  account_id: string | null;
  category_id: string | null;
  amount: string;
  enabled: boolean;
  custom_period: boolean;
}
export interface Budget extends BudgetInput {
  id: string;
  category_name: string;
  spent: string;
  remaining: string;
  percent: string;
  exceeded: boolean;
}
export interface BudgetReminder {
  id: string;
  budget_id: string;
  level: 'warning' | 'exhausted' | 'exceeded';
  spent: string;
  amount: string;
}
export const budgetsApi = {
  list: () => invoke<Budget[]>('list_budgets'),
  save: (request: BudgetInput) => invoke<string>('write_budget', { request }),
  archive: (id: string) => invoke<void>('archive_budget', { id }),
  reminders: () => invoke<BudgetReminder[]>('budget_reminders'),
};
