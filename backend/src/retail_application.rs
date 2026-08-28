use crate::reconciliation::ReconciliationService;
use crate::retail::{
    ClientAccount, FeePosition, InternalTransfer, RetailOrder, ServiceRecord,
    ServiceRecordAmendment,
};
use alloy::primitives::Address;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
const TOKEN_UNITS_PER_CENT: u64 = 10_000;
pub trait RetailStore: Send + Sync {
    fn activate_inventory(&self, amount: u64) -> Result<(), RetailError>;
    fn add_inventory_once(
        &self,
        operation: &str,
        wallet: &str,
        amount: u64,
    ) -> Result<(), RetailError>;
    fn account(&self, client: &str) -> Result<ClientAccount, RetailError>;
    fn accounts(&self) -> Result<Vec<ClientAccount>, RetailError>;
    fn client_id_by_wallet(&self, wallet_address: &str) -> Result<Option<String>, RetailError>;
    fn purchase(
        &self,
        id: &str,
        client: &str,
        amount_minor: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError>;
    fn sale(
        &self,
        id: &str,
        client: &str,
        amount_raw: u64,
        contract: &str,
        chain: u64,
    ) -> Result<RetailOrder, RetailError>;
    fn begin_redemption(
        &self,
        id: &str,
        client: &str,
        amount_raw: u64,
        contract: &str,
        chain: u64,
        hot: &str,
    ) -> Result<RetailOrder, RetailError>;
    fn complete_redemption(&self, id: &str, tx: Option<&str>) -> Result<RetailOrder, RetailError>;
    fn fail_redemption(&self, id: &str, message: &str) -> Result<(), RetailError>;
    fn records(&self, client: &str) -> Result<Vec<ServiceRecord>, RetailError>;
    fn all_records(&self) -> Result<Vec<ServiceRecord>, RetailError>;
    fn amend_record(
        &self,
        original: &str,
        amendment_type: &str,
        reason: &str,
    ) -> Result<ServiceRecordAmendment, RetailError>;
    fn amendments(&self) -> Result<Vec<ServiceRecordAmendment>, RetailError>;
    fn transfer(&self, command: TransferPosting<'_>) -> Result<InternalTransfer, RetailError>;
    fn fee_position(&self) -> Result<FeePosition, RetailError>;
}
pub struct TransferPosting<'a> {
    pub id: &'a str,
    pub sender: &'a str,
    pub recipient: &'a str,
    pub gross_raw: u64,
    pub purpose: &'a str,
    pub contract: &'a str,
    pub chain: u64,
}
#[derive(Debug, Clone)]
pub struct IssuerRedemption {
    pub transaction_hash: Option<String>,
}
#[async_trait]
pub trait RetailIssuerGateway: Send + Sync {
    async fn create_redemption(
        &self,
        id: &str,
        holder: Address,
        amount_raw: u64,
    ) -> Result<(), RetailError>;
    async fn settle_redemption(&self, id: &str) -> Result<IssuerRedemption, RetailError>;
}
#[derive(Clone)]
pub struct RetailService {
    store: Arc<dyn RetailStore>,
    issuer: Arc<dyn RetailIssuerGateway>,
    hot: Address,
    contract: String,
    chain: u64,
    reconciliation: Arc<ReconciliationService>,
    lock: Arc<tokio::sync::Mutex<()>>,
}
impl RetailService {
    pub fn new(
        store: Arc<dyn RetailStore>,
        issuer: Arc<dyn RetailIssuerGateway>,
        hot: Address,
        contract: String,
        chain: u64,
        reconciliation: Arc<ReconciliationService>,
    ) -> Self {
        Self {
            store,
            issuer,
            hot,
            contract,
            chain,
            reconciliation,
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub fn activate_bootstrap_inventory(&self, amount: u64) -> Result<(), RetailError> {
        self.store.activate_inventory(amount)
    }
    pub fn accounts(&self) -> Result<Vec<ClientAccount>, RetailError> {
        self.store.accounts()
    }
    pub fn account(&self, client: &str) -> Result<ClientAccount, RetailError> {
        validate_client(client)?;
        self.store.account(client)
    }
    pub fn records(&self, client: &str) -> Result<Vec<ServiceRecord>, RetailError> {
        validate_client(client)?;
        self.store.records(client)
    }
    pub fn all_records(&self) -> Result<Vec<ServiceRecord>, RetailError> {
        self.store.all_records()
    }
    pub fn amendments(&self) -> Result<Vec<ServiceRecordAmendment>, RetailError> {
        self.store.amendments()
    }
    pub fn amend_record(
        &self,
        original: &str,
        amendment_type: &str,
        reason: &str,
    ) -> Result<ServiceRecordAmendment, RetailError> {
        validate_id(original)?;
        if !matches!(amendment_type, "correction" | "reversal") || reason.trim().is_empty() {
            return Err(RetailError::Invalid(
                "amendmentType must be correction or reversal and reason is required".into(),
            ));
        }
        self.store.amend_record(original, amendment_type, reason)
    }
    pub fn fee_position(&self) -> Result<FeePosition, RetailError> {
        self.store.fee_position()
    }
    pub async fn transfer(
        &self,
        sender: &str,
        recipient: &str,
        id: &str,
        gross_raw: u64,
        purpose: &str,
    ) -> Result<InternalTransfer, RetailError> {
        validate_client(sender)?;
        let recipient_id = match self.store.client_id_by_wallet(recipient)? {
            Some(client_id) => client_id,
            None => {
                validate_client(recipient)?;
                recipient.to_owned()
            }
        };
        validate_id(id)?;
        if sender == recipient_id {
            return Err(RetailError::Invalid(
                "sender and recipient must be different clients".into(),
            ));
        }
        if gross_raw == 0 || !gross_raw.is_multiple_of(TOKEN_UNITS_PER_CENT) {
            return Err(RetailError::Invalid(
                "tokenAmountRaw must be positive and represent whole USD cents".into(),
            ));
        }
        if purpose != "private_transfer" && purpose != "goods_or_services" {
            return Err(RetailError::Invalid(
                "purposeClassification must be private_transfer or goods_or_services".into(),
            ));
        }
        let _guard = self.lock.lock().await;
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        let transfer = self.store.transfer(TransferPosting {
            id,
            sender,
            recipient: &recipient_id,
            gross_raw,
            purpose,
            contract: &self.contract,
            chain: self.chain,
        })?;
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        Ok(transfer)
    }
    pub async fn purchase(
        &self,
        client: &str,
        id: &str,
        amount_minor: u64,
    ) -> Result<RetailOrder, RetailError> {
        validate_client(client)?;
        validate_id(id)?;
        if amount_minor == 0 {
            return Err(RetailError::Invalid(
                "amountUsdMinor must be positive".into(),
            ));
        }
        let _guard = self.lock.lock().await;
        self.reconciliation
            .require_customer_purchase()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        let order = self
            .store
            .purchase(id, client, amount_minor, &self.contract, self.chain)?;
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        Ok(order)
    }
    pub async fn sale(
        &self,
        client: &str,
        id: &str,
        amount_raw: u64,
    ) -> Result<RetailOrder, RetailError> {
        validate_client(client)?;
        validate_id(id)?;
        if amount_raw == 0 || !amount_raw.is_multiple_of(TOKEN_UNITS_PER_CENT) {
            return Err(RetailError::Invalid(
                "tokenAmountRaw must be positive and represent whole USD cents".into(),
            ));
        }
        let _guard = self.lock.lock().await;
        // A sale only moves an entitlement back to unallocated inventory. It
        // does not create a new customer liability, so it remains available
        // during a mismatch while still producing fresh before/after evidence.
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        let order = self
            .store
            .sale(id, client, amount_raw, &self.contract, self.chain)?;
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        Ok(order)
    }
    pub async fn redeem(
        &self,
        client: &str,
        id: &str,
        amount_raw: u64,
    ) -> Result<RetailOrder, RetailError> {
        validate_client(client)?;
        validate_id(id)?;
        if amount_raw == 0 || !amount_raw.is_multiple_of(TOKEN_UNITS_PER_CENT) {
            return Err(RetailError::Invalid(
                "tokenAmountRaw must be positive and represent whole USD cents".into(),
            ));
        }
        let _guard = self.lock.lock().await;
        // Redemption reduces the customer's entitlement together with the
        // on-chain custody pool, so a mismatch does not remove the exit path.
        self.reconciliation
            .check()
            .await
            .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
        let order = self.store.begin_redemption(
            id,
            client,
            amount_raw,
            &self.contract,
            self.chain,
            &self.hot.to_checksum(None),
        )?;
        if order.status == "completed" {
            return Ok(order);
        }
        let issuer_id = order
            .issuer_operation_id
            .as_deref()
            .ok_or_else(|| RetailError::Storage("issuer operation ID missing".into()))?;
        let result = async {
            self.issuer
                .create_redemption(issuer_id, self.hot, amount_raw)
                .await?;
            self.issuer.settle_redemption(issuer_id).await
        }
        .await;
        match result {
            Ok(value) => {
                let completed = self
                    .store
                    .complete_redemption(&order.operation_id, value.transaction_hash.as_deref())?;
                self.reconciliation
                    .check()
                    .await
                    .map_err(|error| RetailError::Reconciliation(error.to_string()))?;
                Ok(completed)
            }
            Err(e) => {
                let _ = self
                    .store
                    .fail_redemption(&order.operation_id, &e.to_string());
                Err(e)
            }
        }
    }
}
#[derive(Debug, Error)]
pub enum RetailError {
    #[error("invalid retail request: {0}")]
    Invalid(String),
    #[error("operation ID was already used for different parameters")]
    IdempotencyConflict,
    #[error("insufficient CASP inventory")]
    InsufficientInventory,
    #[error("insufficient client balance")]
    InsufficientBalance,
    #[error("retail persistence failed: {0}")]
    Storage(String),
    #[error("issuer redemption failed: {0}")]
    Issuer(String),
    #[error("CASP custody reconciliation failed: {0}")]
    Reconciliation(String),
}
fn validate_id(id: &str) -> Result<(), RetailError> {
    if id.trim().is_empty() || id.len() > 128 {
        Err(RetailError::Invalid(
            "operationId must contain 1-128 characters".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_client(client: &str) -> Result<(), RetailError> {
    if matches!(client, "alice" | "bob" | "carol") {
        Ok(())
    } else {
        Err(RetailError::Invalid("unknown demo client".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{BootstrapError, WalletGateway},
        domain::WalletBalances,
        infrastructure::{SqliteReconciliationStore, SqliteRetailStore},
        reconciliation::ReconciliationService,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Issuer(AtomicUsize);
    #[async_trait]
    impl RetailIssuerGateway for Issuer {
        async fn create_redemption(&self, _: &str, _: Address, _: u64) -> Result<(), RetailError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn settle_redemption(&self, _: &str) -> Result<IssuerRedemption, RetailError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(IssuerRedemption {
                transaction_hash: Some("0xburn".into()),
            })
        }
    }
    struct Wallet {
        hot: u64,
        cold: u64,
    }
    #[async_trait]
    impl WalletGateway for Wallet {
        async fn ensure_balance(
            &self,
            _: Address,
            _: u64,
        ) -> Result<Option<String>, BootstrapError> {
            unreachable!()
        }
        async fn balances(
            &self,
            _: Address,
            _: Address,
            _: Address,
        ) -> Result<WalletBalances, BootstrapError> {
            Ok(WalletBalances {
                corporate_raw: "0".into(),
                hot_raw: self.hot.to_string(),
                cold_raw: self.cold.to_string(),
                evidence_block: Some(1),
            })
        }
    }

    #[tokio::test]
    async fn completed_redemption_does_not_call_issuer_twice() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        store
            .purchase("purchase", "alice", 100, "0x1", 31337)
            .unwrap();
        let issuer = Arc::new(Issuer(AtomicUsize::new(0)));
        let service = RetailService::new(
            store.clone(),
            issuer.clone(),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
            Arc::new(ReconciliationService::new(
                Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
                store,
                Arc::new(Wallet {
                    hot: 2_000_000,
                    cold: 8_000_000,
                }),
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            )),
        );
        service
            .redeem("alice", "redemption", 500_000)
            .await
            .unwrap();
        service
            .redeem("alice", "redemption", 500_000)
            .await
            .unwrap();
        assert_eq!(issuer.0.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn purchase_is_rejected_before_ledger_change_when_custody_is_short() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        let service = RetailService::new(
            store.clone(),
            Arc::new(Issuer(AtomicUsize::new(0))),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
            Arc::new(ReconciliationService::new(
                Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
                store.clone(),
                Arc::new(Wallet {
                    hot: 2_000_000,
                    cold: 7_999_999,
                }),
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            )),
        );

        assert!(matches!(
            service.purchase("alice", "purchase", 100).await,
            Err(RetailError::Reconciliation(_))
        ));
        assert_eq!(store.account("alice").unwrap().available_raw, "0");
    }

    #[tokio::test]
    async fn concurrent_purchases_are_serialized_and_remain_reconciled() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        let reconciliation = Arc::new(ReconciliationService::new(
            Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
            store.clone(),
            Arc::new(Wallet {
                hot: 2_000_000,
                cold: 8_000_000,
            }),
            Address::with_last_byte(1),
            Address::with_last_byte(2),
            Address::with_last_byte(3),
        ));
        let service = Arc::new(RetailService::new(
            store.clone(),
            Arc::new(Issuer(AtomicUsize::new(0))),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
            reconciliation.clone(),
        ));
        let first = service.clone();
        let second = service.clone();
        let (alice, bob) = tokio::join!(
            async move { first.purchase("alice", "purchase-a", 100).await },
            async move { second.purchase("bob", "purchase-b", 100).await }
        );
        alice.unwrap();
        bob.unwrap();

        assert_eq!(store.account("alice").unwrap().available_raw, "1000000");
        assert_eq!(store.account("bob").unwrap().available_raw, "1000000");
        assert_eq!(
            reconciliation.current().unwrap().status,
            crate::reconciliation::ReconciliationStatus::Balanced
        );
    }

    #[tokio::test]
    async fn rejects_self_transfer_before_any_ledger_change() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        let service = RetailService::new(
            store.clone(),
            Arc::new(Issuer(AtomicUsize::new(0))),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
            Arc::new(ReconciliationService::new(
                Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
                store.clone(),
                Arc::new(Wallet {
                    hot: 2_000_000,
                    cold: 8_000_000,
                }),
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            )),
        );
        assert!(matches!(
            service
                .transfer("alice", "alice", "self", 100_000, "private_transfer")
                .await,
            Err(RetailError::Invalid(_))
        ));
        assert_eq!(store.fee_position().unwrap().pending_raw, "0");
    }

    #[tokio::test]
    async fn concurrent_transfers_cannot_spend_the_same_balance_twice() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        store
            .purchase("purchase", "alice", 150, "0x1", 31337)
            .unwrap();
        let service = Arc::new(RetailService::new(
            store.clone(),
            Arc::new(Issuer(AtomicUsize::new(0))),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
            Arc::new(ReconciliationService::new(
                Arc::new(SqliteReconciliationStore::open(":memory:").unwrap()),
                store.clone(),
                Arc::new(Wallet {
                    hot: 2_000_000,
                    cold: 8_000_000,
                }),
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            )),
        ));
        let first = service.clone();
        let second = service.clone();
        let (bob, carol) = tokio::join!(
            async move {
                first
                    .transfer("alice", "bob", "to-bob", 1_000_000, "private_transfer")
                    .await
            },
            async move {
                second
                    .transfer("alice", "carol", "to-carol", 1_000_000, "private_transfer")
                    .await
            }
        );
        assert_eq!(usize::from(bob.is_ok()) + usize::from(carol.is_ok()), 1);
        assert_eq!(store.account("alice").unwrap().available_raw, "500000");
        assert_eq!(store.fee_position().unwrap().pending_raw, "1000");
    }
}
