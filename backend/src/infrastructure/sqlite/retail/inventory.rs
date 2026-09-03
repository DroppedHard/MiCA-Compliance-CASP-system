use super::*;

pub(super) fn activate(connection: &Mutex<Connection>, amount: u64) -> Result<(), RetailError> {
    connection
        .lock()
        .map_err(storage)?
        .execute(
            "UPDATE inventory_state SET available_raw=?1,activated_at_unix_ms=?2 WHERE singleton=1 AND activated_at_unix_ms IS NULL",
            params![as_i64(amount)?, now() as i64],
        )
        .map_err(storage)?;
    Ok(())
}

pub(super) fn add_once(
    connection: &Mutex<Connection>,
    operation: &str,
    wallet: &str,
    amount: u64,
) -> Result<(), RetailError> {
    let mut connection = connection.lock().map_err(storage)?;
    let transaction = connection.transaction().map_err(storage)?;
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO inventory_replenishment_postings(operation_id,wallet_role,amount_raw,created_at_unix_ms) VALUES(?1,?2,?3,?4)",
            params![operation, wallet, as_i64(amount)?, now() as i64],
        )
        .map_err(storage)?;
    if inserted == 1 {
        transaction
            .execute(
                "UPDATE inventory_state SET available_raw=available_raw+?1 WHERE singleton=1 AND activated_at_unix_ms IS NOT NULL",
                [as_i64(amount)?],
            )
            .map_err(storage)?;
    } else {
        let recorded: i64 = transaction
            .query_row(
                "SELECT amount_raw FROM inventory_replenishment_postings WHERE operation_id=?1 AND wallet_role=?2",
                params![operation, wallet],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if recorded != as_i64(amount)? {
            return Err(RetailError::IdempotencyConflict);
        }
    }
    transaction.commit().map_err(storage)
}
