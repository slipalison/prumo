//! Cliente da brapi (`brapi.dev`) para o que o Prumo usa dela: o logo de um
//! ativo da B3.
//!
//! São dois passos, com credenciais diferentes:
//!
//! 1. `GET /api/v2/stocks/quote?symbols=<TICKER>` com `Authorization: Bearer
//!    <token>` devolve o `logourl` do ativo (medido em 26/11/2026; a rota v1
//!    `/api/quote/<TICKER>` devolve o mesmo campo, mas a brapi manda usar a v2).
//!    O plano gratuito atende **um ticker por pedido e um pedido por vez**, 15
//!    mil por mês: por isso um símbolo por chamada e uma chamada por vez.
//! 2. `GET <logourl>` (hoje `https://icons.brapi.dev/icons/<TICKER>.svg`, um
//!    arquivo estático) **sem** o token: o host dos logos nem precisa dele, e o
//!    token só viaja para `brapi.dev`.
//!
//! O token é segredo do servidor. Vai no cabeçalho (nunca na URL, que cai em
//! log), o `Debug` o redige, e nenhum erro daqui cita URL, cabeçalho ou corpo da
//! resposta. O `logourl` é dado de fora: só vale se estiver na origem dos logos
//! da brapi (o cliente HTTP do Prumo não segue redirecionamento para o
//! download), para a resposta da API não poder apontar o servidor para outro
//! lugar.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::config::Secret;
use crate::domain::icon::MAX_SVG_BYTES;

pub const API_BASE: &str = "https://brapi.dev";
/// De onde vêm os logos (host estático, sem token).
pub const ICON_ORIGIN: &str = "https://icons.brapi.dev";

const API_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
/// Maior resposta da API aceita (a cotação tem uns 2 kB).
const MAX_API_BODY: usize = 256 * 1024;
/// Quanto esperar quando a brapi manda parar sem dizer por quanto tempo.
const DEFAULT_WAIT: Duration = Duration::from_secs(60);
/// A marca da própria brapi: não é logo de empresa nenhuma.
const OWN_MARK: &str = "BRAPI";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LogoError {
    /// A brapi não conhece o ticker ou não tem logo dele.
    #[error("a brapi nao tem logo para o simbolo")]
    NotFound,
    /// 429: cota do mês ou pedido simultâneo; espere.
    #[error("a brapi pediu para esperar {0:?}")]
    RateLimited(Duration),
    /// 401: token ausente ou inválido.
    #[error("a brapi recusou o token")]
    Unauthorized,
    /// 403: o plano não inclui o recurso.
    #[error("o plano da brapi nao inclui o recurso")]
    Forbidden,
    /// O arquivo do logo não serve (host de fora, tipo errado, grande demais).
    #[error("logo recusado: {0}")]
    Rejected(String),
    #[error("brapi indisponivel: {0}")]
    Unavailable(String),
}

/// O que o download trouxe: a URL e o arquivo como a fonte o mandou (ainda
/// sem sanear).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedLogo {
    pub url: String,
    pub bytes: Vec<u8>,
}

/// Uma fonte de logos por símbolo.
#[async_trait]
pub trait LogoSource: Send + Sync {
    async fn fetch(&self, symbol: &str) -> Result<FetchedLogo, LogoError>;
}

#[derive(Clone)]
pub struct BrapiLogos {
    http: reqwest::Client,
    api_base: String,
    icon_origin: String,
    token: Secret,
}

impl std::fmt::Debug for BrapiLogos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrapiLogos")
            .field("api_base", &self.api_base)
            .field("icon_origin", &self.icon_origin)
            .field("token", &self.token)
            .finish()
    }
}

#[derive(Deserialize)]
struct QuoteResponse {
    #[serde(default)]
    results: Vec<QuoteResult>,
}

#[derive(Deserialize)]
struct QuoteResult {
    data: Option<QuoteData>,
}

#[derive(Deserialize)]
struct QuoteData {
    logourl: Option<String>,
}

impl BrapiLogos {
    /// `http` não deve seguir redirecionamento (ver o módulo).
    pub fn new(http: reqwest::Client, token: Secret) -> Self {
        Self::with_bases(http, token, API_BASE, ICON_ORIGIN)
    }

    /// Com outras origens: os testes apontam para um servidor falso.
    pub fn with_bases(
        http: reqwest::Client,
        token: Secret,
        api_base: &str,
        icon_origin: &str,
    ) -> Self {
        Self {
            http,
            api_base: api_base.trim_end_matches('/').to_owned(),
            icon_origin: icon_origin.trim_end_matches('/').to_owned(),
            token,
        }
    }

    /// O `logourl` do ticker, conferido.
    async fn logo_url(&self, symbol: &str) -> Result<String, LogoError> {
        let response = self
            .http
            .get(format!("{}/api/v2/stocks/quote", self.api_base))
            .query(&[("symbols", symbol)])
            .bearer_auth(self.token.expose())
            .timeout(API_TIMEOUT)
            .send()
            .await
            .map_err(network)?;
        let status = response.status();
        match status.as_u16() {
            200 => {}
            400 | 404 => return Err(LogoError::NotFound),
            401 => return Err(LogoError::Unauthorized),
            403 => return Err(LogoError::Forbidden),
            429 => {
                let wait = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map_or(DEFAULT_WAIT, Duration::from_secs);
                return Err(LogoError::RateLimited(wait));
            }
            other => return Err(LogoError::Unavailable(format!("status {other}"))),
        }
        let body = read_limited(response, MAX_API_BODY).await?;
        let parsed: QuoteResponse = serde_json::from_slice(&body)
            .map_err(|_| LogoError::Unavailable("resposta fora do formato".to_owned()))?;
        let url = parsed
            .results
            .into_iter()
            .find_map(|r| r.data?.logourl)
            .map(|u| u.trim().to_owned())
            .filter(|u| !u.is_empty())
            .ok_or(LogoError::NotFound)?;
        self.check_logo_url(&url)?;
        Ok(url)
    }

    /// O `logourl` precisa estar na origem dos logos e ser um `.svg` direto;
    /// a marca da própria brapi não é logo de ativo.
    fn check_logo_url(&self, url: &str) -> Result<(), LogoError> {
        let rejected = || LogoError::Rejected("logourl fora da origem dos logos".to_owned());
        let path = url
            .strip_prefix(&self.icon_origin)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(rejected)?;
        let stem = path.strip_suffix(".svg").ok_or_else(rejected)?;
        let plain = !stem.is_empty()
            && !stem.contains("..")
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'));
        if !plain {
            return Err(rejected());
        }
        let file = stem.rsplit('/').next().unwrap_or(stem);
        if file.eq_ignore_ascii_case(OWN_MARK) {
            return Err(LogoError::NotFound);
        }
        Ok(())
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>, LogoError> {
        // Sem token: o host dos logos não precisa dele.
        let response = self
            .http
            .get(url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(network)?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(LogoError::NotFound);
        }
        if !status.is_success() {
            return Err(LogoError::Unavailable(format!("logo: status {status}")));
        }
        let svg = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim().to_ascii_lowercase().starts_with("image/svg+xml"));
        if !svg {
            return Err(LogoError::Rejected("o logo nao veio como svg".to_owned()));
        }
        read_limited(response, MAX_SVG_BYTES)
            .await
            .map_err(|e| match e {
                LogoError::Rejected(_) => LogoError::Rejected("logo grande demais".to_owned()),
                other => other,
            })
    }
}

/// O erro de rede sem a URL (a do logo e a da API não vão para log).
fn network(error: reqwest::Error) -> LogoError {
    LogoError::Unavailable(error.without_url().to_string())
}

/// Lê o corpo até `max` bytes; passou disso, recusa sem terminar de baixar.
async fn read_limited(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, LogoError> {
    if response.content_length().is_some_and(|n| n > max as u64) {
        return Err(LogoError::Rejected("corpo grande demais".to_owned()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if body.len() + chunk.len() > max {
            return Err(LogoError::Rejected("corpo grande demais".to_owned()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[async_trait]
impl LogoSource for BrapiLogos {
    async fn fetch(&self, symbol: &str) -> Result<FetchedLogo, LogoError> {
        let url = self.logo_url(symbol).await?;
        let bytes = self.download(&url).await?;
        Ok(FetchedLogo { url, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> BrapiLogos {
        BrapiLogos::new(reqwest::Client::new(), Secret::new("segredo-da-brapi"))
    }

    #[test]
    fn debug_never_shows_the_token() {
        let printed = format!("{:?}", client());
        assert!(!printed.contains("segredo-da-brapi"), "{printed}");
        assert!(printed.contains("Secret(***)"));
    }

    #[test]
    fn only_svg_files_of_the_icon_origin_pass() {
        let brapi = client();
        for ok in [
            "https://icons.brapi.dev/icons/ITSA4.svg",
            "https://icons.brapi.dev/icons/sub/KNRI11.svg",
        ] {
            assert_eq!(brapi.check_logo_url(ok), Ok(()), "{ok}");
        }
        for bad in [
            "http://icons.brapi.dev/icons/ITSA4.svg",
            "https://icons.brapi.dev.exemplo.test/icons/ITSA4.svg",
            "https://icons.brapi.dev@exemplo.test/icons/ITSA4.svg",
            "https://exemplo.test/icons/ITSA4.svg",
            "https://icons.brapi.dev/icons/ITSA4.png",
            "https://icons.brapi.dev/icons/ITSA4.svg?x=1",
            "https://icons.brapi.dev/icons/../ITSA4.svg",
            "https://icons.brapi.dev/.svg",
            "javascript:alert(1)",
            "//exemplo.test/ITSA4.svg",
        ] {
            assert!(
                matches!(brapi.check_logo_url(bad), Err(LogoError::Rejected(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_brand_mark_of_the_provider_is_not_an_asset_logo() {
        assert_eq!(
            client().check_logo_url("https://icons.brapi.dev/icons/BRAPI.svg"),
            Err(LogoError::NotFound)
        );
        assert_eq!(
            client().check_logo_url("https://icons.brapi.dev/icons/brapi.svg"),
            Err(LogoError::NotFound)
        );
    }
}
