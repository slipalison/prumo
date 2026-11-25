//! Séries históricas de fontes públicas: os índices de referência (CDI, Selic
//! e IPCA do Banco Central; Ibovespa do Yahoo) e o fechamento diário dos ativos
//! (Yahoo `chart`).
//!
//! Fica atrás de [`HistoryProvider`], separado do [`crate::quotes::QuoteProvider`]
//! (que cota o preço de agora): os testes trocam a rede por um falso, e a troca
//! de fonte não toca o resto.

pub mod bcb;
pub mod yahoo;

use async_trait::async_trait;
use rust_decimal::Decimal;
use time::Date;

/// Os índices que o Prumo guarda em `index_history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexCode {
    /// CDI diário, em % ao dia (BCB/SGS 12).
    Cdi,
    /// Selic diária, em % ao dia (BCB/SGS 11).
    Selic,
    /// IPCA mensal, em % no mês, datado no dia 1º (BCB/SGS 433).
    Ipca,
    /// Ibovespa, fechamento em pontos (Yahoo `^BVSP`).
    Ibov,
}

impl IndexCode {
    pub const ALL: [IndexCode; 4] = [
        IndexCode::Cdi,
        IndexCode::Selic,
        IndexCode::Ipca,
        IndexCode::Ibov,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IndexCode::Cdi => "cdi",
            IndexCode::Selic => "selic",
            IndexCode::Ipca => "ipca",
            IndexCode::Ibov => "ibov",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        IndexCode::ALL.into_iter().find(|c| c.as_str() == value)
    }

    /// Rótulo que a pessoa lê.
    pub fn label(self) -> &'static str {
        match self {
            IndexCode::Cdi => "CDI",
            IndexCode::Selic => "Selic",
            IndexCode::Ipca => "IPCA",
            IndexCode::Ibov => "Ibovespa",
        }
    }

    /// Código da série no SGS do Banco Central (`None` = vem do Yahoo).
    pub fn sgs(self) -> Option<u32> {
        match self {
            IndexCode::Cdi => Some(12),
            IndexCode::Selic => Some(11),
            IndexCode::Ipca => Some(433),
            IndexCode::Ibov => None,
        }
    }

    /// O IPCA sai uma vez por mês; os outros, por dia útil.
    pub fn monthly(self) -> bool {
        self == IndexCode::Ipca
    }
}

/// Símbolo do Ibovespa no Yahoo.
pub const IBOV_SYMBOL: &str = "^BVSP";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HistoryError {
    #[error("serie sem dado no provedor")]
    NotFound,
    #[error("provedor indisponivel: {0}")]
    Unavailable(String),
}

/// Um valor por data, em ordem crescente e sem data repetida.
pub type Series = Vec<(Date, Decimal)>;

#[async_trait]
pub trait HistoryProvider: Send + Sync {
    /// O índice de `from` a `to`, inclusive. Janela sem dado publicado é
    /// lista vazia, não erro.
    async fn index(&self, code: IndexCode, from: Date, to: Date) -> Result<Series, HistoryError>;

    /// Fechamento diário de um símbolo do Yahoo (`KNRI11.SA`, `AAPL`,
    /// `USDBRL=X`), de `from` a `to`, inclusive.
    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError>;
}

/// As fontes públicas de verdade: o SGS do Banco Central e o `chart` do Yahoo.
pub struct PublicHistory {
    bcb: bcb::BcbClient,
    yahoo: yahoo::YahooHistory,
}

impl PublicHistory {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            bcb: bcb::BcbClient::new(http.clone()),
            yahoo: yahoo::YahooHistory::new(http),
        }
    }

    pub fn with_bases(http: reqwest::Client, bcb_base: &str, yahoo_base: &str) -> Self {
        Self {
            bcb: bcb::BcbClient::with_base(http.clone(), bcb_base),
            yahoo: yahoo::YahooHistory::with_base(http, yahoo_base),
        }
    }
}

#[async_trait]
impl HistoryProvider for PublicHistory {
    async fn index(&self, code: IndexCode, from: Date, to: Date) -> Result<Series, HistoryError> {
        match code.sgs() {
            Some(series) => self.bcb.series(series, from, to).await,
            None => self.yahoo.closes(IBOV_SYMBOL, from, to).await,
        }
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        self.yahoo.closes(symbol, from, to).await
    }
}

/// Provedor que não busca nada — com `QUOTES_ENABLED=false` (sem rede). A
/// série da carteira continua; só os índices e os fechamentos ficam de fora.
pub struct DisabledHistory;

#[async_trait]
impl HistoryProvider for DisabledHistory {
    async fn index(&self, _: IndexCode, _: Date, _: Date) -> Result<Series, HistoryError> {
        Err(HistoryError::Unavailable("series desligadas".into()))
    }

    async fn closes(&self, _: &str, _: Date, _: Date) -> Result<Series, HistoryError> {
        Err(HistoryError::Unavailable("series desligadas".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn codes_round_trip_and_know_their_source() {
        for code in IndexCode::ALL {
            assert_eq!(IndexCode::parse(code.as_str()), Some(code));
            assert!(!code.label().is_empty());
        }
        assert_eq!(IndexCode::parse("ifix"), None);
        assert_eq!(IndexCode::Cdi.sgs(), Some(12));
        assert_eq!(IndexCode::Selic.sgs(), Some(11));
        assert_eq!(IndexCode::Ipca.sgs(), Some(433));
        assert_eq!(IndexCode::Ibov.sgs(), None);
        assert!(IndexCode::Ipca.monthly());
        assert!(!IndexCode::Cdi.monthly());
    }

    #[tokio::test]
    async fn disabled_history_fetches_nothing() {
        let d = date!(2026 - 11 - 25);
        assert!(DisabledHistory.index(IndexCode::Cdi, d, d).await.is_err());
        assert!(DisabledHistory.closes("KNRI11.SA", d, d).await.is_err());
    }
}
