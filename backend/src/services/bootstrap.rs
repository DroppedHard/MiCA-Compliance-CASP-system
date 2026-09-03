use crate::domain::{BootstrapOperation, BootstrapStatus, WalletBalances};
use alloy::primitives::Address;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

pub const PURCHASE_USD_MINOR: u64 = 1_000_000;
pub const PURCHASE_TOKEN_RAW: u64 = 10_000_000_000;
pub const COLD_TARGET_RAW: u64 = 8_000_000_000;
pub const HOT_TARGET_RAW: u64 = 2_000_000_000;

pub trait BootstrapStore: Send + Sync {
    fn get(&self) -> Result<Option<BootstrapOperation>, BootstrapError>;
    fn create(
        &self,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Result<BootstrapOperation, BootstrapError>;
    fn advance(
        &self,
        status: BootstrapStatus,
        issuer_hash: Option<&str>,
        cold_hash: Option<&str>,
        hot_hash: Option<&str>,
    ) -> Result<BootstrapOperation, BootstrapError>;
    fn fail(&self, message: &str) -> Result<(), BootstrapError>;
}
#[derive(Debug, Clone)]
pub struct IssuerOrder {
    pub transaction_hash: Option<String>,
}
#[async_trait]
pub trait IssuerGateway: Send + Sync {
    async fn create_order(
        &self,
        operation_id: &str,
        recipient: Address,
        amount_minor: u64,
    ) -> Result<(), BootstrapError>;
    async fn settle_order(&self, operation_id: &str) -> Result<IssuerOrder, BootstrapError>;
}
#[async_trait]
pub trait BankGateway: Send + Sync {
    async fn send_usd(&self, operation_id: &str, amount_minor: u64) -> Result<(), BootstrapError>;
}
#[async_trait]
pub trait WalletGateway: Send + Sync {
    async fn ensure_balance(
        &self,
        destination: Address,
        target_raw: u64,
    ) -> Result<Option<String>, BootstrapError>;
    async fn balances(
        &self,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Result<WalletBalances, BootstrapError>;
}

#[derive(Clone)]
pub struct BootstrapService {
    store: Arc<dyn BootstrapStore>,
    issuer: Arc<dyn IssuerGateway>,
    bank: Arc<dyn BankGateway>,
    wallet: Arc<dyn WalletGateway>,
    corporate: Address,
    hot: Address,
    cold: Address,
    lock: Arc<tokio::sync::Mutex<()>>,
}
impl BootstrapService {
    pub fn new(
        store: Arc<dyn BootstrapStore>,
        issuer: Arc<dyn IssuerGateway>,
        bank: Arc<dyn BankGateway>,
        wallet: Arc<dyn WalletGateway>,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Self {
        Self {
            store,
            issuer,
            bank,
            wallet,
            corporate,
            hot,
            cold,
            lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub fn operation(&self) -> Result<Option<BootstrapOperation>, BootstrapError> {
        self.store.get()
    }
    pub async fn balances(&self) -> Result<WalletBalances, BootstrapError> {
        self.wallet
            .balances(self.corporate, self.hot, self.cold)
            .await
    }
    pub async fn execute(&self) -> Result<BootstrapOperation, BootstrapError> {
        let _guard = self.lock.lock().await;
        let mut operation = match self.store.get()? {
            Some(value) => value,
            None => self.store.create(self.corporate, self.hot, self.cold)?,
        };
        if operation.status == BootstrapStatus::Distributed {
            return Ok(operation);
        }
        let result = self.resume(&mut operation).await;
        if let Err(error) = &result {
            let _ = self.store.fail(&error.to_string());
        }
        result
    }
    async fn resume(
        &self,
        operation: &mut BootstrapOperation,
    ) -> Result<BootstrapOperation, BootstrapError> {
        if matches!(
            operation.status,
            BootstrapStatus::Created | BootstrapStatus::Failed
        ) {
            self.issuer
                .create_order(&operation.operation_id, self.hot, PURCHASE_USD_MINOR)
                .await?;
            *operation =
                self.store
                    .advance(BootstrapStatus::IssuerOrderCreated, None, None, None)?;
        }
        if operation.status == BootstrapStatus::IssuerOrderCreated {
            self.bank
                .send_usd(&operation.operation_id, PURCHASE_USD_MINOR)
                .await?;
            *operation = self
                .store
                .advance(BootstrapStatus::FiatSent, None, None, None)?;
        }
        if operation.status == BootstrapStatus::FiatSent {
            let issued = self.issuer.settle_order(&operation.operation_id).await?;
            *operation = self.store.advance(
                BootstrapStatus::TokensIssued,
                issued.transaction_hash.as_deref(),
                None,
                None,
            )?;
        }
        if operation.status == BootstrapStatus::TokensIssued {
            // Issuer mints the custody pool directly to the hot wallet. The only
            // subsequent distribution transaction is hot -> cold; customer assets
            // never pass through the CASP corporate wallet.
            let cold_hash = self
                .wallet
                .ensure_balance(self.cold, COLD_TARGET_RAW)
                .await?;
            *operation = self.store.advance(
                BootstrapStatus::Distributed,
                None,
                cold_hash.as_deref(),
                None,
            )?;
            let balances = self.balances().await?;
            // Corporate funds earned or acquired outside this bootstrap are valid.
            // Reconciliation therefore verifies only the 20/80 allocation targets.
            if balances.hot_raw != HOT_TARGET_RAW.to_string()
                || balances.cold_raw != COLD_TARGET_RAW.to_string()
            {
                return Err(BootstrapError::Reconciliation(
                    "wallet balances do not match the 2000/8000 hot/cold target".to_owned(),
                ));
            }
        }
        Ok(operation.clone())
    }
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("CASP persistence failed: {0}")]
    Storage(String),
    #[error("issuer request failed: {0}")]
    Issuer(String),
    #[error("Emisja rUSD jest obecnie zablokowana przez emitenta.")]
    IssuanceBlocked,
    #[error("bank transfer failed: {0}")]
    Bank(String),
    #[error("wallet operation failed: {0}")]
    Wallet(String),
    #[error("bootstrap reconciliation failed: {0}")]
    Reconciliation(String),
    #[error("bootstrap has not been started")]
    NotStarted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Store(Mutex<Option<BootstrapOperation>>);
    impl BootstrapStore for Store {
        fn get(&self) -> Result<Option<BootstrapOperation>, BootstrapError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn create(
            &self,
            c: Address,
            h: Address,
            d: Address,
        ) -> Result<BootstrapOperation, BootstrapError> {
            let value = BootstrapOperation {
                operation_id: "op-1".into(),
                status: BootstrapStatus::Created,
                amount_usd_minor: PURCHASE_USD_MINOR.to_string(),
                token_amount_raw: PURCHASE_TOKEN_RAW.to_string(),
                corporate_address: c.to_string(),
                hot_address: h.to_string(),
                cold_address: d.to_string(),
                hot_target_raw: HOT_TARGET_RAW.to_string(),
                cold_target_raw: COLD_TARGET_RAW.to_string(),
                issuer_transaction_hash: None,
                cold_transaction_hash: None,
                hot_transaction_hash: None,
                last_error: None,
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
            };
            *self.0.lock().unwrap() = Some(value.clone());
            Ok(value)
        }
        fn advance(
            &self,
            s: BootstrapStatus,
            i: Option<&str>,
            c: Option<&str>,
            h: Option<&str>,
        ) -> Result<BootstrapOperation, BootstrapError> {
            let mut lock = self.0.lock().unwrap();
            let value = lock.as_mut().unwrap();
            value.status = s;
            if i.is_some() {
                value.issuer_transaction_hash = i.map(str::to_owned)
            }
            if c.is_some() {
                value.cold_transaction_hash = c.map(str::to_owned)
            }
            if h.is_some() {
                value.hot_transaction_hash = h.map(str::to_owned)
            }
            Ok(value.clone())
        }
        fn fail(&self, m: &str) -> Result<(), BootstrapError> {
            let mut lock = self.0.lock().unwrap();
            let value = lock.as_mut().unwrap();
            value.status = BootstrapStatus::Failed;
            value.last_error = Some(m.to_owned());
            Ok(())
        }
    }
    struct Issuer(Mutex<(u8, Option<Address>)>);
    #[async_trait]
    impl IssuerGateway for Issuer {
        async fn create_order(
            &self,
            _: &str,
            recipient: Address,
            _: u64,
        ) -> Result<(), BootstrapError> {
            let mut calls = self.0.lock().unwrap();
            calls.0 += 1;
            calls.1 = Some(recipient);
            Ok(())
        }
        async fn settle_order(&self, _: &str) -> Result<IssuerOrder, BootstrapError> {
            self.0.lock().unwrap().0 += 1;
            Ok(IssuerOrder {
                transaction_hash: Some("0xissuer".into()),
            })
        }
    }
    struct Bank(Mutex<u8>);
    #[async_trait]
    impl BankGateway for Bank {
        async fn send_usd(&self, _: &str, _: u64) -> Result<(), BootstrapError> {
            *self.0.lock().unwrap() += 1;
            Ok(())
        }
    }
    struct Wallet(Mutex<(u64, u64, u64)>);
    #[async_trait]
    impl WalletGateway for Wallet {
        async fn ensure_balance(
            &self,
            d: Address,
            t: u64,
        ) -> Result<Option<String>, BootstrapError> {
            let mut b = self.0.lock().unwrap();
            if d == Address::with_last_byte(3) {
                let moved = t - b.2;
                b.2 = t;
                b.1 -= moved;
            } else {
                b.1 = t
            }
            Ok(Some("0xtransfer".into()))
        }
        async fn balances(
            &self,
            _: Address,
            _: Address,
            _: Address,
        ) -> Result<WalletBalances, BootstrapError> {
            let b = self.0.lock().unwrap();
            Ok(WalletBalances {
                corporate_raw: b.0.to_string(),
                hot_raw: b.1.to_string(),
                cold_raw: b.2.to_string(),
                evidence_block: Some(1),
            })
        }
    }
    #[tokio::test]
    async fn executes_complete_purchase_and_distribution_only_once() {
        let store = Arc::new(Store(Mutex::new(None)));
        let issuer = Arc::new(Issuer(Mutex::new((0, None))));
        let bank = Arc::new(Bank(Mutex::new(0)));
        let wallet = Arc::new(Wallet(Mutex::new((0, PURCHASE_TOKEN_RAW, 0))));
        let service = BootstrapService::new(
            store,
            issuer.clone(),
            bank.clone(),
            wallet,
            Address::with_last_byte(1),
            Address::with_last_byte(2),
            Address::with_last_byte(3),
        );
        assert_eq!(
            service.execute().await.unwrap().status,
            BootstrapStatus::Distributed
        );
        assert_eq!(
            service.execute().await.unwrap().status,
            BootstrapStatus::Distributed
        );
        let issuer_calls = issuer.0.lock().unwrap();
        assert_eq!(issuer_calls.0, 2);
        assert_eq!(issuer_calls.1, Some(Address::with_last_byte(2)));
        assert_eq!(*bank.0.lock().unwrap(), 1);
    }
}
