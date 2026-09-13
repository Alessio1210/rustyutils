use crate::table::{Table, Value};

pub fn to_json_rows(table: &Table) -> String {
    let mut output = String::from("[");
    for row in 0..table.row_count() {
        if row != 0 {
            output.push(',');
        }
        output.push('{');
        for (index, column) in table.columns().iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            push_json_string(&mut output, &column.name);
            output.push(':');
            push_value(&mut output, &column.values[row]);
        }
        output.push('}');
    }
    output.push(']');
    output
}

fn push_value(output: &mut String, value: &Value) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Int(value) => output.push_str(&value.to_string()),
        Value::Float(value) if value.is_finite() => output.push_str(&value.to_string()),
        Value::Float(_) => output.push_str("null"),
        Value::Text(value) => push_json_string(output, value),
        Value::DateTimeMicros(value) => push_json_string(output, &format_datetime(*value)),
    }
}
fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                use std::fmt::Write;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

fn format_datetime(micros: i64) -> String {
    let seconds = micros.div_euclid(1_000_000);
    let fraction = micros.rem_euclid(1_000_000);
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = time / 3_600;
    let minute = (time % 3_600) / 60;
    let second = time % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{fraction:06}Z")
}
// Gregorian conversion for days since 1970-01-01; derived from civil calendar arithmetic.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (
        year + if mp < 10 { 0 } else { 1 },
        doy - (153 * mp + 2) / 5 + 1,
        mp + if mp < 10 { 3 } else { -9 },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Column;
    #[test]
    fn escapes_and_formats() {
        let table = Table::new(vec![
            Column::new("x\n", vec![Value::Text("\"\\".into())]),
            Column::new("date", vec![Value::DateTimeMicros(0)]),
        ])
        .unwrap();
        assert_eq!(
            to_json_rows(&table),
            "[{\"x\\n\":\"\\\"\\\\\",\"date\":\"1970-01-01T00:00:00.000000Z\"}]"
        );
    }
}
