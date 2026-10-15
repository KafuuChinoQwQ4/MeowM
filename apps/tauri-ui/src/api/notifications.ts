import { invoke } from './client';
export interface NotificationSource {
  package_name: string;
  enabled: boolean;
  account_id: string | null;
}
export interface CaptureStatus {
  package_name: string;
  code: string;
  received_at_ms: number;
  transaction_id: string | null;
}
export interface NotificationState {
  sources: NotificationSource[];
  latest: CaptureStatus[];
}
export interface NotificationAccess {
  android: boolean;
  granted: boolean;
  connected: boolean;
}
export const notificationsApi = {
  state: () => invoke<NotificationState>('notification_state'),
  access: () => invoke<NotificationAccess>('notification_access'),
  save: (source: NotificationSource, consent: boolean) =>
    invoke<void>('save_notification_source', { source, consent }),
  openSettings: () => invoke<void>('open_notification_settings'),
};
