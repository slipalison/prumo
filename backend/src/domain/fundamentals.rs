//! Indicadores fundamentalistas das empresas e dos FIIs da carteira, a partir
//! dos dados abertos da CVM. Regra pura: sem banco, sem rede.
//!
//! - [`company`]: os balanços (DFP anual e ITR trimestral) de uma companhia
//!   viram [`CompanyMetrics`]: receita, lucro, dívida, EBITDA aproximado, ROE,
//!   margens, CAGR, anos com lucro, recuperação judicial.
//! - [`fund`]: o informe mensal e o trimestral de um FII viram [`FundMetrics`]:
//!   valor patrimonial por cota, P/VP e vacância física.
//!
//! As contas da CVM, as fórmulas e as limitações estão em `docs/fundamentos.md`.
//!
//! **Nada é inventado.** Toda medida é um [`Figure`]: o número, ou o motivo
//! ([`Gap`]) de não haver número — conta que o plano de contas do banco não
//! tem, patrimônio negativo no denominador, ano sem balanço.

pub mod company;
pub mod fund;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::income_auto::parse_ticker;

pub use company::{
    company_metrics, keeps_line, template_of, Column, CompanyMetrics, Filing, FilingKind,
    FilingRef, FiscalYear, Line, PeriodFigures, Registration, Scope, Statement, Template,
    YearCount, WINDOW_YEARS,
};
pub use fund::{fund_metrics, physical_vacancy, FundMetrics, FundReport, RentalProperty, Vacancy};

/// Casas decimais das razões (ROE, margens, alavancagem, CAGR, P/VP, vacância).
pub const RATIO_DP: u32 = 6;

/// Por que uma medida não tem número.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    /// Banco ou seguradora: o plano de contas da CVM para instituição
    /// financeira não tem a conta (receita líquida, EBIT, dívida bruta...), e a
    /// medida não se aplica.
    FinancialTemplate,
    /// Plano de contas que o Prumo não reconhece: melhor sem número que com um
    /// número de outra conta.
    UnknownTemplate,
    /// A conta não veio na demonstração entregue (ou veio com outro nome).
    NotReported,
    /// Não há balanço do exercício nos arquivos da CVM (ano faltando).
    NoFiling,
    /// O denominador é zero ou negativo: patrimônio líquido negativo, EBITDA
    /// negativo, receita zero.
    NonPositiveBase,
    /// CAGR com valor zero ou negativo numa das pontas (prejuízo): a taxa não
    /// tem sentido.
    NonPositiveEndpoint,
    /// Faltam exercícios na janela para a medida (CAGR precisa das duas pontas).
    InsufficientHistory,
    /// Sem cotação do ativo no Prumo (P/VP).
    NoQuote,
    /// O fundo não declara imóvel para renda acabado no informe trimestral
    /// (fundo de papel, fundo de fundos, fundo em construção).
    NoRentalProperty,
    /// O fundo não aparece no informe trimestral da CVM.
    NoQuarterlyReport,
    /// Sem número de ações (a B3 não informou) ou de cotas (o informe mensal
    /// veio sem cotas emitidas), ou veio zero.
    NoShares,
}

impl Gap {
    pub fn as_str(self) -> &'static str {
        match self {
            Gap::FinancialTemplate => "financial_template",
            Gap::UnknownTemplate => "unknown_template",
            Gap::NotReported => "not_reported",
            Gap::NoFiling => "no_filing",
            Gap::NonPositiveBase => "non_positive_base",
            Gap::NonPositiveEndpoint => "non_positive_endpoint",
            Gap::InsufficientHistory => "insufficient_history",
            Gap::NoQuote => "no_quote",
            Gap::NoRentalProperty => "no_rental_property",
            Gap::NoQuarterlyReport => "no_quarterly_report",
            Gap::NoShares => "no_shares",
        }
    }

    pub const ALL: [Gap; 11] = [
        Gap::FinancialTemplate,
        Gap::UnknownTemplate,
        Gap::NotReported,
        Gap::NoFiling,
        Gap::NonPositiveBase,
        Gap::NonPositiveEndpoint,
        Gap::InsufficientHistory,
        Gap::NoQuote,
        Gap::NoRentalProperty,
        Gap::NoQuarterlyReport,
        Gap::NoShares,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Gap::ALL.into_iter().find(|g| g.as_str() == value)
    }
}

/// Uma medida: o número ou o motivo de não haver. Exatamente um dos dois.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Figure {
    value: Option<Decimal>,
    gap: Option<Gap>,
}

impl Figure {
    pub fn known(value: Decimal) -> Self {
        Self {
            value: Some(value),
            gap: None,
        }
    }

    pub fn missing(gap: Gap) -> Self {
        Self {
            value: None,
            gap: Some(gap),
        }
    }

    /// O número, se houver.
    pub fn value(&self) -> Option<Decimal> {
        self.value
    }

    /// Por que não há número (`None` quando há).
    pub fn gap(&self) -> Option<Gap> {
        self.gap
    }

    /// Junta duas medidas com `f`; a primeira que faltar diz o motivo.
    fn zip(self, other: Figure, f: impl FnOnce(Decimal, Decimal) -> Figure) -> Figure {
        match (self.value, other.value) {
            (Some(a), Some(b)) => f(a, b),
            (None, _) => self,
            (_, None) => other,
        }
    }

    fn add(self, other: Figure) -> Figure {
        self.zip(other, |a, b| Figure::known(a + b))
    }

    fn sub(self, other: Figure) -> Figure {
        self.zip(other, |a, b| Figure::known(a - b))
    }

    /// `self / base`, com o denominador estritamente positivo.
    fn ratio(self, base: Figure) -> Figure {
        self.zip(base, |a, b| {
            if b <= Decimal::ZERO {
                Figure::missing(Gap::NonPositiveBase)
            } else {
                a.checked_div(b)
                    .map_or(Figure::missing(Gap::NonPositiveBase), |r| {
                        Figure::known(r.round_dp(RATIO_DP).normalize())
                    })
            }
        })
    }
}

/// Por que um ativo da carteira não tem indicadores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unsupported {
    /// Fora da B3: ação dos EUA, REIT, cripto, Tesouro, renda fixa.
    NotListedOnB3,
    /// ETF ou BDR negociado na B3 (fora de Ações Nacionais e FIIs): não entrega
    /// DFP nem informe de FII à CVM.
    ExchangeTradedProduct,
    /// A B3 não informa código CVM para o emissor (ETF cadastrado em Ações
    /// Nacionais, empresa estrangeira, código fora do padrão).
    NoCvmCode,
    /// O código CVM existe, mas não há DFP nem ITR nos arquivos lidos.
    NoFilings,
    /// O FII não aparece no informe mensal da CVM (pelo ISIN da cota).
    FundNotFound,
    /// A primeira busca ainda não terminou: a próxima rodada traz.
    Pending,
}

impl Unsupported {
    pub fn as_str(self) -> &'static str {
        match self {
            Unsupported::NotListedOnB3 => "not_listed_on_b3",
            Unsupported::ExchangeTradedProduct => "exchange_traded_product",
            Unsupported::NoCvmCode => "no_cvm_code",
            Unsupported::NoFilings => "no_filings",
            Unsupported::FundNotFound => "fund_not_found",
            Unsupported::Pending => "pending",
        }
    }
}

/// Os indicadores de um ativo da carteira. É o que
/// `Store::fundamentals_for_user` devolve por símbolo.
#[derive(Debug, Clone, PartialEq)]
pub enum AssetFundamentals {
    Company(Box<CompanyMetrics>),
    Fund(Box<FundMetrics>),
    Unsupported(Unsupported),
}

/// Que fonte cobre o ativo: a companhia (pelo emissor, para achar o código CVM
/// na B3), o FII (pelo emissor, a raiz do ISIN da cota) ou nenhuma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    Company { issuer: String },
    Fund { issuer: String },
    Unsupported(Unsupported),
}

/// A cobertura de um ativo pela classe, pelo mercado e pelo símbolo.
pub fn coverage(class: AssetClass, market: Market, symbol: Option<&str>) -> Coverage {
    if market != Market::B3 {
        return Coverage::Unsupported(Unsupported::NotListedOnB3);
    }
    let Some(ticker) = symbol.and_then(parse_ticker) else {
        return Coverage::Unsupported(Unsupported::NoCvmCode);
    };
    match class {
        AssetClass::BrStocks => Coverage::Company {
            issuer: ticker.issuer,
        },
        AssetClass::BrReits => Coverage::Fund {
            issuer: ticker.issuer,
        },
        _ => Coverage::Unsupported(Unsupported::ExchangeTradedProduct),
    }
}

/// Minúsculas e sem acento, para comparar descrição de conta (o texto da CVM
/// muda de caixa e de acento entre empresas: "Atribuído aos Sócios" e
/// "Atribuido a Socios").
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn a_figure_is_a_number_or_a_reason() {
        let n = Figure::known(dec!(10));
        assert_eq!(n.value(), Some(dec!(10)));
        assert_eq!(n.gap(), None);
        let m = Figure::missing(Gap::NotReported);
        assert_eq!(m.value(), None);
        assert_eq!(m.gap(), Some(Gap::NotReported));
    }

    #[test]
    fn the_first_missing_operand_gives_the_reason() {
        let a = Figure::missing(Gap::FinancialTemplate);
        let b = Figure::missing(Gap::NotReported);
        assert_eq!(a.add(b).gap(), Some(Gap::FinancialTemplate));
        assert_eq!(Figure::known(dec!(1)).sub(b).gap(), Some(Gap::NotReported));
        assert_eq!(
            Figure::known(dec!(3)).add(Figure::known(dec!(4))).value(),
            Some(dec!(7))
        );
    }

    #[test]
    fn a_ratio_needs_a_positive_base() {
        let one = Figure::known(dec!(1));
        assert_eq!(
            one.ratio(Figure::known(dec!(3))).value(),
            Some(dec!(0.333333))
        );
        assert_eq!(
            one.ratio(Figure::known(dec!(0))).gap(),
            Some(Gap::NonPositiveBase)
        );
        assert_eq!(
            one.ratio(Figure::known(dec!(-5))).gap(),
            Some(Gap::NonPositiveBase)
        );
        // Numerador negativo é número (prejuízo dá ROE negativo).
        assert_eq!(
            Figure::known(dec!(-1))
                .ratio(Figure::known(dec!(4)))
                .value(),
            Some(dec!(-0.25))
        );
    }

    #[test]
    fn gap_codes_round_trip() {
        for gap in Gap::ALL {
            assert_eq!(Gap::parse(gap.as_str()), Some(gap));
            assert_eq!(
                serde_json::to_value(gap).unwrap(),
                serde_json::Value::String(gap.as_str().to_owned())
            );
        }
        assert_eq!(Gap::parse("outro"), None);
        for reason in [
            Unsupported::NotListedOnB3,
            Unsupported::ExchangeTradedProduct,
            Unsupported::NoCvmCode,
            Unsupported::NoFilings,
            Unsupported::FundNotFound,
            Unsupported::Pending,
        ] {
            assert_eq!(
                serde_json::to_value(reason).unwrap(),
                serde_json::Value::String(reason.as_str().to_owned())
            );
        }
    }

    #[test]
    fn coverage_by_class_market_and_symbol() {
        assert_eq!(
            coverage(AssetClass::BrStocks, Market::B3, Some("WEGE3")),
            Coverage::Company {
                issuer: "WEGE".into()
            }
        );
        // Unit também é companhia.
        assert_eq!(
            coverage(AssetClass::BrStocks, Market::B3, Some("TAEE11")),
            Coverage::Company {
                issuer: "TAEE".into()
            }
        );
        assert_eq!(
            coverage(AssetClass::BrReits, Market::B3, Some("KNRI11")),
            Coverage::Fund {
                issuer: "KNRI".into()
            }
        );
        assert_eq!(
            coverage(AssetClass::Crypto, Market::B3, Some("HASH11")),
            Coverage::Unsupported(Unsupported::ExchangeTradedProduct)
        );
        assert_eq!(
            coverage(AssetClass::IntlStocks, Market::B3, Some("AAPL34")),
            Coverage::Unsupported(Unsupported::ExchangeTradedProduct)
        );
        assert_eq!(
            coverage(AssetClass::IntlStocks, Market::Us, Some("AAPL")),
            Coverage::Unsupported(Unsupported::NotListedOnB3)
        );
        assert_eq!(
            coverage(AssetClass::Crypto, Market::Crypto, Some("BTC")),
            Coverage::Unsupported(Unsupported::NotListedOnB3)
        );
        assert_eq!(
            coverage(AssetClass::FixedIncome, Market::Manual, None),
            Coverage::Unsupported(Unsupported::NotListedOnB3)
        );
        assert_eq!(
            coverage(AssetClass::BrStocks, Market::B3, Some("XPTO")),
            Coverage::Unsupported(Unsupported::NoCvmCode)
        );
    }

    #[test]
    fn fold_drops_case_and_accents() {
        assert_eq!(
            fold("Patrimônio Líquido Atribuído ao Controlador"),
            "patrimonio liquido atribuido ao controlador"
        );
        assert_eq!(fold("DEPRECIAÇÃO"), "depreciacao");
    }
}
