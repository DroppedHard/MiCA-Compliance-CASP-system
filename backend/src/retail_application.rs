use crate::retail::{ClientAccount, RetailOrder, ServiceRecord};
use alloy::primitives::Address;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
const TOKEN_UNITS_PER_CENT: u64 = 10_000;
pub trait RetailStore: Send + Sync {
    fn activate_inventory(&self, amount: u64) -> Result<(), RetailError>;
    fn account(&self, client: &str) -> Result<ClientAccount, RetailError>;
    fn accounts(&self) -> Result<Vec<ClientAccount>, RetailError>;
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
    lock: Arc<tokio::sync::Mutex<()>>,
}
impl RetailService {
    pub fn new(
        store: Arc<dyn RetailStore>,
        issuer: Arc<dyn RetailIssuerGateway>,
        hot: Address,
        contract: String,
        chain: u64,
    ) -> Self {
        Self {
            store,
            issuer,
            hot,
            contract,
            chain,
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
    pub fn purchase(
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
        self.store
            .purchase(id, client, amount_minor, &self.contract, self.chain)
    }
    pub fn sale(
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
        self.store
            .sale(id, client, amount_raw, &self.contract, self.chain)
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
            Ok(value) => self
                .store
                .complete_redemption(&order.operation_id, value.transaction_hash.as_deref()),
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
    use crate::infrastructure::SqliteRetailStore;
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

    #[tokio::test]
    async fn completed_redemption_does_not_call_issuer_twice() {
        let store = Arc::new(SqliteRetailStore::open(":memory:").unwrap());
        store.activate_inventory(10_000_000).unwrap();
        store
            .purchase("purchase", "alice", 100, "0x1", 31337)
            .unwrap();
        let issuer = Arc::new(Issuer(AtomicUsize::new(0)));
        let service = RetailService::new(
            store,
            issuer.clone(),
            Address::with_last_byte(2),
            "0x1".into(),
            31337,
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
}
