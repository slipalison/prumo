//! Imposto de renda da pessoa física sobre bolsa (mercado à vista da B3) e
//! cripto: isentômetro, apuração mensal, compensação de prejuízo, DARF e o que
//! vai para a declaração anual (IRPF).
//!
//! Regra pura: sem banco, sem rede, sem relógio. Toda alíquota, limite, código
//! e prazo vem da tabela versionada por ano-calendário em [`rules`], cada valor
//! com a fonte oficial ao lado; ano sem regra cadastrada é erro
//! ([`TaxError::NoRulesForYear`]), nunca a regra de outro ano. As regras, as
//! fontes e o que o Prumo **não** calcula estão em `docs/ir.md`.
//!
//! O caminho:
//!
//! 1. [`positions::replay`] percorre os lançamentos (compra, venda,
//!    desdobramento, bonificação), separa o day trade (mesmo dia, mesmo ativo,
//!    mesma instituição) e devolve cada resultado realizado com o custo pelo
//!    preço médio ponderado com taxas (regra da Receita);
//! 2. [`monthly::assess`] junta os resultados por mês: vendas por categoria
//!    contra o limite de isenção, compensação do prejuízo por grupo, imposto,
//!    IR retido, DARF (ou o acúmulo abaixo do mínimo) e vencimento;
//! 3. [`irpf::annual_report`] fecha o ano: posição e custo em 31/12 para Bens e
//!    Direitos, rendimentos isentos e de tributação exclusiva por fonte
//!    pagadora;
//! 4. [`late::late_charge`] calcula a multa e os juros do DARF pago depois do
//!    vencimento, com a Selic de cada mês.

pub mod calendar;
pub mod irpf;
pub mod late;
pub mod monthly;
pub mod positions;
pub mod rules;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use time::{Date, Month};

/// O que o ativo é **para o imposto**. Não é a classe da carteira
/// (`AssetClass`): um ETF de bitcoin da B3 é cripto na carteira e ETF aqui;
/// um BDR é ação internacional na carteira e BDR aqui. Quem liga ao livro
/// decide o instrumento de cada ativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    /// Ação ou unit no mercado à vista da B3.
    Stock,
    /// Cota de fundo de índice (ETF) de renda variável negociada na B3.
    Etf,
    /// Brazilian Depositary Receipt negociado na B3.
    Bdr,
    /// Cota de fundo de investimento imobiliário (FII) negociada na B3.
    RealEstateFund,
    /// Cota de Fiagro negociada na B3.
    AgribusinessFund,
    /// Criptoativo custodiado ou negociado no Brasil (exchange brasileira ou
    /// custódia própria de residente).
    Crypto,
}

impl Instrument {
    pub const ALL: [Instrument; 6] = [
        Instrument::Stock,
        Instrument::Etf,
        Instrument::Bdr,
        Instrument::RealEstateFund,
        Instrument::AgribusinessFund,
        Instrument::Crypto,
    ];
}

/// Um mês do calendário (período de apuração).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct YearMonth {
    pub year: i32,
    /// 1 a 12.
    pub month: u8,
}

impl YearMonth {
    /// O mês da data.
    pub fn of(date: Date) -> Self {
        Self {
            year: date.year(),
            month: u8::from(date.month()),
        }
    }

    /// Janeiro do ano.
    pub fn january(year: i32) -> Self {
        Self { year, month: 1 }
    }

    /// O mês seguinte.
    pub fn next(self) -> Self {
        if self.month >= 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    pub(crate) fn time_month(self) -> Month {
        Month::try_from(self.month).unwrap_or(Month::January)
    }
}

/// Por que a apuração não sai.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaxError {
    #[error("sem regra de IR cadastrada para o ano-calendario {0}")]
    NoRulesForYear(i32),
    #[error("regra de IR do ano {year} incompleta: falta {missing}")]
    IncompleteRules { year: i32, missing: &'static str },
    #[error("sem calendario de dias uteis cadastrado para o ano {0}")]
    NoCalendarForYear(i32),
    #[error("{symbol} aparece com dois instrumentos diferentes")]
    InstrumentMismatch { symbol: String },
    #[error("venda de {symbol} em {date} maior que a posicao")]
    SellExceedsPosition { symbol: String, date: Date },
    #[error("lancamento invalido de {symbol} em {date}: {reason}")]
    InvalidOperation {
        symbol: String,
        date: Date,
        reason: &'static str,
    },
    #[error("intervalo de anos invalido: {first} a {last}")]
    InvalidYearRange { first: i32, last: i32 },
}

/// Arredonda para centavos (meio para longe do zero), como no DARF.
pub(crate) fn cents(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn year_month_walks_across_the_year() {
        let december = YearMonth::of(date!(2025 - 12 - 31));
        assert_eq!(
            december,
            YearMonth {
                year: 2025,
                month: 12
            }
        );
        assert_eq!(december.next(), YearMonth::january(2026));
        assert_eq!(
            YearMonth::january(2026).next(),
            YearMonth {
                year: 2026,
                month: 2
            }
        );
        assert_eq!(december.time_month(), Month::December);
    }

    #[test]
    fn cents_rounds_half_away_from_zero() {
        assert_eq!(cents(Decimal::new(12345, 3)), Decimal::new(1235, 2));
        assert_eq!(cents(Decimal::new(-12345, 3)), Decimal::new(-1235, 2));
        assert_eq!(cents(Decimal::new(12344, 3)), Decimal::new(1234, 2));
    }
}
