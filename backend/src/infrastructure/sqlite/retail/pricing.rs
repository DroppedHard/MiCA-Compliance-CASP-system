use super::*;

pub(super) fn get(connection: &Mutex<Connection>) -> Result<ExchangeRate, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    read_exchange_rate(&connection)
}

pub(super) fn set(
    connection: &Mutex<Connection>,
    usd_minor_per_rusd: u64,
) -> Result<ExchangeRate, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    connection
        .execute(
            "UPDATE casp_exchange_rate SET usd_minor_per_rusd=?1,updated_at_unix_ms=?2 WHERE singleton=1",
            params![as_i64(usd_minor_per_rusd)?, now() as i64],
        )
        .map_err(storage)?;
    read_exchange_rate(&connection)
}
