use super::*;

pub(super) fn for_client(
    connection: &Mutex<Connection>,
    client: &str,
) -> Result<Vec<ServiceRecord>, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let mut statement = connection.prepare(&format!(
        "{RECORD_SELECT} WHERE s.client_id=?1 OR s.source_account=?1 OR s.destination_account=?1 ORDER BY s.created_at_unix_ms DESC,s.record_id DESC LIMIT 200"
    )).map_err(storage)?;
    statement
        .query_map([client], map_record)
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)
}

pub(super) fn all(connection: &Mutex<Connection>) -> Result<Vec<ServiceRecord>, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let mut statement = connection
        .prepare(&format!(
            "{RECORD_SELECT} ORDER BY s.created_at_unix_ms DESC,s.record_id DESC LIMIT 1000"
        ))
        .map_err(storage)?;
    statement
        .query_map([], map_record)
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)
}

pub(super) fn amend(
    connection: &Mutex<Connection>,
    original: &str,
    amendment_type: &str,
    reason: &str,
) -> Result<ServiceRecordAmendment, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM service_records WHERE record_id=?1)",
            [original],
            |row| row.get(0),
        )
        .map_err(storage)?;
    if !exists {
        return Err(RetailError::Invalid(
            "original service record does not exist".into(),
        ));
    }
    let amendment = ServiceRecordAmendment {
        amendment_id: Uuid::now_v7().to_string(),
        original_record_id: original.to_owned(),
        amendment_type: amendment_type.to_owned(),
        reason: reason.to_owned(),
        actor: "casp-admin-demo".into(),
        created_at_unix_ms: now(),
    };
    connection
        .execute(
            "INSERT INTO service_record_amendments VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                amendment.amendment_id,
                amendment.original_record_id,
                amendment.amendment_type,
                amendment.reason,
                amendment.actor,
                amendment.created_at_unix_ms as i64
            ],
        )
        .map_err(storage)?;
    Ok(amendment)
}

pub(super) fn amendments(
    connection: &Mutex<Connection>,
) -> Result<Vec<ServiceRecordAmendment>, RetailError> {
    let connection = connection.lock().map_err(storage)?;
    let mut statement = connection.prepare(
        "SELECT amendment_id,original_record_id,amendment_type,reason,actor,created_at_unix_ms FROM service_record_amendments ORDER BY created_at_unix_ms DESC"
    ).map_err(storage)?;
    statement
        .query_map([], |row| {
            Ok(ServiceRecordAmendment {
                amendment_id: row.get(0)?,
                original_record_id: row.get(1)?,
                amendment_type: row.get(2)?,
                reason: row.get(3)?,
                actor: row.get(4)?,
                created_at_unix_ms: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)
}
