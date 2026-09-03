use crate::external_deposits::{
    ExternalDepositError, ExternalDepositEvent, ExternalDepositGateway,
};
use alloy::{
    primitives::{Address, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::SolEvent,
};
use async_trait::async_trait;

sol! { event DepositReceived(address indexed sender, bytes32 indexed clientReference, uint256 amount); }

pub struct AlloyExternalDepositGateway {
    provider: DynProvider,
    router: Address,
    confirmations: u64,
}
impl AlloyExternalDepositGateway {
    pub async fn connect(
        rpc: &str,
        router: Address,
        confirmations: u64,
    ) -> Result<Self, ExternalDepositError> {
        let provider = ProviderBuilder::new()
            .connect(rpc)
            .await
            .map_err(rpc_error)?
            .erased();
        Ok(Self {
            provider,
            router,
            confirmations,
        })
    }
}

#[async_trait]
impl ExternalDepositGateway for AlloyExternalDepositGateway {
    async fn confirmed_block(&self) -> Result<u64, ExternalDepositError> {
        Ok(self
            .provider
            .get_block_number()
            .await
            .map_err(rpc_error)?
            .saturating_sub(self.confirmations))
    }
    async fn events(
        &self,
        from: u64,
        to: u64,
    ) -> Result<Vec<ExternalDepositEvent>, ExternalDepositError> {
        if from > to {
            return Ok(Vec::new());
        }
        let filter = Filter::new()
            .address(self.router)
            .event_signature(DepositReceived::SIGNATURE_HASH)
            .from_block(from)
            .to_block(to);
        self.provider
            .get_logs(&filter)
            .await
            .map_err(rpc_error)?
            .into_iter()
            .filter(|log| !log.removed)
            .map(|log| {
                let decoded = log
                    .log_decode_validate::<DepositReceived>()
                    .map_err(rpc_error)?;
                let amount: U256 = decoded.inner.data.amount;
                Ok(ExternalDepositEvent {
                    transaction_hash: log
                        .transaction_hash
                        .ok_or_else(|| {
                            ExternalDepositError::Rpc("deposit log has no transaction hash".into())
                        })?
                        .to_string(),
                    log_index: log.log_index.ok_or_else(|| {
                        ExternalDepositError::Rpc("deposit log has no index".into())
                    })?,
                    block_number: log.block_number.ok_or_else(|| {
                        ExternalDepositError::Rpc("deposit log has no block".into())
                    })?,
                    sender: decoded.inner.data.sender,
                    client_reference: decoded.inner.data.clientReference,
                    amount_raw: amount
                        .try_into()
                        .map_err(|_| ExternalDepositError::AmountOverflow)?,
                })
            })
            .collect()
    }
}
fn rpc_error(error: impl std::fmt::Display) -> ExternalDepositError {
    ExternalDepositError::Rpc(error.to_string())
}
