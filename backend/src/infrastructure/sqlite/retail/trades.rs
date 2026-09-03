use super::*;

pub(super) fn purchase(
    connection: &Mutex<Connection>,
    id: &str,
    client: &str,
    cents: u64,
    contract: &str,
    chain: u64,
) -> Result<RetailOrder, RetailError> {
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    let rate = exchange_rate_minor(&tx)?;
    let raw = cents
        .checked_mul(1_000_000)
        .ok_or_else(|| RetailError::Invalid("amount is too large".into()))?
        / rate;
    if raw == 0 {
        return Err(RetailError::Invalid(
            "amount is below the configured exchange-rate precision".into(),
        ));
    }
    if let Some(existing) = order(&tx, id)? {
        verify_same(&existing, "purchase", client, raw, cents)?;
        return Ok(existing);
    }
    let changed = tx.execute("UPDATE inventory_state SET available_raw=available_raw-?1 WHERE singleton=1 AND available_raw>=?1", [as_i64(raw)?]).map_err(storage)?;
    if changed == 0 {
        return Err(RetailError::InsufficientInventory);
    }
    tx.execute("UPDATE client_positions SET available_raw=available_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3", params![as_i64(raw)?, now() as i64, client]).map_err(storage)?;
    insert_order(&tx, id, client, "purchase", raw, cents, "completed", None)?;
    ledger(&tx, id, "inventory", "casp-inventory", "debit", raw)?;
    ledger(&tx, id, "client", client, "credit", raw)?;
    record(
        &tx,
        id,
        client,
        "purchase",
        raw,
        cents,
        "completed",
        Some("casp-inventory"),
        Some(client),
        None,
        contract,
        chain,
    )?;
    let result =
        order(&tx, id)?.ok_or_else(|| RetailError::Storage("purchase disappeared".into()))?;
    tx.commit().map_err(storage)?;
    Ok(result)
}

pub(super) fn sale(
    connection: &Mutex<Connection>,
    id: &str,
    client: &str,
    raw: u64,
    contract: &str,
    chain: u64,
) -> Result<RetailOrder, RetailError> {
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    let rate = exchange_rate_minor(&tx)?;
    let cents = raw
        .checked_mul(rate)
        .ok_or_else(|| RetailError::Invalid("amount is too large".into()))?
        / 1_000_000;
    if let Some(existing) = order(&tx, id)? {
        verify_same(&existing, "sale", client, raw, cents)?;
        return Ok(existing);
    }
    let changed=tx.execute("UPDATE client_positions SET available_raw=available_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",params![as_i64(raw)?,now() as i64,client]).map_err(storage)?;
    if changed == 0 {
        return Err(RetailError::InsufficientBalance);
    }
    tx.execute(
        "UPDATE inventory_state SET available_raw=available_raw+?1 WHERE singleton=1",
        [as_i64(raw)?],
    )
    .map_err(storage)?;
    insert_order(&tx, id, client, "sale", raw, cents, "completed", None)?;
    ledger(&tx, id, "client", client, "debit", raw)?;
    ledger(&tx, id, "inventory", "casp-inventory", "credit", raw)?;
    record(
        &tx,
        id,
        client,
        "sale",
        raw,
        cents,
        "completed",
        Some(client),
        Some("casp-inventory"),
        None,
        contract,
        chain,
    )?;
    let result = order(&tx, id)?.ok_or_else(|| RetailError::Storage("sale disappeared".into()))?;
    tx.commit().map_err(storage)?;
    Ok(result)
}
