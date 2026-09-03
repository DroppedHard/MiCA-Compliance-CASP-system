use super::*;

pub(super) fn post(
    connection: &Mutex<Connection>,
    command: TransferPosting<'_>,
) -> Result<InternalTransfer, RetailError> {
    let TransferPosting {
        id,
        sender,
        recipient,
        gross_raw: gross,
        purpose,
        contract,
        chain,
    } = command;
    let fee = gross / 1_000;
    let net = gross
        .checked_sub(fee)
        .ok_or_else(|| RetailError::Invalid("transfer fee exceeds amount".into()))?;
    let mut connection = connection.lock().map_err(storage)?;
    let tx = connection.transaction().map_err(storage)?;
    if let Some(existing) = internal_transfer(&tx, id)? {
        if existing.sender_client_id == sender
            && existing.recipient_client_id == recipient
            && existing.gross_raw == gross.to_string()
            && existing.purpose_classification == purpose
        {
            return Ok(existing);
        }
        return Err(RetailError::IdempotencyConflict);
    }
    let changed = tx.execute(
        "UPDATE client_positions SET available_raw=available_raw-?1,updated_at_unix_ms=?2 WHERE client_id=?3 AND available_raw>=?1",
        params![as_i64(gross)?, now() as i64, sender],
    ).map_err(storage)?;
    if changed == 0 {
        return Err(RetailError::InsufficientBalance);
    }
    tx.execute(
        "UPDATE client_positions SET available_raw=available_raw+?1,updated_at_unix_ms=?2 WHERE client_id=?3",
        params![as_i64(net)?, now() as i64, recipient],
    ).map_err(storage)?;
    tx.execute(
        "UPDATE fee_position SET pending_raw=pending_raw+?1 WHERE singleton=1",
        [as_i64(fee)?],
    )
    .map_err(storage)?;
    tx.execute(
        "INSERT INTO internal_transfers(operation_id,sender_client_id,recipient_client_id,gross_raw,fee_raw,net_raw,purpose_classification,status,created_at_unix_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,'completed',?8)",
        params![id, sender, recipient, as_i64(gross)?, as_i64(fee)?, as_i64(net)?, purpose, now() as i64],
    ).map_err(storage)?;
    ledger(&tx, id, "client", sender, "debit", gross)?;
    ledger(&tx, id, "client", recipient, "credit", net)?;
    ledger(&tx, id, "casp_fee_pending", "casp-fees", "credit", fee)?;
    record(
        &tx,
        id,
        sender,
        "internal_transfer",
        gross,
        0,
        "completed",
        Some(sender),
        Some(recipient),
        None,
        contract,
        chain,
    )?;
    let result = internal_transfer(&tx, id)?
        .ok_or_else(|| RetailError::Storage("transfer disappeared".into()))?;
    tx.commit().map_err(storage)?;
    Ok(result)
}

pub(super) fn fee_position(connection: &Mutex<Connection>) -> Result<FeePosition, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let pending: i64 = connection
        .query_row(
            "SELECT pending_raw FROM fee_position WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(storage)?;
    Ok(FeePosition {
        pending_raw: pending.to_string(),
    })
}
