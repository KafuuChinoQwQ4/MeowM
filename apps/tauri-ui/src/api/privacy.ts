import { invoke } from './client';
export interface PrivacyStatus {
  locked: boolean;
  lock_enabled: boolean;
  hide_amounts: boolean;
  auto_lock_minutes: number;
}
export interface DeletePreview {
  id: string;
  scope: 'notifications' | 'all';
  counts: [string, number][];
  pending: number;
  confirmed: number;
  receipts: number;
  blocked: number;
}
export const privacyApi = {
  status: () => invoke<PrivacyStatus>('privacy_status'),
  unlock: (password: string) => invoke<PrivacyStatus>('privacy_unlock', { password }),
  lock: () => invoke<PrivacyStatus>('privacy_lock_now'),
  touch: () => invoke<void>('privacy_touch'),
  enable: (password: string) => invoke<PrivacyStatus>('privacy_enable_lock', { password }),
  disable: (password: string) => invoke<PrivacyStatus>('privacy_disable_lock', { password }),
  hide: (hidden: boolean) => invoke<PrivacyStatus>('privacy_set_hide_amounts', { hidden }),
  timeout: (minutes: number) => invoke<PrivacyStatus>('privacy_set_auto_lock_minutes', { minutes }),
  previewDelete: (scope: 'notifications' | 'all') =>
    invoke<DeletePreview>('privacy_delete_preview', { scope }),
  confirmDelete: (previewId: string, phrase: string) =>
    invoke<PrivacyStatus>('privacy_delete_confirm', { previewId, phrase }),
};
