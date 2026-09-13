//! Column-oriented utility library, ported from `pyutils/cpputils`.
//!
//! The crate has no runtime or Python dependency. Keep adapters at the edge;
//! keep tables, cache, aggregation, JSON, and database configuration here.

pub mod aggregate;
pub mod cache;
pub mod connections;
pub mod json;
pub mod odbc;
pub mod table;

pub use aggregate::{Aggregation, AggregationOp, aggregate};
pub use cache::{Cache, DEFAULT_CACHE_TTL, stable_hash};
pub use connections::{ConnectionConfigError, ConnectionSpec, DatabaseConnection, Secrets};
pub use json::to_json_rows;
pub use odbc::OdbcConfig;
#[cfg(feature = "odbc")]
pub use odbc::{OdbcError, fetch_odbc, fetch_odbc_table, run_query};
pub use table::{Column, PolarsConversionError, Table, TableError, Value};
