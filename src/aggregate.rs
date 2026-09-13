use crate::table::{Column, Table, TableError, Value};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregationOp {
    Sum,
    Mean,
    Count,
    First,
    Last,
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregation {
    pub output: String,
    pub source: String,
    pub operation: AggregationOp,
}

impl Aggregation {
    pub fn new(
        output: impl Into<String>,
        source: impl Into<String>,
        operation: AggregationOp,
    ) -> Self {
        Self {
            output: output.into(),
            source: source.into(),
            operation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateError {
    Table(TableError),
    DuplicateOutput(String),
    Unsupported {
        operation: AggregationOp,
        value_type: &'static str,
        column: String,
    },
}

impl fmt::Display for AggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(error) => error.fmt(f),
            Self::DuplicateOutput(name) => write!(f, "duplicate output column: {name}"),
            Self::Unsupported {
                operation,
                value_type,
                column,
            } => write!(
                f,
                "{operation:?} does not support {value_type} in column {column}"
            ),
        }
    }
}
impl std::error::Error for AggregateError {}
impl From<TableError> for AggregateError {
    fn from(value: TableError) -> Self {
        Self::Table(value)
    }
}

#[derive(Default)]
struct Group {
    keys: Vec<Value>,
    states: Vec<State>,
}
#[derive(Default)]
struct State {
    count: u64,
    first: Option<Value>,
    last: Option<Value>,
    min: Option<Value>,
    max: Option<Value>,
    integer_sum: i64,
    float_sum: f64,
    has_float: bool,
}

pub fn aggregate(
    table: &Table,
    group_by: &[&str],
    specifications: &[Aggregation],
) -> Result<Table, AggregateError> {
    for name in group_by {
        table.column(name)?;
    }
    for specification in specifications {
        table.column(&specification.source)?;
    }
    let mut output_names: Vec<&str> = group_by.to_vec();
    for specification in specifications {
        if output_names.contains(&specification.output.as_str()) {
            return Err(AggregateError::DuplicateOutput(
                specification.output.clone(),
            ));
        }
        output_names.push(&specification.output);
    }

    let mut groups: Vec<Group> = Vec::new();
    let mut indices: HashMap<String, usize> = HashMap::new();
    for row in 0..table.row_count() {
        let keys: Vec<Value> = group_by
            .iter()
            .map(|name| table.value(name, row).cloned())
            .collect::<Result<_, _>>()?;
        let encoded = group_key(&keys);
        let group_index = match indices.get(&encoded) {
            Some(index) => *index,
            None => {
                let index = groups.len();
                indices.insert(encoded, index);
                groups.push(Group {
                    keys,
                    states: (0..specifications.len())
                        .map(|_| State::default())
                        .collect(),
                });
                index
            }
        };
        for (index, specification) in specifications.iter().enumerate() {
            let value = table.value(&specification.source, row)?;
            update(&mut groups[group_index].states[index], value, specification)?;
        }
    }

    let mut columns: Vec<Column> = group_by
        .iter()
        .enumerate()
        .map(|(index, name)| {
            Column::new(
                *name,
                groups
                    .iter()
                    .map(|group| group.keys[index].clone())
                    .collect(),
            )
        })
        .collect();
    for (index, specification) in specifications.iter().enumerate() {
        let values = groups
            .iter()
            .map(|group| finish(&group.states[index], specification))
            .collect::<Result<_, _>>()?;
        columns.push(Column::new(&specification.output, values));
    }
    Ok(Table::new(columns)?)
}

fn update(
    state: &mut State,
    value: &Value,
    specification: &Aggregation,
) -> Result<(), AggregateError> {
    state.count += 1;
    if state.first.is_none() {
        state.first = Some(value.clone());
    }
    state.last = Some(value.clone());
    if matches!(
        specification.operation,
        AggregationOp::Count | AggregationOp::First | AggregationOp::Last
    ) {
        return Ok(());
    }
    match value {
        Value::Null => Ok(()),
        Value::Int(number) | Value::DateTimeMicros(number) => {
            match specification.operation {
                AggregationOp::Sum | AggregationOp::Mean => {
                    state.integer_sum = state
                        .integer_sum
                        .checked_add(*number)
                        .ok_or_else(|| unsupported(specification, value))?
                }
                AggregationOp::Min => set_min(&mut state.min, value),
                AggregationOp::Max => set_max(&mut state.max, value),
                _ => unreachable!(),
            }
            Ok(())
        }
        Value::Float(number) => {
            state.has_float = true;
            match specification.operation {
                AggregationOp::Sum | AggregationOp::Mean => state.float_sum += number,
                AggregationOp::Min => set_min(&mut state.min, value),
                AggregationOp::Max => set_max(&mut state.max, value),
                _ => unreachable!(),
            }
            Ok(())
        }
        Value::Text(_)
            if matches!(
                specification.operation,
                AggregationOp::Min | AggregationOp::Max
            ) =>
        {
            if specification.operation == AggregationOp::Min {
                set_min(&mut state.min, value)
            } else {
                set_max(&mut state.max, value)
            };
            Ok(())
        }
        _ => Err(unsupported(specification, value)),
    }
}

fn finish(state: &State, specification: &Aggregation) -> Result<Value, AggregateError> {
    Ok(match specification.operation {
        AggregationOp::Count => Value::Int(state.count as i64),
        AggregationOp::First => state.first.clone().unwrap_or(Value::Null),
        AggregationOp::Last => state.last.clone().unwrap_or(Value::Null),
        AggregationOp::Min => state.min.clone().unwrap_or(Value::Null),
        AggregationOp::Max => state.max.clone().unwrap_or(Value::Null),
        AggregationOp::Sum if state.has_float => Value::Float(state.float_sum),
        AggregationOp::Sum => Value::Int(state.integer_sum),
        AggregationOp::Mean if state.count == 0 => Value::Null,
        AggregationOp::Mean if state.has_float => {
            Value::Float(state.float_sum / state.count as f64)
        }
        AggregationOp::Mean => Value::Float(state.integer_sum as f64 / state.count as f64),
    })
}

fn unsupported(specification: &Aggregation, value: &Value) -> AggregateError {
    AggregateError::Unsupported {
        operation: specification.operation,
        value_type: value.type_name(),
        column: specification.source.clone(),
    }
}
fn set_min(slot: &mut Option<Value>, candidate: &Value) {
    if slot
        .as_ref()
        .is_none_or(|current| compare(candidate, current).is_lt())
    {
        *slot = Some(candidate.clone());
    }
}
fn set_max(slot: &mut Option<Value>, candidate: &Value) {
    if slot
        .as_ref()
        .is_none_or(|current| compare(candidate, current).is_gt())
    {
        *slot = Some(candidate.clone());
    }
}
fn compare(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) | (Value::DateTimeMicros(a), Value::DateTimeMicros(b)) => {
            a.cmp(b)
        }
        (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}
fn group_key(values: &[Value]) -> String {
    values
        .iter()
        .map(|value| match value {
            Value::Null => "N".into(),
            Value::Int(number) => format!("I:{number}"),
            Value::Float(number) => format!("F:{:x}", number.to_bits()),
            Value::Text(text) => format!("S:{}:{text}", text.len()),
            Value::DateTimeMicros(number) => format!("D:{number}"),
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_and_aggregates() {
        let table = Table::new(vec![
            Column::new(
                "machine",
                vec![
                    Value::Text("a".into()),
                    Value::Text("a".into()),
                    Value::Text("b".into()),
                ],
            ),
            Column::new("value", vec![Value::Int(2), Value::Int(4), Value::Int(3)]),
        ])
        .unwrap();
        let out = aggregate(
            &table,
            &["machine"],
            &[
                Aggregation::new("total", "value", AggregationOp::Sum),
                Aggregation::new("mean", "value", AggregationOp::Mean),
            ],
        )
        .unwrap();
        assert_eq!(
            out.column("total").unwrap().values,
            vec![Value::Int(6), Value::Int(3)]
        );
        assert_eq!(
            out.column("mean").unwrap().values,
            vec![Value::Float(3.0), Value::Float(3.0)]
        );
    }
}
