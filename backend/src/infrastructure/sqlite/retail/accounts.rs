use super::*;

pub(super) fn account(
    connection: &Mutex<Connection>,
    client: &str,
) -> Result<ClientAccount, RetailError> {
    connection
        .lock()
        .map_err(storage)?
        .query_row(
            "SELECT p.available_raw,p.locked_raw,i.available_raw,w.wallet_address FROM client_positions p JOIN client_wallets w ON w.client_id=p.client_id CROSS JOIN inventory_state i WHERE p.client_id=?1 AND i.singleton=1",
            [client],
            |row| Ok(ClientAccount {
                client_id: client.to_owned(),
                wallet_address: row.get(3)?,
                available_raw: row.get::<_, i64>(0)?.to_string(),
                locked_raw: row.get::<_, i64>(1)?.to_string(),
                inventory_available_raw: row.get::<_, i64>(2)?.to_string(),
            }),
        )
        .map_err(storage)
}

pub(super) fn accounts(connection: &Mutex<Connection>) -> Result<Vec<ClientAccount>, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let inventory: i64 = connection
        .query_row(
            "SELECT available_raw FROM inventory_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(storage)?;
    let mut statement=connection.prepare("SELECT p.client_id,p.available_raw,p.locked_raw,w.wallet_address FROM client_positions p JOIN client_wallets w ON w.client_id=p.client_id WHERE p.client_id IN ('alice','bob','carol') ORDER BY p.client_id").map_err(storage)?;
    statement
        .query_map([], |row| {
            Ok(ClientAccount {
                client_id: row.get(0)?,
                wallet_address: row.get(3)?,
                available_raw: row.get::<_, i64>(1)?.to_string(),
                locked_raw: row.get::<_, i64>(2)?.to_string(),
                inventory_available_raw: inventory.to_string(),
            })
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)
}

pub(super) fn client_id_by_wallet(
    connection: &Mutex<Connection>,
    wallet_address: &str,
) -> Result<Option<String>, RetailError> {
    connection
        .lock()
        .map_err(storage)?
        .query_row(
            "SELECT client_id FROM client_wallets WHERE wallet_address=?1",
            [wallet_address],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage)
}
