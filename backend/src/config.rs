//! Configuração lida do ambiente, uma vez, no arranque.
//!
//! O binário recusa subir com configuração pela metade: sem `DATABASE_URL`,
//! sem `AUTH_MODE`, ou em modo Access sem team e AUD. Falhar no arranque é
//! melhor que subir aceitando requisição que não sabe autenticar.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthConfig {
    /// Cloudflare Access: JWT do team, conferido pelo AUD da aplicação.
    CloudflareAccess {
        team_domain: String,
        audience: String,
    },
    /// Desenvolvimento: toda requisição é esta pessoa.
    Dev { email: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub operational_port: u16,
    pub database_url: String,
    /// Teto de conexões do pool de cada réplica. O papel do app no Postgres
    /// aceita 20; no canário convivem 4 pods (2 estáveis + 2 novos), então o
    /// padrão 4 cabe com folga (16).
    pub database_max_connections: u32,
    pub static_dir: String,
    pub auth: AuthConfig,
    pub quotes_enabled: bool,
    /// Preço ao vivo pelo streamer do Yahoo (com `quotes_enabled`).
    pub quotes_stream: bool,
    pub quotes_refresh: Duration,
    /// Setor e segmento dos papeis, de fontes publicas da B3 e da CVM.
    pub sectors_enabled: bool,
    /// Horário de Brasília da tarefa diária de desempenho (`None` = desligada).
    pub snapshot_at: Option<time::Time>,
    pub version: String,
    pub commit: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("variavel obrigatoria ausente ou vazia: {0}")]
    Missing(&'static str),
    #[error("valor invalido em {name}: {value}")]
    Invalid { name: &'static str, value: String },
}

impl Config {
    /// Lê do ambiente do processo.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Lê de uma função de busca — é o que deixa testar sem mexer no ambiente.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let required = |name: &'static str| get(name).ok_or(ConfigError::Missing(name));
        let number = |name: &'static str, default: u64| -> Result<u64, ConfigError> {
            match get(name) {
                None => Ok(default),
                Some(value) => value
                    .parse()
                    .map_err(|_| ConfigError::Invalid { name, value }),
            }
        };
        let port = |name: &'static str, default: u64| -> Result<u16, ConfigError> {
            let value = number(name, default)?;
            u16::try_from(value).map_err(|_| ConfigError::Invalid {
                name,
                value: value.to_string(),
            })
        };

        let auth = match required("AUTH_MODE")?.as_str() {
            "cloudflare-access" => AuthConfig::CloudflareAccess {
                team_domain: required("CF_ACCESS_TEAM_DOMAIN")?,
                audience: required("CF_ACCESS_AUD")?,
            },
            "dev" => AuthConfig::Dev {
                email: get("DEV_USER_EMAIL").unwrap_or_else(|| "dev@prumo.local".to_owned()),
            },
            other => {
                return Err(ConfigError::Invalid {
                    name: "AUTH_MODE",
                    value: other.to_owned(),
                });
            }
        };

        let flag = |name: &'static str| -> Result<bool, ConfigError> {
            match get(name).as_deref() {
                None | Some("true") => Ok(true),
                Some("false") => Ok(false),
                Some(other) => Err(ConfigError::Invalid {
                    name,
                    value: other.to_owned(),
                }),
            }
        };

        Ok(Self {
            port: port("PORT", 8080)?,
            operational_port: port("OPERATIONAL_PORT", 9090)?,
            database_url: required("DATABASE_URL")?,
            database_max_connections: u32::try_from(number("DATABASE_MAX_CONNECTIONS", 4)?)
                .ok()
                .filter(|n| (1..=100).contains(n))
                .ok_or_else(|| ConfigError::Invalid {
                    name: "DATABASE_MAX_CONNECTIONS",
                    value: get("DATABASE_MAX_CONNECTIONS").unwrap_or_default(),
                })?,
            static_dir: get("STATIC_DIR").unwrap_or_else(|| "static".to_owned()),
            auth,
            quotes_enabled: flag("QUOTES_ENABLED")?,
            quotes_stream: flag("QUOTES_STREAM")?,
            quotes_refresh: Duration::from_secs(number("QUOTES_REFRESH_SECS", 900)?.max(60)),
            sectors_enabled: flag("SECTORS_ENABLED")?,
            snapshot_at: snapshot_time(get("SNAPSHOT_AT"))?,
            version: get("APP_VERSION").unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
            commit: get("GIT_SHA").unwrap_or_else(|| "unknown".to_owned()),
        })
    }
}

/// `SNAPSHOT_AT`: `HH:MM` no horário de Brasília (padrão 19:00, depois do
/// fechamento da B3), ou `off`.
fn snapshot_time(value: Option<String>) -> Result<Option<time::Time>, ConfigError> {
    let invalid = |value: &str| ConfigError::Invalid {
        name: "SNAPSHOT_AT",
        value: value.to_owned(),
    };
    match value.as_deref() {
        None => Ok(Some(time::macros::time!(19:00))),
        Some("off") => Ok(None),
        Some(text) => {
            let (hour, minute) = text.split_once(':').ok_or_else(|| invalid(text))?;
            let hour: u8 = hour.parse().map_err(|_| invalid(text))?;
            let minute: u8 = minute.parse().map_err(|_| invalid(text))?;
            time::Time::from_hms(hour, minute, 0)
                .map(Some)
                .map_err(|_| invalid(text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn access_mode_needs_team_and_audience() {
        let base = [
            ("DATABASE_URL", "postgres://x"),
            ("AUTH_MODE", "cloudflare-access"),
        ];
        assert_eq!(
            Config::from_lookup(lookup(&base)),
            Err(ConfigError::Missing("CF_ACCESS_TEAM_DOMAIN"))
        );

        let mut full = base.to_vec();
        full.push(("CF_ACCESS_TEAM_DOMAIN", "time.cloudflareaccess.com"));
        full.push(("CF_ACCESS_AUD", "  "));
        assert_eq!(
            Config::from_lookup(lookup(&full)),
            Err(ConfigError::Missing("CF_ACCESS_AUD"))
        );

        full.pop();
        full.push(("CF_ACCESS_AUD", "abc"));
        full.push(("GIT_SHA", "a1b2c3d"));
        let config = Config::from_lookup(lookup(&full)).unwrap();
        assert_eq!(
            config.auth,
            AuthConfig::CloudflareAccess {
                team_domain: "time.cloudflareaccess.com".into(),
                audience: "abc".into()
            }
        );
        assert_eq!(config.port, 8080);
        assert_eq!(config.operational_port, 9090);
        assert_eq!(config.static_dir, "static");
        assert!(config.quotes_enabled);
        assert!(config.quotes_stream);
        assert_eq!(config.database_max_connections, 4);
        assert!(config.sectors_enabled);
        assert_eq!(config.quotes_refresh, Duration::from_secs(900));
        assert_eq!(config.snapshot_at, Some(time::macros::time!(19:00)));
        assert_eq!(config.commit, "a1b2c3d");
    }

    #[test]
    fn dev_mode_has_a_default_email() {
        let config = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("AUTH_MODE", "dev"),
            ("PORT", "3000"),
            ("QUOTES_ENABLED", "false"),
            ("QUOTES_STREAM", "false"),
            ("QUOTES_REFRESH_SECS", "5"),
            ("DATABASE_MAX_CONNECTIONS", "8"),
            ("SECTORS_ENABLED", "false"),
        ]))
        .unwrap();
        assert_eq!(
            config.auth,
            AuthConfig::Dev {
                email: "dev@prumo.local".into()
            }
        );
        assert_eq!(config.port, 3000);
        assert!(!config.quotes_enabled);
        assert!(!config.quotes_stream);
        assert_eq!(config.database_max_connections, 8);
        assert!(!config.sectors_enabled);
        assert_eq!(config.quotes_refresh, Duration::from_secs(60));
    }

    #[test]
    fn snapshot_time_is_configurable() {
        let with = |value: &str| {
            Config::from_lookup(lookup(&[
                ("DATABASE_URL", "postgres://x"),
                ("AUTH_MODE", "dev"),
                ("SNAPSHOT_AT", value),
            ]))
        };
        assert_eq!(
            with("18:45").unwrap().snapshot_at,
            Some(time::macros::time!(18:45))
        );
        assert_eq!(with("off").unwrap().snapshot_at, None);
        for bad in ["19", "25:00", "19:60", "sete:00", "19:xx"] {
            assert!(matches!(
                with(bad),
                Err(ConfigError::Invalid {
                    name: "SNAPSHOT_AT",
                    ..
                })
            ));
        }
    }

    #[test]
    fn refuses_half_configuration() {
        assert_eq!(
            Config::from_lookup(lookup(&[])),
            Err(ConfigError::Missing("AUTH_MODE"))
        );
        assert_eq!(
            Config::from_lookup(lookup(&[("AUTH_MODE", "dev")])),
            Err(ConfigError::Missing("DATABASE_URL"))
        );
        assert!(matches!(
            Config::from_lookup(lookup(&[("AUTH_MODE", "open"), ("DATABASE_URL", "x")])),
            Err(ConfigError::Invalid {
                name: "AUTH_MODE",
                ..
            })
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[
                ("AUTH_MODE", "dev"),
                ("DATABASE_URL", "x"),
                ("PORT", "99999")
            ])),
            Err(ConfigError::Invalid { name: "PORT", .. })
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[
                ("AUTH_MODE", "dev"),
                ("DATABASE_URL", "x"),
                ("QUOTES_ENABLED", "talvez")
            ])),
            Err(ConfigError::Invalid {
                name: "QUOTES_ENABLED",
                ..
            })
        ));
        assert!(matches!(
            Config::from_lookup(lookup(&[
                ("AUTH_MODE", "dev"),
                ("DATABASE_URL", "x"),
                ("QUOTES_STREAM", "sim")
            ])),
            Err(ConfigError::Invalid {
                name: "QUOTES_STREAM",
                ..
            })
        ));
        for bad in ["0", "101", "muitas"] {
            assert!(matches!(
                Config::from_lookup(lookup(&[
                    ("AUTH_MODE", "dev"),
                    ("DATABASE_URL", "x"),
                    ("DATABASE_MAX_CONNECTIONS", bad)
                ])),
                Err(ConfigError::Invalid {
                    name: "DATABASE_MAX_CONNECTIONS",
                    ..
                })
            ));
        }
        assert!(matches!(
            Config::from_lookup(lookup(&[
                ("AUTH_MODE", "dev"),
                ("DATABASE_URL", "x"),
                ("QUOTES_REFRESH_SECS", "x")
            ])),
            Err(ConfigError::Invalid {
                name: "QUOTES_REFRESH_SECS",
                ..
            })
        ));
    }
}
