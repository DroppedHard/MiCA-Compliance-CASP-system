use crate::{
    application::{BootstrapError, WalletGateway},
    domain::WalletBalances,
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
}
impl AlloyWalletGateway {
    pub async fn connect(
        rpc: &str,
        token: Address,
        key: &str,
        expected: Address,
    ) -> Result<Self, BootstrapError> {
        let signer: PrivateKeySigner = key.parse().map_err(wallet)?;
        let provider = ProviderBuilder::new()
            .wallet(signer)
            .connect(rpc)
            .await
            .map_err(wallet)?;
        if provider.default_signer_address() != expected {
            return Err(BootstrapError::Wallet(
                "CASP_CORPORATE_PRIVATE_KEY does not match CASP_CORPORATE_ADDRESS".to_owned(),
            ));
        }
        Ok(Self {
            provider: provider.erased(),
            token,
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
