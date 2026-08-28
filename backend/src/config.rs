use alloy::primitives::Address;
use std::{env, net::SocketAddr};
use thiserror::Error;

#[derive(Clone)]
pub struct Config {
    pub http_address: SocketAddr,
    pub database_path: String,
    pub issuer_url: String,
    pub issuer_public_url: String,
    pub mock_bank_url: String,
    pub rpc_url: String,
    pub token_address: Address,
    pub deposit_router_address: Address,
    pub deposit_confirmations: u64,
    pub corporate_private_key: String,
    pub hot_private_key: String,
    pub corporate_address: Address,
    pub hot_address: Address,
    pub cold_address: Address,
    pub chain_id: u64,
}
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid address in {0}")]
    Address(&'static str),
    #[error("invalid CASP_HTTP_ADDRESS")]
    HttpAddress,
    #[error("invalid CHAIN_ID")]
    ChainId,
}
impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            http_address: env::var("CASP_HTTP_ADDRESS")
                .unwrap_or_else(|_| "127.0.0.1:3200".to_owned())
                .parse()
                .map_err(|_| ConfigError::HttpAddress)?,
            database_path: env::var("CASP_DATABASE_PATH")
                .unwrap_or_else(|_| "data/casp.sqlite".to_owned()),
            issuer_url: env::var("ISSUER_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned()),
            issuer_public_url: env::var("ISSUER_PUBLIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5173".to_owned()),
            mock_bank_url: env::var("MOCK_BANK_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3100".to_owned()),
            rpc_url: env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_owned()),
            token_address: address("TOKEN_ADDRESS")?,
            deposit_router_address: address("CASP_DEPOSIT_ROUTER_ADDRESS")?,
            deposit_confirmations: env::var("CASP_DEPOSIT_CONFIRMATIONS")
                .unwrap_or_else(|_| "2".into())
                .parse()
                .map_err(|_| ConfigError::ChainId)?,
            corporate_private_key: required("CASP_CORPORATE_PRIVATE_KEY")?,
            hot_private_key: required("CASP_HOT_PRIVATE_KEY")?,
            corporate_address: address("CASP_CORPORATE_ADDRESS")?,
            hot_address: address("CASP_HOT_ADDRESS")?,
            cold_address: address("CASP_COLD_ADDRESS")?,
            chain_id: env::var("CHAIN_ID")
                .unwrap_or_else(|_| "31337".to_owned())
                .parse()
                .map_err(|_| ConfigError::ChainId)?,
        })
    }
}
fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name))
}
fn address(name: &'static str) -> Result<Address, ConfigError> {
    required(name)?
        .parse()
        .map_err(|_| ConfigError::Address(name))
}
