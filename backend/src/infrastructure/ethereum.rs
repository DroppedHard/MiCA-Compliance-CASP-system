use crate::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
    fee_sweep::{FeeSweepError, FeeSweepGateway},
};
use alloy::{
    primitives::{Address, U256},
    providers::{DynProvider, Provider, ProviderBuilder, WalletProvider},
    signers::local::PrivateKeySigner,
    sol,
};
use async_trait::async_trait;
sol! {#[sol(rpc)]interface Token{function balanceOf(address account)external view returns(uint256);function transfer(address to,uint256 amount)external returns(bool);}}
pub struct AlloyWalletGateway {
    provider: DynProvider,
    token: Address,
    signer_address: Address,
}
impl AlloyWalletGateway {
    pub async fn connect(
        rpc: &str,
        token: Address,
        key: &str,
        expected: Address,
    ) -> Result<Self, BootstrapError> {
        Self::connect_for_role(rpc, token, key, expected, "corporate").await
    }

    pub async fn connect_for_role(
        rpc: &str,
        token: Address,
        key: &str,
        expected: Address,
        role: &str,
    ) -> Result<Self, BootstrapError> {
        let signer: PrivateKeySigner = key.parse().map_err(wallet)?;
        let provider = ProviderBuilder::new()
            .wallet(signer)
            .connect(rpc)
            .await
            .map_err(wallet)?;
        if provider.default_signer_address() != expected {
            return Err(BootstrapError::Wallet(format!(
                "CASP {role} private key does not match configured address"
            )));
        }
        Ok(Self {
            provider: provider.erased(),
            token,
            signer_address: expected,
        })
    }
    async fn balance(&self, address: Address) -> Result<U256, BootstrapError> {
        Token::new(self.token, &self.provider)
            .balanceOf(address)
            .call()
            .await
            .map_err(wallet)
    }
}

#[async_trait]
impl FeeSweepGateway for AlloyWalletGateway {
    async fn transfer_to_corporate(
        &self,
        corporate: Address,
        amount_raw: u64,
    ) -> Result<String, FeeSweepError> {
        let amount = U256::from(amount_raw);
        let current = self
            .balance(self.signer_address)
            .await
            .map_err(|error| FeeSweepError::Wallet(error.to_string()))?;
        if current < amount {
            return Err(FeeSweepError::InsufficientHotBalance);
        }
        let pending = Token::new(self.token, &self.provider)
            .transfer(corporate, amount)
            .send()
            .await
            .map_err(|error| FeeSweepError::Wallet(error.to_string()))?;
        let receipt = pending
            .get_receipt()
            .await
            .map_err(|error| FeeSweepError::Wallet(error.to_string()))?;
        Ok(receipt.transaction_hash.to_string())
    }
}
#[async_trait]
impl WalletGateway for AlloyWalletGateway {
    async fn ensure_balance(
        &self,
        destination: Address,
        target_raw: u64,
    ) -> Result<Option<String>, BootstrapError> {
        let current = self.balance(destination).await?;
        let target = U256::from(target_raw);
        if current == target {
            return Ok(None);
        }
        if current > target {
            return Err(BootstrapError::Reconciliation(format!(
                "wallet {destination} already exceeds its bootstrap target"
            )));
        }
        let amount = target - current;
        let pending = Token::new(self.token, &self.provider)
            .transfer(destination, amount)
            .send()
            .await
            .map_err(wallet)?;
        let receipt = pending.get_receipt().await.map_err(wallet)?;
        Ok(Some(receipt.transaction_hash.to_string()))
    }
    async fn balances(
        &self,
        corporate: Address,
        hot: Address,
        cold: Address,
    ) -> Result<WalletBalances, BootstrapError> {
        Ok(WalletBalances {
            corporate_raw: self.balance(corporate).await?.to_string(),
            hot_raw: self.balance(hot).await?.to_string(),
            cold_raw: self.balance(cold).await?.to_string(),
            evidence_block: Some(self.provider.get_block_number().await.map_err(wallet)?),
        })
    }
}
fn wallet(e: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Wallet(e.to_string())
}
