# rustyutils

Native Rust replacement for `pyutils/cpputils`.

- `Table` and `Column`: validated columnar data model.
- `Table::to_polars()`: typed `polars::DataFrame` results, including nulls and UTC microsecond timestamps.
- `aggregate`: grouped `sum`, `mean`, `count`, `first`, `last`, `min`, and `max`.
- `Cache`: thread-safe in-memory TTL cache with deterministic FNV-1a keys.
- `to_json_rows`: JSON rows with correct escaping.
- `OdbcConfig`: connection-string builder with redacted logging variant.
- `fetch_odbc`: optional batched ODBC reader returning `polars::DataFrame`.
  Enable with `--features odbc`; Linux needs `unixodbc-dev` at build time.
- `run_query`: ergonomic `fetch_odbc` entry point, equivalent to former Python
  `Connection.run_query` and also returning `polars::DataFrame`.

```rust
use rustyutils::{aggregate, Aggregation, AggregationOp, Column, Table, Value};

let source = Table::new(vec![
    Column::new("machine", vec![Value::Text("a".into()), Value::Text("a".into())]),
    Column::new("value", vec![Value::Int(2), Value::Int(4)]),
])?;

let result = aggregate(
    &source,
    &["machine"],
    &[Aggregation::new("total", "value", AggregationOp::Sum)],
)?;
let data_frame = result.to_polars()?;
```

Run checks with `cargo test`.

## ODBC queries

Enable ODBC at compile time. `params` is a typed tuple; use `()` when the SQL
has no placeholders. Results are a Polars `DataFrame`.

```rust,no_run
use rustyutils::{run_query, OdbcConfig};

let database = OdbcConfig {
    driver_name: "ODBC Driver 18 for SQL Server".into(),
    server: "db.internal".into(),
    database: "telemetry".into(),
    user: "reader".into(),
    password: std::env::var("DATABASE_PASSWORD")?,
};

let data_frame = run_query(
    &database,
    "SELECT machine_id, value FROM measurements WHERE machine_id = ?",
    ("machine-42",),
)?;
```

Build with `cargo test --features odbc`. `run_query` has no implicit cache or
retry. Use `Cache` around repeated, read-only queries when stale results are OK.

## Project connection aliases

Create `src/connections.rs` in each consuming project. Secrets remain outside
Git in `~/.env/.env`:

```dotenv
REPORTING_HOST=db.internal
REPORTING_DATABASE=reporting
REPORTING_USER=reader
REPORTING_PASSWORD=change-me
```

```rust
use rustyutils::{connections, ConnectionSpec};

connections! {
    pub struct Connections {
        reporting: ConnectionSpec::new(
            "ODBC Driver 18 for SQL Server",
            "REPORTING_HOST",
            "REPORTING_DATABASE",
            "REPORTING_USER",
            "REPORTING_PASSWORD",
        ),
    }
}
```

Use typed aliases. This preserves Rust compile-time checks while keeping the
familiar call form:

```rust,no_run
# use rustyutils::ConnectionConfigError;
# struct Connections { reporting: rustyutils::DatabaseConnection }
# fn example(con: Connections) -> Result<(), Box<dyn std::error::Error>> {
let data_frame = con.reporting.run_query("SELECT * FROM reports", ())?;
# Ok(()) }
```
