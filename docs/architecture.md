# 架构边界

- `ledger-domain` 定义金额、账户、交易及不变量，不依赖平台。
- `ledger-app` 编排账户、交易、分类和报表用例，通过 ports 访问持久层。
- `ledger-storage` 将账本保存在本机 SQLite。
- `ledger-import` 处理微信/支付宝账单预览、标准化和去重。
- `ledger-notifications` 解析 Android 适配层送来的通知事件。
- `ledger-backup` 管理本地加密备份和恢复。
- `apps/slint-ui` 提供 Rust/Slint 界面；`apps/android` 只承接 Android 系统 API 和 JNI 桥接。

每个目录使用自己的 `tree` 说明直接文件。运行 `python3 scripts/update_tree.py` 更新，运行 `python3 scripts/update_tree.py --check` 检查。

## 加密备份与原子恢复

备份格式 `MEOWMBAK` 容器版本 1：16 字节系统随机盐、12 字节系统随机 nonce，Argon2id v1.3（64 MiB、3 次、1 lane）派生 256 位密钥，AES-256-GCM 认证加密；整个头部作为 AAD。固定版本参数避免文件指定高成本 KDF。密码至少 8 个字符；应用不保存密码，丢失后无法恢复。序列化明文和派生密钥使用 zeroize 管理；UI 清空密码输入，但不承诺操作系统或 UI 字符串副本被物理擦除。

白名单 JSON archive schema 4 包含账户、分类、交易、分录、导入批次、来源摘要、分类规则、设置及 pending_details。绝不复制 SQLite 原始页、任意新表或通知正文/Token。schema 3 可补默认设置升级，其他版本拒绝。最多接受 64 MiB 备份和 32 MiB CSV。

恢复在临时内存 SQLite 中按正式 schema 重建，校验外键、唯一性、字段类型、分录/退款/domain 约束，再在当前连接 `BEGIN IMMEDIATE` 事务中原子替换全部账本内容。使用原数据库 inode，避免 Slint 长期连接和 JNI 按次连接写入已被 rename 的旧数据库。并发写入由 SQLite 排队，超时或任何写入失败全部回滚。此处是原子逻辑替换，不是文件 rename；断电恢复依赖 SQLite 默认 rollback journal。恢复保留本机通知及 API 授权开关，其余备份设置恢复，不能由另一设备的备份重新启用采集。

Android 用 SAF `ACTION_CREATE_DOCUMENT` 新建加密导出文件、`ACTION_OPEN_DOCUMENT` 选择备份/CSV；密码不跨 JNI。导出失败会尽力删除新建未完成文档，第三方文档 provider 不提供统一的原子写保证。取消、选错文件、密码/认证错误均不修改账本。CSV 仅暂存在 app cache，读取后立即清理；备份输入是密文，成功恢复和下次启动时清理。桌面导出在目标目录临时写入并 rename，禁止覆盖当前数据库。

## Optional provider boundary (offline contract only)

`ledger-app::ports` owns `ProviderId`, redacted/zeroizing `SecretToken`, and
`CredentialStore`. `ledger-import::providers` has independent WeChat, Alipay and
generic bank adapter modules with authorization, paginated time-range reads and
revocation operations. Their JSON schemas are **synthetic test contracts**, not
published provider protocols. There are no live URLs, HTTP client, OAuth flow or
background sync. `UnavailableTransport` fails closed; settings clearly say each
provider is not connected. Real platform access must be assessed separately before
adding any network transport or presenting authorization/sync controls.

Both the global API switch and the specific provider opt-in must be enabled.
Neither enables notification capture or file imports. Missing tokens do not affect
local functions. Provider fetches check current consent before each page, use a
half-open UTC range, bound pages/rows/bytes, reject repeated cursors and discard the
whole transient result on failure. A retry starts the same range; no success cursor
is persisted before user confirmation. Network/server errors and 429 retry hints
remain recoverable. 401/403 erase the local credential. Explicit revocation erases
local credentials even if the remote endpoint fails and reports `LocalOnly` so a
future UI can instruct the user to revoke at the platform too.

All adapters normalize decimal strings through the existing integer-money parser,
create shared Pending `ImportCandidate`s, use the same category rule evaluator and
`ImportPreview` duplicate matching, and require the existing explicit atomic
`MobileLedger` confirmation. Fetch/preview does not insert transactions or affect
balances. CSV and API identities use the same WeChat/Alipay/BankCsv source namespace
so repeated IDs across those channels cannot double-book. Bank is only a generic
fixture boundary: a real multi-bank integration must evaluate institution/account
identity namespaces before rollout. Unmapped refunds and foreign amounts lacking a
CNY report value cannot be confirmed until reviewed through the existing workflow.

Per-provider local opt-ins live in a singleton `provider_settings` auxiliary table,
created idempotently after schema 3/4 migration; this device-local table has no
ledger foreign keys and is deliberately outside portable archive schema 4. Settings
save updates global and per-provider values in one SQLite transaction. Backup
restoration preserves this entire table and the current global API switch, just as
it preserves current notification consent; it cannot revive another device's grants.
Android notification preferences remain authoritative in Kotlin, with no new API
preference copy added there.

`ProviderCredentialStore` in Kotlin uses non-exportable Android Keystore AES-256-GCM
keys, one per provider, random 96-bit nonces and authenticated provider/version AAD.
Only bounded encrypted envelopes are written using `AtomicFile` beneath
`Context.noBackupFilesDir/provider-credentials`. Tokens never enter SQLite, ordinary
preferences, logs, or portable backup. The small Rust JNI `AndroidCredentialStore`
implements the application credential port; transient Rust byte buffers zeroize on
drop and intermediate Java token arrays are cleared. This does not claim to erase
all JVM/OS/cipher-internal copies. No plaintext desktop fallback is provided.

Data-erasure implementations must reset global API consent and all three
`provider_settings` flags, delete every credential slot through
`ProviderCredentialStore.delete`, and erase aliases `meowm.provider.wechat`,
`meowm.provider.alipay`, and `meowm.provider.bank`; deleting SQLite alone is not
credential revocation. No API preference implies a live platform grant today.

## Release data lifecycle and explicit review

`DeletionScope`/`DeletionRepository` implement single transaction, import batch and all-local deletion through `MobileLedger::delete_data`. Application confirmation precedes writes; SQLite commits the entire operation atomically. Refund references block removing retained parents. Batch deletion removes only its evidence and transactions lacking other evidence; merged records retain other-source provenance. All-local clears ledger tables and resets both mobile_settings and provider_settings. Android stops capture before deletion, then erases Keystore aliases/ciphertext and cache staging; those stores cannot share the SQLite transaction. Retryable cleanup failure is explicit. See privacy-and-deletion.md.

ImportReview requires an explicit decision for strong matches, including posting-free notification Pending records with matching type, CNY amount, merchant and five-minute window. Users can exclude, declare distinct, or link to an existing transaction. Confirmed target postings must match; Pending linking uses the reviewed imported postings and keeps the existing transaction ID, then confirms atomically with the new evidence. Replays use both the original notification identity and file external ID. Candidate scoring never automatically confirms or merges fuzzy matches.

Android `LockActivity` uses device credential verification. Native ledger and settings views hide on pause and while locked, FLAG_SECURE hides recents/screenshots, and cancellation does not unlock. App-lock preferences are device-local and excluded from portable backup. Android Auto Backup and device transfer are disabled and all backup domains excluded. No Internet permission or telemetry is installed. Optional provider contracts remain offline only.

Settings rule management adds/updates/disables deterministic merchant/keyword rules with priority and target category. Rules suggest categories for file previews and Pending editors and do not silently recategorize confirmed entries.
