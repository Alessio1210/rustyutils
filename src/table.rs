use polars::prelude::{
    Column as PolarsColumn, DataFrame, DataType, IntoColumn, NamedFrom, Series, TimeUnit,
};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    /// Microseconds since Unix epoch, encoded as UTC JSON timestamp.
    DateTimeMicros(i64),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Int(_) => "integer",
            Self::Float(_) => "float",
            Self::Text(_) => "text",
            Self::DateTimeMicros(_) => "datetime",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    pub values: Vec<Value>,
}

impl Column {
    pub fn new(name: impl Into<String>, values: Vec<Value>) -> Self {
        Self {
            name: name.into(),
            values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::DataType;

    #[test]
    fn converts_typed_columns_to_polars() {
        let table = Table::new(vec![
            Column::new("id", vec![Value::Int(1), Value::Null]),
            Column::new("at", vec![Value::DateTimeMicros(0), Value::Null]),
        ])
        .unwrap();
        let data_frame = table.to_polars().unwrap();
        assert_eq!(data_frame.height(), 2);
        assert_eq!(data_frame.column("id").unwrap().dtype(), &DataType::Int64);
        assert!(matches!(
            data_frame.column("at").unwrap().dtype(),
            DataType::Datetime(TimeUnit::Microseconds, None)
        ));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    columns: Vec<Column>,
    rows: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableError {
    DuplicateColumn(String),
    UnequalColumnLength {
        column: String,
        expected: usize,
        actual: usize,
    },
    MissingColumn(String),
    InvalidRow {
        row: usize,
        rows: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolarsConversionError {
    MixedTypes {
        column: String,
        first: &'static str,
        found: &'static str,
    },
    Polars(String),
}

impl fmt::Display for PolarsConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MixedTypes {
                column,
                first,
                found,
            } => write!(f, "column {column} mixes {first} with {found}"),
            Self::Polars(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for PolarsConversionError {}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateColumn(name) => write!(f, "duplicate column: {name}"),
            Self::UnequalColumnLength {
                column,
                expected,
                actual,
            } => write!(f, "column {column} has {actual} rows, expected {expected}"),
            Self::MissingColumn(name) => write!(f, "missing column: {name}"),
            Self::InvalidRow { row, rows } => write!(f, "row {row} outside table with {rows} rows"),
        }
    }
}

impl std::error::Error for TableError {}

impl Table {
    pub fn new(columns: Vec<Column>) -> Result<Self, TableError> {
        let rows = columns.first().map_or(0, |column| column.values.len());
        for (index, column) in columns.iter().enumerate() {
            if columns[..index]
                .iter()
                .any(|previous| previous.name == column.name)
            {
                return Err(TableError::DuplicateColumn(column.name.clone()));
            }
            if column.values.len() != rows {
                return Err(TableError::UnequalColumnLength {
                    column: column.name.clone(),
                    expected: rows,
                    actual: column.values.len(),
                });
            }
        }
        Ok(Self { columns, rows })
    }

    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: 0,
        }
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    pub fn row_count(&self) -> usize {
        self.rows
    }
    pub fn column(&self, name: &str) -> Result<&Column, TableError> {
        self.columns
            .iter()
            .find(|column| column.name == name)
            .ok_or_else(|| TableError::MissingColumn(name.into()))
    }
    pub fn value(&self, column: &str, row: usize) -> Result<&Value, TableError> {
        if row >= self.rows {
            return Err(TableError::InvalidRow {
                row,
                rows: self.rows,
            });
        }
        Ok(&self.column(column)?.values[row])
    }

    /// Convert native results to a typed Polars `DataFrame`.
    /// Null values stay null. Mixed non-null types are rejected, never coerced.
    pub fn to_polars(&self) -> Result<DataFrame, PolarsConversionError> {
        let columns = self
            .columns
            .iter()
            .map(Column::to_polars)
            .collect::<Result<Vec<_>, _>>()?;
        DataFrame::new_infer_height(columns)
            .map_err(|error| PolarsConversionError::Polars(error.to_string()))
    }
}

impl Column {
    fn to_polars(&self) -> Result<PolarsColumn, PolarsConversionError> {
        let kind = self
            .values
            .iter()
            .find(|value| !matches!(value, Value::Null))
            .map(Value::type_name);
        let compatible =
            |value: &Value| matches!(value, Value::Null) || Some(value.type_name()) == kind;
        if let Some(value) = self.values.iter().find(|value| !compatible(value)) {
            return Err(PolarsConversionError::MixedTypes {
                column: self.name.clone(),
                first: kind.unwrap_or("null"),
                found: value.type_name(),
            });
        }
        let series = match kind {
            None => Series::new(
                self.name.clone().into(),
                vec![Option::<String>::None; self.values.len()],
            ),
            Some("integer") => Series::new(
                self.name.clone().into(),
                self.values
                    .iter()
                    .map(|value| match value {
                        Value::Int(number) => Some(*number),
                        Value::Null => None,
                        _ => unreachable!(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Some("float") => Series::new(
                self.name.clone().into(),
                self.values
                    .iter()
                    .map(|value| match value {
                        Value::Float(number) => Some(*number),
                        Value::Null => None,
                        _ => unreachable!(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Some("text") => Series::new(
                self.name.clone().into(),
                self.values
                    .iter()
                    .map(|value| match value {
                        Value::Text(text) => Some(text.as_str()),
                        Value::Null => None,
                        _ => unreachable!(),
                    })
                    .collect::<Vec<_>>(),
            ),
            Some("datetime") => Series::new(
                self.name.clone().into(),
                self.values
                    .iter()
                    .map(|value| match value {
                        Value::DateTimeMicros(number) => Some(*number),
                        Value::Null => None,
                        _ => unreachable!(),
                    })
                    .collect::<Vec<_>>(),
            )
            .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
            .map_err(|error| PolarsConversionError::Polars(error.to_string()))?,
            Some(_) => unreachable!(),
        };
        Ok(series.into_column())
    }
}
