use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OdbcConfig {
    pub driver_name: String,
    pub server: String,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl OdbcConfig {
    /// Build connection string equivalent to former C++ fast path.
    /// Values containing `;` or `}` are rejected by callers before connection.
    pub fn connection_string(&self) -> String {
        let mut value = format!(
            "DRIVER={{{}}};SERVER={};DATABASE={};UID={};PWD={};",
            self.driver_name, self.server, self.database, self.user, self.password
        );
        if self.driver_name.contains("SQL Server") {
            value.push_str("TrustServerCertificate=yes;Encrypt=no;");
        }
        value
    }
    pub fn redacted_connection_string(&self) -> String {
        let mut value = String::new();
        let _ = write!(
            value,
            "DRIVER={{{}}};SERVER={};DATABASE={};UID={};PWD=***;",
            self.driver_name, self.server, self.database, self.user
        );
        value
    }
}

#[cfg(feature = "odbc")]
#[derive(Debug)]
pub enum OdbcError {
    Driver(odbc_api::Error),
    Table(crate::TableError),
    Polars(crate::PolarsConversionError),
}

#[cfg(feature = "odbc")]
impl std::fmt::Display for OdbcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Driver(error) => error.fmt(formatter),
            Self::Table(error) => error.fmt(formatter),
            Self::Polars(error) => error.fmt(formatter),
        }
    }
}

#[cfg(feature = "odbc")]
impl std::error::Error for OdbcError {}

#[cfg(feature = "odbc")]
impl From<odbc_api::Error> for OdbcError {
    fn from(error: odbc_api::Error) -> Self {
        Self::Driver(error)
    }
}

#[cfg(feature = "odbc")]
impl From<crate::TableError> for OdbcError {
    fn from(error: crate::TableError) -> Self {
        Self::Table(error)
    }
}

#[cfg(feature = "odbc")]
impl From<crate::PolarsConversionError> for OdbcError {
    fn from(error: crate::PolarsConversionError) -> Self {
        Self::Polars(error)
    }
}

/// Execute parameterized SQL and return a Polars `DataFrame`.
#[cfg(feature = "odbc")]
pub fn fetch_odbc<P>(
    config: &OdbcConfig,
    query: &str,
    params: P,
) -> Result<polars::prelude::DataFrame, OdbcError>
where
    P: odbc_api::ParameterCollectionRef,
{
    Ok(fetch_odbc_table(config, query, params)?.to_polars()?)
}

/// Execute one parameterized query and return its rows as a Polars `DataFrame`.
///
/// This is the Rust equivalent of Python's `Connection.run_query`. It deliberately
/// does no hidden caching or retry: callers can choose a [`crate::Cache`] policy
/// and surface database failures instead of receiving stale data unexpectedly.
/// Pass `()` when the SQL has no `?` placeholders.
#[cfg(feature = "odbc")]
pub fn run_query<P>(
    config: &OdbcConfig,
    query: &str,
    params: P,
) -> Result<polars::prelude::DataFrame, OdbcError>
where
    P: odbc_api::ParameterCollectionRef,
{
    fetch_odbc(config, query, params)
}

/// Same reader as [`fetch_odbc`], retaining crate-native column data.
///
/// ODBC text buffers preserve `NULL` as `Value::Null`; database values are text
/// because a dynamic ODBC schema can contain arbitrary driver-specific types.
/// Convert selected columns after fetch when schema-specific types matter.
#[cfg(feature = "odbc")]
pub fn fetch_odbc_table<P>(
    config: &OdbcConfig,
    query: &str,
    params: P,
) -> Result<crate::Table, OdbcError>
where
    P: odbc_api::ParameterCollectionRef,
{
    use crate::{Column, Value};
    use odbc_api::{
        ConnectionOptions, Cursor, Environment, ResultSetMetadata, buffers::TextRowSet,
    };

    const BATCH_SIZE: usize = 1_024;
    const MAX_TEXT_BYTES: usize = 16 * 1_024;

    let environment = Environment::new()?;
    let connection = environment.connect_with_connection_string(
        &config.connection_string(),
        ConnectionOptions::default(),
    )?;
    let Some(mut cursor) = connection.execute(query, params, None)? else {
        return Ok(crate::Table::empty());
    };
    let names = cursor.column_names()?.collect::<Result<Vec<_>, _>>()?;
    let mut columns = names
        .into_iter()
        .map(|name| Column::new(name, Vec::new()))
        .collect::<Vec<_>>();
    let mut buffers = TextRowSet::for_cursor(BATCH_SIZE, &mut cursor, Some(MAX_TEXT_BYTES))?;
    let mut row_set_cursor = cursor.bind_buffer(&mut buffers)?;
    while let Some(batch) = row_set_cursor.fetch()? {
        for row in 0..batch.num_rows() {
            for column in 0..batch.num_cols() {
                let value = batch
                    .at(column, row)
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                    .map(Value::Text)
                    .unwrap_or(Value::Null);
                columns[column].values.push(value);
            }
        }
    }
    Ok(crate::Table::new(columns)?)
}
