use super::*;

#[derive(Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub category: String,
}
impl Ledger {
    pub(super) fn account(&self, id: &str) -> Result<DomainAccount> {
        let account = self
            .repo()?
            .get_account(account_id(id)?)
            .map_err(|e| e.to_string())?
            .ok_or("平台不存在")?;
        if account.currency != cny() {
            return Err("当前界面仅支持人民币平台".into());
        }
        Ok(account)
    }
    pub fn accounts(&self) -> Result<Vec<Account>> {
        let repo = self.repo()?;
        repo.list_accounts()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|a| {
                let category = repo
                    .platform_category(a.id)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| {
                        match a.display_name.as_str() {
                            "微信" | "支付宝" => "wallet",
                            "现金" => "cash",
                            _ => "other",
                        }
                        .into()
                    });
                Ok(Account {
                    id: a.id.as_uuid().to_string(),
                    name: a.display_name,
                    category,
                })
            })
            .collect()
    }
    #[cfg(test)]
    pub fn create_account(&self, name: &str) -> Result<Account> {
        self.create_platform(name, "other")
    }
    fn validate_platform(name: &str, category: &str) -> Result<()> {
        if name.is_empty() || name.chars().count() > 40 {
            return Err("平台名称需为 1–40 个字符".into());
        }
        if !["wallet", "bank", "securities", "exchange", "cash", "other"].contains(&category) {
            return Err("请选择有效的平台分类".into());
        }
        Ok(())
    }
    pub fn create_platform(&self, name: &str, category: &str) -> Result<Account> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        let id = AccountId::new();
        self.repo()?
            .save_platform(id, name, category, true)
            .map_err(|e| e.to_string())?;
        Ok(Account {
            id: id.as_uuid().to_string(),
            name: name.into(),
            category: category.into(),
        })
    }
    pub fn update_platform(&self, id: &str, name: &str, category: &str) -> Result<()> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        self.repo()?
            .save_platform(account_id(id)?, name, category, false)
            .map_err(|e| e.to_string())
    }
}
