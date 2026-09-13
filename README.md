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
