use super::*;

#[derive(Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub category: String,
    pub currency_code: String,
    pub currency_exponent: u8,
}
impl Ledger {
    pub(crate) fn account(&self, id: &str) -> Result<DomainAccount> {
        let account = self
            .repo()?
            .get_account(account_id(id)?)
            .map_err(|e| e.to_string())?
            .ok_or("平台不存在")?;
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
                    currency_code: a.currency.code().into(),
                    currency_exponent: a.currency.minor_unit_exponent(),
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
        self.create_platform_currency(name, category, None)
    }
    pub fn create_platform_currency(
        &self,
        name: &str,
        category: &str,
        currency_code: Option<&str>,
    ) -> Result<Account> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        let code = currency_code.unwrap_or("CNY");
        let definition = ledger_domain::supported_currencies()
            .iter()
            .find(|c| c.code == code)
            .ok_or("不支持的币种")?;
        let currency =
            CurrencyCode::new(definition.code, definition.exponent).map_err(|e| e.to_string())?;
        let id = AccountId::new();
        self.repo()?
            .save_platform(id, name, category, true, &currency)
            .map_err(|e| e.to_string())?;
        Ok(Account {
            id: id.as_uuid().to_string(),
            name: name.into(),
            category: category.into(),
            currency_code: code.into(),
            currency_exponent: definition.exponent,
        })
    }
    pub fn update_platform(&self, id: &str, name: &str, category: &str) -> Result<()> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        self.repo()?
            .save_platform(account_id(id)?, name, category, false, &cny())
            .map_err(|e| e.to_string())
    }
    pub fn delete_platform(&self, id: &str) -> Result<()> {
        self.repo()?
            .delete_platform(account_id(id)?)
            .map_err(|error| match error {
                ledger_app::RepositoryError::NotFound => "平台不存在或已删除".into(),
                ledger_app::RepositoryError::InvalidData(message) => message,
                other => other.to_string(),
            })?;
        self.clear_import_preview();
        Ok(())
    }
}
