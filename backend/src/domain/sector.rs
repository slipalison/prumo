//! Setor e segmento de um papel: de onde vêm e como achar a linha certa da
//! tabela de referência a partir do símbolo.
//!
//! Só duas classes têm fonte pública e gratuita que o Prumo usa: Ações
//! Nacionais (Classificação Setorial da B3) e Fundos Imobiliários (informe
//! mensal da CVM). O resto fica "sem setor" — e isso é dito, não escondido.

use serde::Serialize;

use super::asset_class::AssetClass;

/// De onde veio a classificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectorSource {
    /// Planilha de Classificação Setorial da B3 (ações).
    B3Stocks,
    /// Informe mensal de FII da CVM (dados abertos).
    CvmReits,
}

impl SectorSource {
    pub const ALL: [SectorSource; 2] = [SectorSource::B3Stocks, SectorSource::CvmReits];

    pub fn as_str(self) -> &'static str {
        match self {
            SectorSource::B3Stocks => "b3_stocks",
            SectorSource::CvmReits => "cvm_reits",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        SectorSource::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// A classe que a fonte classifica.
    pub fn class(self) -> AssetClass {
        match self {
            SectorSource::B3Stocks => AssetClass::BrStocks,
            SectorSource::CvmReits => AssetClass::BrReits,
        }
    }
}

/// O que a referência diz de um papel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorInfo {
    pub sector: String,
    pub subsector: Option<String>,
    pub segment: Option<String>,
}

/// A fonte e o código de quatro caracteres do emissor a consultar para um
/// ativo. `WEGE3` → (B3, `WEGE`); `KNRI11` → (CVM, `KNRI`); `PETR4F` (mercado
/// fracionário) → (B3, `PETR`). `None` para o que não tem fonte (classe sem
/// ela ou símbolo que não é de ticker da B3).
pub fn reference_key(class: AssetClass, symbol: &str) -> Option<(SectorSource, String)> {
    let source = SectorSource::ALL.into_iter().find(|s| s.class() == class)?;
    let upper = symbol.trim().to_uppercase();
    if !upper.is_ascii() || upper.len() < 5 {
        return None;
    }
    let (root, tail) = upper.split_at(4);
    let digits = tail.strip_suffix('F').unwrap_or(tail);
    let root_ok = root.chars().all(|c| c.is_ascii_alphanumeric())
        && root.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    let tail_ok = (1..=2).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit());
    (root_ok && tail_ok).then(|| (source, root.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_round_trip_and_know_their_class() {
        for source in SectorSource::ALL {
            assert_eq!(SectorSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(SectorSource::parse("sec"), None);
        assert_eq!(SectorSource::B3Stocks.class(), AssetClass::BrStocks);
        assert_eq!(SectorSource::CvmReits.class(), AssetClass::BrReits);
    }

    #[test]
    fn the_key_is_the_issuer_code() {
        let key = |class, symbol| reference_key(class, symbol);
        assert_eq!(
            key(AssetClass::BrStocks, "WEGE3"),
            Some((SectorSource::B3Stocks, "WEGE".into()))
        );
        assert_eq!(
            key(AssetClass::BrStocks, "petr4f"),
            Some((SectorSource::B3Stocks, "PETR".into()))
        );
        // O código do emissor pode ter dígito: B3SA3, AZUL4.
        assert_eq!(
            key(AssetClass::BrStocks, "B3SA3"),
            Some((SectorSource::B3Stocks, "B3SA".into()))
        );
        assert_eq!(
            key(AssetClass::BrReits, "KNRI11"),
            Some((SectorSource::CvmReits, "KNRI".into()))
        );
    }

    #[test]
    fn classes_without_a_source_and_odd_symbols_have_no_key() {
        assert_eq!(reference_key(AssetClass::IntlStocks, "AAPL"), None);
        assert_eq!(reference_key(AssetClass::UsReits, "O"), None);
        assert_eq!(reference_key(AssetClass::Crypto, "BTC"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGE"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGE333"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGEX"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "1EGE3"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "AÇÃO3"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, ""), None);
    }
}
