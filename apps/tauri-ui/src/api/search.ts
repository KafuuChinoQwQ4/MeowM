import { invoke } from './client';
export interface Category {
  id: string;
  name: string;
  enabled: boolean;
}
export interface RuleInput {
  id?: string;
  priority: number;
  merchant_contains: string | null;
  description_contains: string | null;
  category_id: string;
  enabled: boolean;
}
export interface Rule extends RuleInput {
  id: string;
}
export interface MerchantAlias {
  alias: string;
  canonical: string;
}
export interface ClassificationState {
  categories: Category[];
  rules: Rule[];
  aliases: MerchantAlias[];
}
export interface RulePreview {
  match_count: number;
  pending_count: number;
  pending_ids: string[];
  pending_samples: {
    id: string;
    merchant: string | null;
    description: string | null;
    amount: string | null;
  }[];
}
export const searchApi = {
  state: () => invoke<ClassificationState>('classification_state'),
  preview: (request: RuleInput) => invoke<RulePreview>('preview_classification_rule', { request }),
  rule: (request: RuleInput) => invoke<string>('write_classification_rule', { request }),
  category: (name: string, enabled: boolean, id?: string) =>
    invoke<string>('write_category', { id: id ?? null, name, enabled }),
  deleteCategory: (id: string) => invoke<void>('delete_category', { id }),
  alias: (alias: string, canonical: string) =>
    invoke<void>('save_merchant_alias', { alias, canonical }),
  deleteAlias: (alias: string) => invoke<void>('delete_merchant_alias', { alias }),
};
