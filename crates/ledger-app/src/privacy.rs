//! Process-local app lock. Only an Argon2id verifier is persisted; plaintext stays in the caller.
use crate::{FeatureRepository, RepositoryError, ports::PrivacyRepository};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use ledger_domain::PrivacySettings;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum PrivacyError {
    Locked,
    WrongPassword,
    RateLimited,
    InvalidInput,
    Changed,
    Blocked,
    Storage,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDeletePreview {
    pub pending: usize,
    pub confirmed: usize,
    pub receipts: usize,
    pub blocked: usize,
}
impl From<RepositoryError> for PrivacyError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::Conflict => Self::Changed,
            RepositoryError::InvalidData(_) => Self::Blocked,
            _ => Self::Storage,
        }
    }
}
impl std::fmt::Display for PrivacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Locked => "应用已锁定，请先解锁",
            Self::WrongPassword => "密码错误，请重试",
            Self::RateLimited => "尝试过多，请稍后重试",
            Self::InvalidInput => {
                "密码须为 8 至 256 字节；自动锁定为 1 至 120 分钟，且仅能在未启用时设置新密码"
            }
            Self::Changed => "预览后账本已变化，请重新检查删除范围",
            Self::Blocked => "记录被历史对账或其他来源引用，不能单独删除",
            Self::Storage => "无法保存隐私设置，请重试",
        })
    }
}
impl std::error::Error for PrivacyError {}

pub struct PrivacyService {
    settings: PrivacySettings,
    locked: bool,
    last_activity: Instant,
    failures: u8,
    retry_after: Option<Instant>,
}
impl PrivacyService {
    pub fn load(repo: &impl FeatureRepository) -> Result<Self, PrivacyError> {
        let settings = repo.privacy_settings()?;
        Ok(Self {
            locked: settings.lock_enabled,
            settings,
            last_activity: Instant::now(),
            failures: 0,
            retry_after: None,
        })
    }
    pub fn settings(&self) -> &PrivacySettings {
        &self.settings
    }
    pub fn reload_settings(&mut self, repo: &impl FeatureRepository) -> Result<(), PrivacyError> {
        self.settings = repo.privacy_settings()?;
        Ok(())
    }
    pub fn is_locked(&self) -> bool {
        self.locked
    }
    pub fn require_unlocked(&mut self) -> Result<(), PrivacyError> {
        self.require_unlocked_at(Instant::now())
    }
    pub fn require_unlocked_at(&mut self, now: Instant) -> Result<(), PrivacyError> {
        if self.settings.lock_enabled
            && now.saturating_duration_since(self.last_activity)
                >= Duration::from_secs(u64::from(self.settings.auto_lock_minutes) * 60)
        {
            self.locked = true;
        }
        if self.locked {
            Err(PrivacyError::Locked)
        } else {
            Ok(())
        }
    }
    pub fn touch_at(&mut self, now: Instant) -> Result<(), PrivacyError> {
        self.require_unlocked_at(now)?;
        self.last_activity = now;
        Ok(())
    }
    pub fn touch(&mut self) -> Result<(), PrivacyError> {
        self.touch_at(Instant::now())
    }
    pub fn lock_now(&mut self) {
        if self.settings.lock_enabled {
            self.locked = true;
        }
    }
    pub fn unlock(&mut self, password: &str) -> Result<(), PrivacyError> {
        if !self.settings.lock_enabled {
            return Ok(());
        }
        let now = Instant::now();
        if self.retry_after.is_some_and(|until| now < until) {
            return Err(PrivacyError::RateLimited);
        }
        let verifier = self
            .settings
            .password_verifier
            .as_deref()
            .ok_or(PrivacyError::Locked)?;
        let hash = PasswordHash::new(verifier).map_err(|_| PrivacyError::Locked)?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_err()
        {
            self.failures = self.failures.saturating_add(1);
            if self.failures >= 5 {
                self.retry_after = Some(now + Duration::from_secs(30));
                self.failures = 0;
            }
            return Err(PrivacyError::WrongPassword);
        }
        self.locked = false;
        self.last_activity = now;
        self.failures = 0;
        self.retry_after = None;
        Ok(())
    }
    pub fn enable_lock(
        &mut self,
        repo: &impl FeatureRepository,
        password: &str,
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        if self.settings.lock_enabled || !(8..=256).contains(&password.len()) {
            return Err(PrivacyError::InvalidInput);
        }
        let mut salt_bytes = [0; 16];
        getrandom::getrandom(&mut salt_bytes).map_err(|_| PrivacyError::Storage)?;
        let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| PrivacyError::Storage)?;
        let verifier = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| PrivacyError::Storage)?
            .to_string();
        let mut settings = self.settings.clone();
        settings.lock_enabled = true;
        settings.password_verifier = Some(verifier);
        repo.save_privacy_settings(settings.clone())?;
        self.settings = settings;
        self.last_activity = Instant::now();
        Ok(())
    }
    pub fn disable_lock(
        &mut self,
        repo: &impl FeatureRepository,
        password: &str,
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        if !self.settings.lock_enabled {
            return Ok(());
        }
        let hash = PasswordHash::new(
            self.settings
                .password_verifier
                .as_deref()
                .ok_or(PrivacyError::Locked)?,
        )
        .map_err(|_| PrivacyError::Locked)?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_err()
        {
            return Err(PrivacyError::WrongPassword);
        }
        let mut settings = self.settings.clone();
        settings.lock_enabled = false;
        settings.password_verifier = None;
        repo.save_privacy_settings(settings.clone())?;
        self.settings = settings;
        self.locked = false;
        Ok(())
    }
    pub fn set_hide_amounts(
        &mut self,
        repo: &impl FeatureRepository,
        hide: bool,
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        let mut settings = self.settings.clone();
        settings.hide_amounts = hide;
        repo.save_privacy_settings(settings.clone())?;
        self.settings = settings;
        Ok(())
    }
    pub fn set_auto_lock_minutes(
        &mut self,
        repo: &impl FeatureRepository,
        minutes: u16,
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        if !(1..=120).contains(&minutes) {
            return Err(PrivacyError::InvalidInput);
        }
        let mut settings = self.settings.clone();
        settings.auto_lock_minutes = minutes;
        repo.save_privacy_settings(settings.clone())?;
        self.settings = settings;
        Ok(())
    }
    pub fn redact_view(&self, amount: &str) -> String {
        if self.settings.hide_amounts {
            "••••".into()
        } else {
            amount.into()
        }
    }
    pub fn preview_source_data(
        &mut self,
        repo: &impl PrivacyRepository,
    ) -> Result<SourceDeletePreview, PrivacyError> {
        self.require_unlocked()?;
        repo.preview_notification_delete().map_err(Into::into)
    }
    pub fn delete_source_data(
        &mut self,
        repo: &impl PrivacyRepository,
        expected: [u8; 32],
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        repo.delete_notification_data(expected).map_err(Into::into)
    }
    pub fn delete_all_data(
        &mut self,
        repo: &impl PrivacyRepository,
        expected: [u8; 32],
    ) -> Result<(), PrivacyError> {
        self.require_unlocked()?;
        repo.delete_all_data(expected)?;
        self.settings = repo.privacy_settings()?;
        self.locked = false;
        Ok(())
    }
}
