use super::*;

pub(super) fn begin(
    connection: &Mutex<Connection>,
    id: &str,
    client: &str,
    raw: u64,
    contract: &str,
    chain: u64,
    hot: &str,
) -> Result<RetailOrder, RetailError> {
    let cents = raw / UNITS_PER_CENT;
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    if let Some(existing) = order(&tx, id)? {
        verify_same(&existing, "redemption", client, raw, cents)?;
        return Ok(existing);
    }
    let changed=tx.execute("UPDATE client_positions SET available_raw=available_raw-?1,locked_raw=locked_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",params![as_i64(raw)?,now() as i64,client]).map_err(storage)?;
    if changed == 0 {
        return Err(RetailError::InsufficientBalance);
    }
    let issuer_id = format!("issuer-{id}");
    insert_order(
        &tx,
        id,
        client,
        "redemption",
        raw,
        cents,
        "pending_issuer",
        Some(&issuer_id),
    )?;
    ledger(&tx, id, "client", client, "debit", raw)?;
    ledger(&tx, id, "client_locked", client, "lock", raw)?;
    record(
        &tx,
        id,
        client,
        "redemption",
        raw,
        cents,
        "pending_issuer",
        Some(client),
        Some(hot),
        None,
        contract,
        chain,
    )?;
    let result =
        order(&tx, id)?.ok_or_else(|| RetailError::Storage("redemption disappeared".into()))?;
    tx.commit().map_err(storage)?;
    Ok(result)
}

pub(super) fn complete(
    connection: &Mutex<Connection>,
    id: &str,
    hash: Option<&str>,
) -> Result<RetailOrder, RetailError> {
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    let current =
        order(&tx, id)?.ok_or_else(|| RetailError::Storage("redemption not found".into()))?;
    if current.status == "completed" {
        return Ok(current);
    }
    let raw = current.quantity_raw.parse().map_err(storage)?;
    tx.execute("UPDATE client_positions SET locked_raw=locked_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND locked_raw>=?1",params![as_i64(raw)?,now() as i64,current.client_id]).map_err(storage)?;
    tx.execute("UPDATE retail_orders SET status='completed',blockchain_transaction_hash=?1,last_error=NULL,updated_at_unix_ms=?2 WHERE operation_id=?3",params![hash,now() as i64,id]).map_err(storage)?;
    ledger(&tx, id, "client_locked", &current.client_id, "debit", raw)?;
    let (contract, chain): (String, i64) = tx.query_row(
        "SELECT contract_address,chain_id FROM service_records WHERE operation_id=?1 ORDER BY created_at_unix_ms LIMIT 1",
        [id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(storage)?;
    record(
        &tx,
        id,
        &current.client_id,
        "redemption",
        raw,
        current.fiat_amount_minor.parse().map_err(storage)?,
        "completed",
        Some(&current.client_id),
        Some("issuer"),
        hash,
        &contract,
        chain as u64,
    )?;
    let result =
        order(&tx, id)?.ok_or_else(|| RetailError::Storage("redemption disappeared".into()))?;
    tx.commit().map_err(storage)?;
    Ok(result)
}

pub(super) fn fail(
    connection: &Mutex<Connection>,
    id: &str,
    message: &str,
) -> Result<(), RetailError> {
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    let timestamp = now() as i64;
    tx.execute("UPDATE retail_orders SET status='issuer_retry_required',last_error=?1,updated_at_unix_ms=?2 WHERE operation_id=?3 AND status<>'completed'",params![message,timestamp,id]).map_err(storage)?;
    tx.execute("UPDATE service_record_details SET failed_at_unix_ms=?1,rejection_reason=?2 WHERE record_id=(SELECT record_id FROM service_records WHERE operation_id=?3 ORDER BY created_at_unix_ms DESC LIMIT 1)",params![timestamp,message,id]).map_err(storage)?;
    tx.commit().map_err(storage)
}
