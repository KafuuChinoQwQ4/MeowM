import { invoke } from './client';

export interface ScreenCaptureStatus {
  android: boolean;
  running: boolean;
  usage_granted: boolean;
  code: string;
}

export interface ScreenCaptureSource {
  package_name: string;
  enabled: boolean;
  account_id: string | null;
}

export interface ScreenCaptureState {
  sources: ScreenCaptureSource[];
  latest: Array<{
    package_name: string;
    code: string;
    received_at_ms: number;
    transaction_id: string | null;
  }>;
}

export const screenCaptureApi = {
  state: () => invoke<ScreenCaptureState>('screen_capture_state'),
  status: () => invoke<ScreenCaptureStatus>('screen_capture_status'),
  usageSettings: () => invoke<void>('screen_capture_usage_settings'),
  save: (source: ScreenCaptureSource, consent: boolean) =>
    invoke<void>('save_screen_capture_source', { source, consent }),
  start: (consent: boolean) => invoke<void>('start_screen_capture', { consent }),
  stop: () => invoke<void>('stop_screen_capture'),
};
