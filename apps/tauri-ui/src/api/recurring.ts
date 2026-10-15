import { invoke } from './client';
export type Cadence = 'weekly' | 'monthly' | 'yearly';
export interface RecurringInput {
  id?: string;
  expected_version?: number;
  account_id: string;
  kind: 'income' | 'expense';
  amount: string;
  category_id: string | null;
  merchant: string;
  description: string;
  cadence: Cadence;
  interval: number;
  next_due_date: string;
  end_date: string | null;
  enabled: boolean;
}
export interface RecurringTemplate extends Omit<RecurringInput, 'id' | 'expected_version'> {
  id: string;
  version: number;
  category_name: string;
  ended: boolean;
}
export interface RecurringReminder {
  template_id: string;
  due_date: string;
  transaction_id: string | null;
  account_id: string;
  amount: string;
  merchant: string | null;
  description: string | null;
  overdue: boolean;
}
export interface RecurringState {
  templates: RecurringTemplate[];
  reminders: RecurringReminder[];
  generated_count: number;
  has_more: boolean;
}
export interface RecurringInstance {
  due_date: string;
  transaction_id: string | null;
  status: 'pending' | 'confirmed' | 'ignored' | 'skipped';
  amount: string | null;
}
export const recurringApi = {
  state: () => invoke<RecurringState>('recurring_state'),
  save: (request: RecurringInput) => invoke<string>('write_recurring', { request }),
  enabled: (t: RecurringTemplate, enabled: boolean) =>
    invoke<void>('set_recurring_enabled', { id: t.id, version: t.version, enabled }),
  skip: (t: RecurringTemplate) =>
    invoke<void>('skip_recurring', { id: t.id, dueDate: t.next_due_date, version: t.version }),
  instances: (id: string) => invoke<RecurringInstance[]>('recurring_instances', { id }),
};
