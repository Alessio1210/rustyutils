//! Project-level connection configuration.
//!
//! Load secrets from `~/.env/.env`, define named database aliases in the
//! application, then call `con.alias.run_query(...)`.

use crate::OdbcConfig;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ConnectionConfigError {
    HomeDirectoryMissing,
    ReadEnvFile {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidEnvLine {
        line: usize,
    },
    MissingSecret {
        name: String,
    },
}

impl fmt::Display for ConnectionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeDirectoryMissing => {
                formatter.write_str("HOME is not set; cannot locate ~/.env/.env")
            }
            Self::ReadEnvFile { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::InvalidEnvLine { line } => write!(formatter, "invalid .env line {line}"),
            Self::MissingSecret { name } => write!(formatter, "missing required secret: {name}"),
        }
    }
}
impl std::error::Error for ConnectionConfigError {}

/// Parsed secrets. This type does not mutate process environment variables.
/// It is therefore safe to load before or during multi-threaded execution.
#[derive(Clone, Debug, Default)]
pub struct Secrets {
    values: HashMap<String, String>,
}

impl Secrets {
    /// Load `~/.env/.env`. A missing file is allowed; missing required secrets
    /// then produce a clear error when an alias is created.
    pub fn load_default() -> Result<Self, ConnectionConfigError> {
        let home = std::env::var_os("HOME").ok_or(ConnectionConfigError::HomeDirectoryMissing)?;
        let path = PathBuf::from(home).join(".env/.env");
        Self::from_optional_file(path)
    }

    pub fn from_optional_file(path: impl AsRef<Path>) -> Result<Self, ConnectionConfigError> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(content) => Self::parse(&content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConnectionConfigError::ReadEnvFile {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    pub fn parse(content: &str) -> Result<Self, ConnectionConfigError> {
        let mut values = HashMap::new();
        for (index, raw) in content.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConnectionConfigError::InvalidEnvLine { line: index + 1 });
            };
            let key = key.trim();
            if key.is_empty() {
                return Err(ConnectionConfigError::InvalidEnvLine { line: index + 1 });
            }
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                })
                .unwrap_or(value);
            values.insert(key.to_owned(), value.to_owned());
        }
        Ok(Self { values })
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
    pub fn required(&self, name: &str) -> Result<&str, ConnectionConfigError> {
        self.get(name)
            .ok_or_else(|| ConnectionConfigError::MissingSecret {
                name: name.to_owned(),
            })
    }
}

/// Maps one database alias to four secret names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSpec {
    pub driver_name: String,
    pub server_secret: String,
    pub database_secret: String,
    pub user_secret: String,
    pub password_secret: String,
}

impl ConnectionSpec {
    pub fn new(
        driver_name: impl Into<String>,
        server_secret: impl Into<String>,
        database_secret: impl Into<String>,
        user_secret: impl Into<String>,
        password_secret: impl Into<String>,
    ) -> Self {
        Self {
            driver_name: driver_name.into(),
            server_secret: server_secret.into(),
            database_secret: database_secret.into(),
            user_secret: user_secret.into(),
            password_secret: password_secret.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseConnection {
    config: OdbcConfig,
}

impl DatabaseConnection {
    pub fn from_secrets(
        spec: &ConnectionSpec,
        secrets: &Secrets,
    ) -> Result<Self, ConnectionConfigError> {
        Ok(Self {
            config: OdbcConfig {
                driver_name: spec.driver_name.clone(),
                server: secrets.required(&spec.server_secret)?.to_owned(),
                database: secrets.required(&spec.database_secret)?.to_owned(),
                user: secrets.required(&spec.user_secret)?.to_owned(),
                password: secrets.required(&spec.password_secret)?.to_owned(),
            },
        })
    }
    pub fn config(&self) -> &OdbcConfig {
        &self.config
    }

    #[cfg(feature = "odbc")]
    pub fn run_query<P>(
        &self,
        query: &str,
        params: P,
    ) -> Result<polars::prelude::DataFrame, crate::OdbcError>
    where
        P: odbc_api::ParameterCollectionRef,
    {
        crate::run_query(&self.config, query, params)
    }
}

/// Generate a typed project `Connections` struct with named aliases.
///
/// `Connections::load()` loads `~/.env/.env` once, then creates every alias.
#[macro_export]
macro_rules! connections {
    ($visibility:vis struct $name:ident { $($field:ident : $spec:expr),+ $(,)? }) => {
        $visibility struct $name { $(pub $field: $crate::DatabaseConnection,)+ }
        impl $name {
            pub fn load() -> Result<Self, $crate::ConnectionConfigError> {
                let secrets = $crate::Secrets::load_default()?;
                Ok(Self { $($field: $crate::DatabaseConnection::from_secrets(&$spec, &secrets)?,)+ })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_env_and_builds_connection() {
        let secrets = Secrets::parse(
            "DB_HOST=localhost\nDB_NAME='app'\nDB_USER=reader\nDB_PASSWORD=secret\n",
        )
        .unwrap();
        let connection = DatabaseConnection::from_secrets(
            &ConnectionSpec::new("FreeTDS", "DB_HOST", "DB_NAME", "DB_USER", "DB_PASSWORD"),
            &secrets,
        )
        .unwrap();
        assert_eq!(connection.config().server, "localhost");
        assert_eq!(connection.config().database, "app");
    }
}
