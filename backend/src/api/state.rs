use crate::{
    account_restrictions::SqliteAccountRestrictions,
    application::BootstrapService,
    blacklist::SqliteAddressBlacklist,
    external_withdrawals::ExternalWithdrawalService,
    fee_sweep::FeeSweepService,
    inventory::{InventoryService, RebalancingService},
    public_info::PublicInfoService,
    reconciliation::ReconciliationService,
    reporting::ReportingService,
    retail_application::RetailService,
    statements::StatementService,
};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) service: Arc<BootstrapService>,
    pub(super) retail: Arc<RetailService>,
    pub(super) reconciliation: Arc<ReconciliationService>,
    pub(super) reporting: Arc<ReportingService>,
    pub(super) inventory: Arc<InventoryService>,
    pub(super) rebalancing: Arc<RebalancingService>,
    pub(super) public_info: Arc<PublicInfoService>,
    pub(super) statements: Arc<StatementService>,
    pub(super) fee_sweeps: Arc<FeeSweepService>,
    pub(super) blacklist: Arc<SqliteAddressBlacklist>,
    pub(super) account_restrictions: Arc<SqliteAccountRestrictions>,
    pub(super) withdrawals: Arc<ExternalWithdrawalService>,
}

pub struct RouterDependencies {
    pub service: Arc<BootstrapService>,
    pub retail: Arc<RetailService>,
    pub reconciliation: Arc<ReconciliationService>,
    pub reporting: Arc<ReportingService>,
    pub inventory: Arc<InventoryService>,
    pub rebalancing: Arc<RebalancingService>,
    pub public_info: Arc<PublicInfoService>,
    pub statements: Arc<StatementService>,
    pub fee_sweeps: Arc<FeeSweepService>,
    pub blacklist: Arc<SqliteAddressBlacklist>,
    pub account_restrictions: Arc<SqliteAccountRestrictions>,
    pub withdrawals: Arc<ExternalWithdrawalService>,
}

impl From<RouterDependencies> for AppState {
    fn from(value: RouterDependencies) -> Self {
        Self {
            service: value.service,
            retail: value.retail,
            reconciliation: value.reconciliation,
            reporting: value.reporting,
            inventory: value.inventory,
            rebalancing: value.rebalancing,
            public_info: value.public_info,
            statements: value.statements,
            fee_sweeps: value.fee_sweeps,
            blacklist: value.blacklist,
            account_restrictions: value.account_restrictions,
            withdrawals: value.withdrawals,
        }
    }
}
