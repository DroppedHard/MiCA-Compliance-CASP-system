use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_order(
    tx: &Transaction,
    id: &str,
    client: &str,
    kind: &str,
    raw: u64,
    cents: u64,
    status: &str,
    issuer: Option<&str>,
) -> Result<(), RetailError> {
    let timestamp = now();
    tx.execute(
        "INSERT INTO retail_orders VALUES(?1,?2,?3,?4,'USD',?5,?6,?7,NULL,NULL,?8,?8)",
        params![
            id,
            client,
            kind,
            as_i64(raw)?,
            as_i64(cents)?,
            status,
            issuer,
            timestamp as i64
        ],
    )
    .map_err(storage)?;
    Ok(())
}

pub(super) fn ledger(
    tx: &Transaction,
    id: &str,
    account_type: &str,
    account: &str,
    direction: &str,
    raw: u64,
) -> Result<(), RetailError> {
    tx.execute(
        "INSERT INTO ledger_entries VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            Uuid::now_v7().to_string(),
            id,
            account_type,
            account,
            direction,
            as_i64(raw)?,
            now() as i64
        ],
    )
    .map_err(storage)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record(
    tx: &Transaction,
    id: &str,
    client: &str,
    kind: &str,
    raw: u64,
    cents: u64,
    status: &str,
    source: Option<&str>,
    destination: Option<&str>,
    hash: Option<&str>,
    contract: &str,
    chain: u64,
) -> Result<(), RetailError> {
    let service_type = if kind == "internal_transfer" {
        "transfer_service"
    } else {
        "exchange_of_crypto_assets_for_funds"
    };
    let record_id = Uuid::now_v7().to_string();
    let timestamp = now();
    let (fee_raw, net_raw) = if kind == "internal_transfer" {
        let fee = raw / 1_000;
        (fee, raw - fee)
    } else {
        (0, raw)
    };
    tx.execute("INSERT INTO service_records(record_id,operation_id,client_id,service_type,order_type,asset_symbol,contract_address,chain_id,quantity_raw,fiat_currency,gross_fiat_minor,fee_minor,status,source_account,destination_account,blockchain_transaction_hash,decision_actor,created_at_unix_ms) VALUES(?1,?2,?3,?4,?5,'rUSD',?6,?7,?8,'USD',?9,0,?10,?11,?12,?13,'casp-retail-demo-v1',?14)",params![record_id,id,client,service_type,kind,contract,as_i64(chain)?,as_i64(raw)?,as_i64(cents)?,status,source,destination,hash,timestamp as i64]).map_err(storage)?;
    let completed = status == "completed";
    let unit_price = if kind == "internal_transfer" {
        None
    } else {
        Some(as_i64(exchange_rate_minor(tx)?)?)
    };
    let retention = timestamp.saturating_add(5 * 365 * 24 * 60 * 60 * 1_000);
    tx.execute("INSERT INTO service_record_details(record_id,record_status,received_at_unix_ms,accepted_at_unix_ms,executed_at_unix_ms,settled_at_unix_ms,failed_at_unix_ms,price_method,unit_price_minor,gross_quantity_raw,net_quantity_raw,fee_quantity_raw,instruction_channel,execution_actor,policy_version,rejection_reason,retention_until_unix_ms) VALUES(?1,'new',?2,?2,?3,?3,NULL,?4,?5,?6,?7,?8,'demo_web','casp-retail-engine','casp-service-record-v2',NULL,?9)",params![record_id,timestamp as i64,completed.then_some(timestamp as i64),if kind=="internal_transfer"{"not_applicable"}else{"casp_admin_configured_rate"},unit_price,as_i64(raw)?,as_i64(net_raw)?,as_i64(fee_raw)?,retention as i64]).map_err(storage)?;
    Ok(())
}
