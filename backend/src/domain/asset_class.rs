//! As sete classes de ativo e o que cada uma decide: checklist, unidade e
//! símbolo no provedor de cotações.

use serde::{Deserialize, Serialize};

/// Classe de ativo. A ordem de [`AssetClass::ALL`] é a ordem da interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    BrStocks,
    BrReits,
    IntlStocks,
    UsReits,
    Crypto,
    FixedIncome,
    IntlFixedIncome,
}

/// Qual checklist dá a nota dos ativos de uma classe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checklist {
    Stocks,
    RealEstate,
}

/// Como a classe é comprada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    /// Cota ou ação inteira.
    Whole,
    /// Fração com até 8 casas (cripto).
    Fractional,
    /// Não há unidade: a pessoa informa o valor (renda fixa).
    Value,
}

impl AssetClass {
    pub const ALL: [AssetClass; 7] = [
        AssetClass::BrStocks,
        AssetClass::BrReits,
        AssetClass::IntlStocks,
        AssetClass::UsReits,
        AssetClass::Crypto,
        AssetClass::FixedIncome,
        AssetClass::IntlFixedIncome,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AssetClass::BrStocks => "br_stocks",
            AssetClass::BrReits => "br_reits",
            AssetClass::IntlStocks => "intl_stocks",
            AssetClass::UsReits => "us_reits",
            AssetClass::Crypto => "crypto",
            AssetClass::FixedIncome => "fixed_income",
            AssetClass::IntlFixedIncome => "intl_fixed_income",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        AssetClass::ALL.into_iter().find(|c| c.as_str() == value)
    }

    /// Rótulo que a pessoa lê (sem acento: atravessa log e JSON de erro).
    pub fn label(self) -> &'static str {
        match self {
            AssetClass::BrStocks => "Acoes Nacionais",
            AssetClass::BrReits => "Fundos Imobiliarios",
            AssetClass::IntlStocks => "Acoes Internacionais",
            AssetClass::UsReits => "REITs",
            AssetClass::Crypto => "Criptomoedas",
            AssetClass::FixedIncome => "Renda Fixa",
            AssetClass::IntlFixedIncome => "Renda Fixa Internacional",
        }
    }

    pub fn checklist(self) -> Option<Checklist> {
        match self {
            AssetClass::BrStocks | AssetClass::IntlStocks => Some(Checklist::Stocks),
            AssetClass::BrReits | AssetClass::UsReits => Some(Checklist::RealEstate),
            AssetClass::Crypto | AssetClass::FixedIncome | AssetClass::IntlFixedIncome => None,
        }
    }

    pub fn unit_kind(self) -> UnitKind {
        match self {
            AssetClass::Crypto => UnitKind::Fractional,
            AssetClass::FixedIncome | AssetClass::IntlFixedIncome => UnitKind::Value,
            _ => UnitKind::Whole,
        }
    }

    /// Símbolo no provedor de cotações. `None` para classes por valor.
    pub fn quote_symbol(self, symbol: &str) -> Option<String> {
        match self {
            AssetClass::BrStocks | AssetClass::BrReits => Some(format!("{symbol}.SA")),
            AssetClass::IntlStocks | AssetClass::UsReits => Some(symbol.to_owned()),
            // O Yahoo nao cota cripto em reais (`BTC-BRL` responde 404, medido em
            // 25/11/2026): vem em dolar e o cambio `USDBRL=X` converte.
            AssetClass::Crypto => Some(format!("{symbol}-USD")),
            AssetClass::FixedIncome | AssetClass::IntlFixedIncome => None,
        }
    }
}

impl Checklist {
    pub const ALL: [Checklist; 2] = [Checklist::Stocks, Checklist::RealEstate];

    pub fn as_str(self) -> &'static str {
        match self {
            Checklist::Stocks => "stocks",
            Checklist::RealEstate => "real_estate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Checklist::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

/// Normaliza o que a pessoa digitou: maiúsculas, sem espaços e sem o sufixo
/// do provedor (`knri11.sa` → `KNRI11`, `btc-brl` → `BTC`). Recusa o que não
/// parece símbolo.
pub fn normalize_symbol(raw: &str) -> Option<String> {
    let upper = raw.trim().to_uppercase();
    let base = upper
        .strip_suffix(".SA")
        .or_else(|| upper.strip_suffix("-BRL"))
        .or_else(|| upper.strip_suffix("-USD"))
        .unwrap_or(&upper);
    let valid = !base.is_empty()
        && base.len() <= 20
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valid.then(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_round_trips_through_its_wire_name() {
        for class in AssetClass::ALL {
            assert_eq!(AssetClass::parse(class.as_str()), Some(class));
            let json = serde_json::to_string(&class).unwrap();
            assert_eq!(json, format!("\"{}\"", class.as_str()));
            assert!(!class.label().is_empty());
        }
        assert_eq!(AssetClass::parse("acoes"), None);
    }

    #[test]
    fn checklists_round_trip() {
        for checklist in Checklist::ALL {
            assert_eq!(Checklist::parse(checklist.as_str()), Some(checklist));
        }
        assert_eq!(Checklist::parse("crypto"), None);
    }

    #[test]
    fn classes_pick_the_right_checklist_and_unit() {
        assert_eq!(AssetClass::BrReits.checklist(), Some(Checklist::RealEstate));
        assert_eq!(AssetClass::UsReits.checklist(), Some(Checklist::RealEstate));
        assert_eq!(AssetClass::IntlStocks.checklist(), Some(Checklist::Stocks));
        assert_eq!(AssetClass::Crypto.checklist(), None);
        assert_eq!(AssetClass::BrStocks.unit_kind(), UnitKind::Whole);
        assert_eq!(AssetClass::Crypto.unit_kind(), UnitKind::Fractional);
        assert_eq!(AssetClass::FixedIncome.unit_kind(), UnitKind::Value);
    }

    #[test]
    fn quote_symbols_follow_the_provider_convention() {
        assert_eq!(
            AssetClass::BrReits.quote_symbol("KNRI11").as_deref(),
            Some("KNRI11.SA")
        );
        assert_eq!(
            AssetClass::Crypto.quote_symbol("BTC").as_deref(),
            Some("BTC-USD")
        );
        assert_eq!(AssetClass::UsReits.quote_symbol("O").as_deref(), Some("O"));
        assert_eq!(AssetClass::FixedIncome.quote_symbol("X"), None);
    }

    #[test]
    fn symbols_are_normalized() {
        assert_eq!(normalize_symbol(" knri11.sa ").as_deref(), Some("KNRI11"));
        assert_eq!(normalize_symbol("btc-brl").as_deref(), Some("BTC"));
        assert_eq!(normalize_symbol("eth-usd").as_deref(), Some("ETH"));
        assert_eq!(normalize_symbol("BRK.B").as_deref(), Some("BRK.B"));
        assert_eq!(normalize_symbol(""), None);
        assert_eq!(normalize_symbol("KN RI"), None);
        assert_eq!(normalize_symbol(&"A".repeat(21)), None);
    }
}
