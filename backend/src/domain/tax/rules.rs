//! A tabela de regras do IR, versionada por ano-calendário.
//!
//! Cada valor vem com a fonte oficial ([`Sourced`]): lei no planalto.gov.br,
//! instrução normativa da Receita ou pergunta do "Perguntas e Respostas IRPF".
//! Ano novo entra como uma função nova na [`table`], revisada contra a
//! legislação daquele ano — nunca copiando o ano anterior em silêncio. Pedir a
//! regra de um ano que não está na tabela é [`TaxError::NoRulesForYear`].
//!
//! Consulta das fontes: 25/11/2026. Detalhe e links em `docs/ir.md`.

use rust_decimal::Decimal;
use serde::Serialize;
use time::{macros::date, Date};

use super::{Instrument, TaxError};

/// Um valor da regra e de onde ele vem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sourced<T> {
    pub value: T,
    pub source: &'static str,
}

const fn sourced<T>(value: T, source: &'static str) -> Sourced<T> {
    Sourced { value, source }
}

/// Como o ganho é apurado e pago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    /// Ganho líquido mensal em bolsa (DARF 6015), com compensação de perdas.
    ExchangeNetGain,
    /// Ganho de capital por alienação (DARF 4600), sem compensação de perdas.
    CapitalGain,
}

/// Grupo de compensação de prejuízo do ganho líquido em bolsa. Prejuízo de um
/// grupo só abate lucro do mesmo grupo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    /// Operações comuns: ações, ETF e BDR no mercado à vista.
    Common,
    /// Day trade de ações, ETF e BDR.
    DayTrade,
    /// Cotas de FII (comum e day trade).
    RealEstateFund,
    /// Cotas de Fiagro (comum e day trade).
    AgribusinessFund,
}

impl Pool {
    pub const ALL: [Pool; 4] = [
        Pool::Common,
        Pool::DayTrade,
        Pool::RealEstateFund,
        Pool::AgribusinessFund,
    ];
}

/// Qual isenção mensal por volume de vendas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExemptionId {
    /// Vendas de ações no mercado à vista até o limite no mês.
    StockSales,
    /// Alienações de criptoativos até o limite no mês.
    CryptoSales,
}

/// Grupo e código da ficha Bens e Direitos (ou de uma linha de rendimentos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IrpfCode {
    pub group: &'static str,
    pub code: &'static str,
    pub label: &'static str,
}

/// O que a regra diz de um instrumento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstrumentRule {
    pub instrument: Instrument,
    pub regime: Regime,
    /// Grupo de compensação da operação comum (só no ganho líquido).
    pub common_pool: Option<Pool>,
    /// Grupo do day trade; `None` quando day trade não se aplica (cripto).
    pub day_trade_pool: Option<Pool>,
    /// Isenção mensal que cobre o ganho da operação comum, se houver.
    pub exemption: Option<ExemptionId>,
    pub sources: &'static str,
    /// Onde a posição vai na ficha Bens e Direitos.
    pub asset_code: Sourced<IrpfCode>,
}

/// Alíquota de um grupo de compensação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolRule {
    pub pool: Pool,
    pub rate: Sourced<Decimal>,
}

/// Limite de isenção por total de vendas no mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExemptionRule {
    pub id: ExemptionId,
    /// O mês é isento quando o total de vendas é **igual ou menor** que isto.
    pub limit: Sourced<Decimal>,
    /// Instrumentos cujas vendas somam no total.
    pub counted: &'static [Instrument],
    /// Se as vendas em day trade somam no total (o ganho de day trade nunca é
    /// isento).
    pub counts_day_trade: Sourced<bool>,
}

/// Faixa da tabela progressiva do ganho de capital.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bracket {
    /// Teto da faixa; `None` na última.
    pub up_to: Option<Decimal>,
    pub rate: Decimal,
}

/// Retenção na fonte sobre dividendos pagos por uma mesma empresa no mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DividendWithholding {
    /// A retenção vale quando o total pago no mês **passa** deste valor.
    pub monthly_threshold: Decimal,
    /// Alíquota sobre o total pago (não só o excedente).
    pub rate: Decimal,
    pub source: &'static str,
}

/// IR retido na fonte em bolsa, como a corretora deve reter. O Prumo usa isto
/// quando a pessoa não informa o retido do mês (da nota de corretagem).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WithholdingRule {
    /// Sobre o valor das alienações em operação comum no mercado à vista
    /// (0,005%), de ações, ETF, BDR, FII e Fiagro.
    pub common_rate: Sourced<Decimal>,
    /// A retenção do mês (somadas as operações) igual ou abaixo disto é
    /// dispensada.
    pub common_dispensed_up_to: Sourced<Decimal>,
    /// Sobre o resultado positivo do day trade de cada dia, com as perdas do
    /// mesmo dia compensadas (1%).
    pub day_trade_rate: Sourced<Decimal>,
}

/// Acréscimos do DARF pago depois do vencimento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LatePaymentRule {
    /// Multa de mora por dia de atraso (0,33%), contada do dia seguinte ao
    /// vencimento até o dia do pagamento.
    pub daily_penalty: Sourced<Decimal>,
    /// Teto da multa de mora (20%).
    pub penalty_cap: Sourced<Decimal>,
    /// Juros do mês do pagamento (1%), somados à Selic dos meses entre o
    /// vencimento e o pagamento.
    pub payment_month_interest: Sourced<Decimal>,
    /// A série da Selic do mês (de onde vem cada taxa somada).
    pub selic_series: Sourced<&'static str>,
}

/// Linhas das fichas de rendimentos da declaração.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeCodes {
    pub dividends: Sourced<IrpfCode>,
    pub fund_income: Sourced<IrpfCode>,
    pub exempt_stock_gains: Sourced<IrpfCode>,
    pub exempt_crypto_gains: Sourced<IrpfCode>,
    pub interest_on_equity: Sourced<IrpfCode>,
}

/// Códigos de Bens e Direitos de cripto pelo tipo do criptoativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CryptoCodes {
    pub bitcoin: IrpfCode,
    pub altcoin: IrpfCode,
    pub stablecoin: IrpfCode,
    /// Símbolos tratados como Bitcoin (código 01).
    pub bitcoin_symbols: &'static [&'static str],
    /// Símbolos tratados como stablecoin (código 03): os exemplos da P&R 473.
    pub stablecoin_symbols: &'static [&'static str],
    pub source: &'static str,
}

/// Todas as regras de um ano-calendário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YearRules {
    pub year: i32,
    /// Quando as fontes desta regra foram conferidas.
    pub reviewed_on: Date,
    pub instruments: Vec<InstrumentRule>,
    pub pools: Vec<PoolRule>,
    pub exemptions: Vec<ExemptionRule>,
    pub capital_gain_brackets: Sourced<Vec<Bracket>>,
    pub exchange_darf_code: Sourced<&'static str>,
    pub capital_gain_darf_code: Sourced<&'static str>,
    /// DARF abaixo disto não é pago: soma ao do mesmo código no mês seguinte.
    pub darf_minimum: Sourced<Decimal>,
    /// O IR retido sobra para o mês seguinte só dentro do ano-calendário.
    pub withholding_carry: Sourced<bool>,
    pub interest_on_equity_rate: Sourced<Decimal>,
    pub dividend_withholding: Option<DividendWithholding>,
    pub income_codes: IncomeCodes,
    pub crypto_codes: CryptoCodes,
    /// O IR retido em bolsa pela regra (0,005% e 1% do day trade).
    pub withholding: WithholdingRule,
    /// Multa e juros do DARF em atraso.
    pub late_payment: LatePaymentRule,
}

impl YearRules {
    pub fn instrument(&self, instrument: Instrument) -> Result<&InstrumentRule, TaxError> {
        self.instruments
            .iter()
            .find(|rule| rule.instrument == instrument)
            .ok_or(TaxError::IncompleteRules {
                year: self.year,
                missing: "instrumento",
            })
    }

    pub fn pool(&self, pool: Pool) -> Result<&PoolRule, TaxError> {
        self.pools
            .iter()
            .find(|rule| rule.pool == pool)
            .ok_or(TaxError::IncompleteRules {
                year: self.year,
                missing: "grupo de compensacao",
            })
    }

    pub fn exemption(&self, id: ExemptionId) -> Result<&ExemptionRule, TaxError> {
        self.exemptions
            .iter()
            .find(|rule| rule.id == id)
            .ok_or(TaxError::IncompleteRules {
                year: self.year,
                missing: "isencao",
            })
    }

    pub fn darf_code(&self, regime: Regime) -> &'static str {
        match regime {
            Regime::ExchangeNetGain => self.exchange_darf_code.value,
            Regime::CapitalGain => self.capital_gain_darf_code.value,
        }
    }

    /// Imposto do ganho de capital pela tabela progressiva (cada faixa sobre a
    /// parcela do ganho dentro dela).
    pub fn capital_gain_tax(&self, gain: Decimal) -> Decimal {
        if gain <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let mut tax = Decimal::ZERO;
        let mut floor = Decimal::ZERO;
        for bracket in &self.capital_gain_brackets.value {
            let ceiling = bracket.up_to.map_or(gain, |up_to| up_to.min(gain));
            if ceiling > floor {
                tax += (ceiling - floor) * bracket.rate;
            }
            match bracket.up_to {
                Some(up_to) if up_to < gain => floor = up_to,
                _ => break,
            }
        }
        tax
    }

    /// Código de Bens e Direitos de um criptoativo pelo símbolo (`BTC`,
    /// `BTC-USD`, `usdt`...).
    pub fn crypto_asset_code(&self, symbol: &str) -> IrpfCode {
        let base = symbol
            .split(['-', '/'])
            .next()
            .unwrap_or(symbol)
            .trim()
            .to_uppercase();
        let codes = &self.crypto_codes;
        if codes.bitcoin_symbols.contains(&base.as_str()) {
            codes.bitcoin
        } else if codes.stablecoin_symbols.contains(&base.as_str()) {
            codes.stablecoin
        } else {
            codes.altcoin
        }
    }
}

/// As regras do ano-calendário, ou erro se o ano não foi revisado.
pub fn rules_for(year: i32) -> Result<YearRules, TaxError> {
    table()
        .into_iter()
        .find(|rules| rules.year == year)
        .ok_or(TaxError::NoRulesForYear(year))
}

/// Os anos com regra cadastrada.
pub fn covered_years() -> Vec<i32> {
    table().into_iter().map(|rules| rules.year).collect()
}

/// A tabela. Um ano por função, cada uma revisada contra a lei daquele ano.
pub fn table() -> Vec<YearRules> {
    vec![year_2025(), year_2026()]
}

// --- Fontes ---------------------------------------------------------------
// P&R = "Perguntas e Respostas IRPF 2026" (Receita Federal, v1.00 de
// 23/04/2026, ano-calendário 2025).

const SRC_COMMON_RATE: &str = "Lei 11.033/2004, art. 2, II; IN RFB 1.585/2015, art. 57; P&R 706";
const SRC_DAY_TRADE_RATE: &str =
    "Lei 11.033/2004, art. 2, I; IN RFB 1.585/2015, art. 65, par. 10 e 11; P&R 706 e 709";
const SRC_FII_RATE: &str = "Lei 8.668/1993, art. 18 (red. Lei 9.779/1999); IN RFB 1.585/2015, art. 37, caput e par. 2 (perda so compensa com fundo da mesma especie)";
const SRC_FIAGRO_RATE: &str = "Lei 8.668/1993, art. 20-D (incl. Lei 14.130/2021); IN RFB 1.585/2015, art. 37, par. 2 (perda so compensa com fundo da mesma especie)";
const SRC_STOCK_EXEMPTION: &str =
    "Lei 11.033/2004, art. 3, I; IN RFB 1.585/2015, art. 59, I e par. 2; P&R 707";
const SRC_STOCK_EXEMPTION_DAY_TRADE: &str = "IN RFB 1.585/2015, art. 59, I (total das alienacoes de acoes no mercado a vista) e art. 65, par. 15 (o ganho de day trade nao e isento); leitura conservadora: day trade soma no total";
const SRC_CRYPTO_EXEMPTION: &str =
    "Lei 9.250/1995, art. 22, II (red. Lei 11.196/2005); P&R 653, item 2";
const SRC_CRYPTO_EXEMPTION_DAY_TRADE: &str =
    "P&R 653, item 2: conta o conjunto de criptoativos alienados no mes";
const SRC_BRACKETS: &str = "Lei 8.981/1995, art. 21 (red. Lei 13.259/2016); P&R 574";
const SRC_DARF_6015: &str = "IN RFB 1.585/2015, art. 56, par. 5; P&R 730";
const SRC_DARF_4600: &str = "P&R 653, item 2";
const SRC_DARF_MINIMUM: &str = "Lei 9.430/1996, art. 68, caput e par. 1";
const SRC_WITHHOLDING_CARRY: &str =
    "Lei 11.033/2004, art. 2, par. 7; IN RFB 1.585/2015, art. 65, par. 8 e 9; P&R 714 e 715";
const SRC_JCP_2025: &str = "Lei 9.249/1995, art. 9, par. 2 (redacao ate 2025); P&R 320";
const SRC_JCP_2026: &str = "Lei 9.249/1995, art. 9, par. 2 (red. LC 224/2025, art. 8; efeitos desde 1/1/2026 pelo art. 14, III)";
const SRC_DIVIDENDS_2026: &str = "Lei 9.250/1995, art. 6-A (incl. Lei 15.270/2025, efeitos desde 1/1/2026 pelo art. 8); P&R 276 (o total do mes da mesma empresa)";
const SRC_WITHHOLDING_COMMON: &str =
    "Lei 11.033/2004, art. 2, par. 1, IV; IN RFB 1.585/2015, art. 63, IV; P&R 706";
const SRC_WITHHOLDING_DISPENSED: &str = "Lei 11.033/2004, art. 2, par. 4 e 5; IN RFB 1.585/2015, art. 63, par. 4 e 5 (soma das operacoes do mes)";
const SRC_WITHHOLDING_DAY_TRADE: &str = "Lei 9.959/2000, art. 8; IN RFB 1.585/2015, art. 65, caput, par. 1, II, e par. 4 (resultado positivo do dia, perdas do mesmo dia compensadas); P&R 705 e 712";
const SRC_LATE_PENALTY: &str = "Lei 9.430/1996, art. 61, caput e par. 1 (do dia seguinte ao vencimento ate o dia do pagamento); Perguntas e Respostas PJ, cap. XVIII, 001";
const SRC_LATE_CAP: &str = "Lei 9.430/1996, art. 61, par. 2";
const SRC_LATE_INTEREST: &str = "Lei 9.430/1996, art. 61, par. 3, e art. 5, par. 3 (Selic acumulada mensalmente do mes seguinte ao vencimento ate o mes anterior ao pagamento, mais 1% no mes do pagamento, sobre o imposto sem a multa); P&R 064; Perguntas e Respostas PJ, cap. XVIII, 001";
const SRC_SELIC_SERIES: &str = "Banco Central, SGS 4390 (Selic acumulada no mes, % a.m.): a mesma taxa da tabela de acrescimos legais da Receita";
const SRC_INSTRUMENT_STOCK: &str =
    "Lei 11.033/2004, arts. 2 e 3, I; IN RFB 1.585/2015, arts. 56 a 59";
const SRC_INSTRUMENT_ETF: &str =
    "IN RFB 1.585/2015, art. 59, par. 2 (isencao nao se aplica a fundo de indice); P&R 707";
const SRC_INSTRUMENT_BDR: &str = "IN RFB 1.585/2015, art. 56, par. 1, I, a (BDR em bolsa: ganho liquido); Lei 11.033/2004, art. 3, I (isencao so para acoes)";
const SRC_INSTRUMENT_FII: &str = "IN RFB 1.585/2015, art. 56, par. 1, I, e, e art. 37";
const SRC_INSTRUMENT_FIAGRO: &str = "Lei 8.668/1993, arts. 20-D e 20-F (incl. Lei 14.130/2021)";
const SRC_INSTRUMENT_CRYPTO: &str = "Lei 8.981/1995, art. 21; Lei 9.250/1995, art. 22; IN SRF 84/2001, art. 2, par. unico (ganho apurado por alienacao); P&R 583 e 653";
const SRC_CODES_IRPF_2026: &str = "Programa IRPF 2026 (ano-calendario 2025); grupo e codigo confirmados so em fonte secundaria (informes de corretoras) — conferir no programa";
const SRC_CODES_IRPF_2027: &str = "Repete o programa IRPF 2026; conferir no programa IRPF 2027 (ano-calendario 2026) quando a Receita publicar";
const SRC_CRYPTO_CODES: &str = "P&R 473 (Grupo 08, codigos 01, 02 e 03; exemplos de stablecoin)";

/// Consulta das fontes desta tabela.
const REVIEWED_ON: Date = date!(2026 - 11 - 25);

fn pct(value: i64, scale: u32) -> Decimal {
    Decimal::new(value, scale)
}

/// O que não muda entre 2025 e 2026 na tabela: alíquotas do ganho em bolsa,
/// isenções, faixas, DARF. Cada ano chama esta função de propósito, depois de
/// conferir que a lei daquele ano não mexeu nesses pontos.
fn exchange_and_crypto(
    year: i32,
    interest_on_equity_rate: Sourced<Decimal>,
    dividend_withholding: Option<DividendWithholding>,
    codes_source: &'static str,
) -> YearRules {
    let irpf = |group, code, label| sourced(IrpfCode { group, code, label }, codes_source);
    YearRules {
        year,
        reviewed_on: REVIEWED_ON,
        instruments: vec![
            InstrumentRule {
                instrument: Instrument::Stock,
                regime: Regime::ExchangeNetGain,
                common_pool: Some(Pool::Common),
                day_trade_pool: Some(Pool::DayTrade),
                exemption: Some(ExemptionId::StockSales),
                sources: SRC_INSTRUMENT_STOCK,
                asset_code: irpf("03", "01", "Acoes (inclusive as listadas em bolsa)"),
            },
            InstrumentRule {
                instrument: Instrument::Etf,
                regime: Regime::ExchangeNetGain,
                common_pool: Some(Pool::Common),
                day_trade_pool: Some(Pool::DayTrade),
                exemption: None,
                sources: SRC_INSTRUMENT_ETF,
                asset_code: irpf("07", "09", "Demais fundos de indice (ETF)"),
            },
            InstrumentRule {
                instrument: Instrument::Bdr,
                regime: Regime::ExchangeNetGain,
                common_pool: Some(Pool::Common),
                day_trade_pool: Some(Pool::DayTrade),
                exemption: None,
                sources: SRC_INSTRUMENT_BDR,
                asset_code: irpf(
                    "04",
                    "04",
                    "Ativos negociados em bolsa no Brasil (BDR, opcoes e outros)",
                ),
            },
            InstrumentRule {
                instrument: Instrument::RealEstateFund,
                regime: Regime::ExchangeNetGain,
                common_pool: Some(Pool::RealEstateFund),
                day_trade_pool: Some(Pool::RealEstateFund),
                exemption: None,
                sources: SRC_INSTRUMENT_FII,
                asset_code: irpf("07", "03", "Fundos de Investimento Imobiliario (FII)"),
            },
            InstrumentRule {
                instrument: Instrument::AgribusinessFund,
                regime: Regime::ExchangeNetGain,
                common_pool: Some(Pool::AgribusinessFund),
                day_trade_pool: Some(Pool::AgribusinessFund),
                exemption: None,
                sources: SRC_INSTRUMENT_FIAGRO,
                asset_code: irpf(
                    "07",
                    "02",
                    "Fundos de Investimento nas Cadeias Produtivas Agroindustriais (Fiagro)",
                ),
            },
            InstrumentRule {
                instrument: Instrument::Crypto,
                regime: Regime::CapitalGain,
                common_pool: None,
                day_trade_pool: None,
                exemption: Some(ExemptionId::CryptoSales),
                sources: SRC_INSTRUMENT_CRYPTO,
                asset_code: sourced(
                    IrpfCode {
                        group: "08",
                        code: "02",
                        label: "Criptoativos (codigo pelo tipo: 01, 02 ou 03)",
                    },
                    SRC_CRYPTO_CODES,
                ),
            },
        ],
        pools: vec![
            PoolRule {
                pool: Pool::Common,
                rate: sourced(pct(15, 2), SRC_COMMON_RATE),
            },
            PoolRule {
                pool: Pool::DayTrade,
                rate: sourced(pct(20, 2), SRC_DAY_TRADE_RATE),
            },
            PoolRule {
                pool: Pool::RealEstateFund,
                rate: sourced(pct(20, 2), SRC_FII_RATE),
            },
            PoolRule {
                pool: Pool::AgribusinessFund,
                rate: sourced(pct(20, 2), SRC_FIAGRO_RATE),
            },
        ],
        exemptions: vec![
            ExemptionRule {
                id: ExemptionId::StockSales,
                limit: sourced(Decimal::new(20_000, 0), SRC_STOCK_EXEMPTION),
                counted: &[Instrument::Stock],
                counts_day_trade: sourced(true, SRC_STOCK_EXEMPTION_DAY_TRADE),
            },
            ExemptionRule {
                id: ExemptionId::CryptoSales,
                limit: sourced(Decimal::new(35_000, 0), SRC_CRYPTO_EXEMPTION),
                counted: &[Instrument::Crypto],
                counts_day_trade: sourced(true, SRC_CRYPTO_EXEMPTION_DAY_TRADE),
            },
        ],
        capital_gain_brackets: sourced(
            vec![
                Bracket {
                    up_to: Some(Decimal::new(5_000_000, 0)),
                    rate: pct(15, 2),
                },
                Bracket {
                    up_to: Some(Decimal::new(10_000_000, 0)),
                    rate: pct(175, 3),
                },
                Bracket {
                    up_to: Some(Decimal::new(30_000_000, 0)),
                    rate: pct(20, 2),
                },
                Bracket {
                    up_to: None,
                    rate: pct(225, 3),
                },
            ],
            SRC_BRACKETS,
        ),
        exchange_darf_code: sourced("6015", SRC_DARF_6015),
        capital_gain_darf_code: sourced("4600", SRC_DARF_4600),
        darf_minimum: sourced(Decimal::new(10, 0), SRC_DARF_MINIMUM),
        withholding_carry: sourced(true, SRC_WITHHOLDING_CARRY),
        interest_on_equity_rate,
        dividend_withholding,
        income_codes: IncomeCodes {
            dividends: irpf("isentos", "09", "Lucros e dividendos recebidos"),
            fund_income: irpf("isentos", "99", "Outros (rendimentos de FII e Fiagro)"),
            exempt_stock_gains: irpf(
                "isentos",
                "20",
                "Ganhos liquidos em operacoes no mercado a vista de acoes ate R$ 20.000,00 no mes",
            ),
            exempt_crypto_gains: irpf(
                "isentos",
                "05",
                "Ganho de capital em alienacoes de ate R$ 35.000,00 no mes",
            ),
            interest_on_equity: irpf("exclusiva", "10", "Juros sobre capital proprio"),
        },
        crypto_codes: CryptoCodes {
            bitcoin: IrpfCode {
                group: "08",
                code: "01",
                label: "Criptomoeda Bitcoin (BTC)",
            },
            altcoin: IrpfCode {
                group: "08",
                code: "02",
                label: "Outras criptomoedas (altcoins)",
            },
            stablecoin: IrpfCode {
                group: "08",
                code: "03",
                label: "Stablecoins",
            },
            bitcoin_symbols: &["BTC"],
            stablecoin_symbols: &["USDT", "BRZ", "USDC", "BUSD", "TUSD", "DAI", "PAXG", "GUSD"],
            source: SRC_CRYPTO_CODES,
        },
        withholding: WithholdingRule {
            common_rate: sourced(pct(5, 5), SRC_WITHHOLDING_COMMON),
            common_dispensed_up_to: sourced(Decimal::ONE, SRC_WITHHOLDING_DISPENSED),
            day_trade_rate: sourced(pct(1, 2), SRC_WITHHOLDING_DAY_TRADE),
        },
        late_payment: LatePaymentRule {
            daily_penalty: sourced(pct(33, 4), SRC_LATE_PENALTY),
            penalty_cap: sourced(pct(20, 2), SRC_LATE_CAP),
            payment_month_interest: sourced(pct(1, 2), SRC_LATE_INTEREST),
            selic_series: sourced("4390", SRC_SELIC_SERIES),
        },
    }
}

/// 2025: a MP 1.303/2025 (que unificaria alíquotas em 17,5% a partir de 2026)
/// perdeu a eficácia em 8/10/2025 sem virar lei (Ato do Presidente do
/// Congresso 67/2025) e não produziu efeito. Vale o regime de sempre.
fn year_2025() -> YearRules {
    exchange_and_crypto(
        2025,
        sourced(pct(15, 2), SRC_JCP_2025),
        None,
        SRC_CODES_IRPF_2026,
    )
}

/// 2026: o ganho em bolsa e cripto não mudou. Mudaram o JCP (17,5%, LC
/// 224/2025) e os dividendos acima de R$ 50 mil por mês de uma mesma empresa
/// (retenção de 10% sobre o total, Lei 15.270/2025).
fn year_2026() -> YearRules {
    exchange_and_crypto(
        2026,
        sourced(pct(175, 3), SRC_JCP_2026),
        Some(DividendWithholding {
            monthly_threshold: Decimal::new(50_000, 0),
            rate: pct(10, 2),
            source: SRC_DIVIDENDS_2026,
        }),
        SRC_CODES_IRPF_2027,
    )
}

/// Retenção que a empresa deve fazer sobre os dividendos pagos a uma pessoa no
/// mês (`total_from_payer` = soma do mês, da mesma empresa). Zero em ano sem a
/// regra ou abaixo do limite. Não vale para lucro apurado até 2025 com
/// distribuição aprovada até 31/12/2025 (art. 6-A, par. 3): quem chama separa.
pub fn dividend_withholding(rules: &YearRules, total_from_payer: Decimal) -> Decimal {
    match &rules.dividend_withholding {
        Some(rule) if total_from_payer > rule.monthly_threshold => total_from_payer * rule.rate,
        _ => Decimal::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn covered_years_are_2025_and_2026() {
        assert_eq!(covered_years(), vec![2025, 2026]);
    }

    #[test]
    fn year_without_rules_is_an_error() {
        assert_eq!(rules_for(2024), Err(TaxError::NoRulesForYear(2024)));
        assert_eq!(rules_for(2027), Err(TaxError::NoRulesForYear(2027)));
    }

    #[test]
    fn every_year_covers_every_instrument_pool_and_exemption() {
        for rules in table() {
            for instrument in Instrument::ALL {
                let rule = rules.instrument(instrument).unwrap();
                assert!(!rule.sources.is_empty());
                assert!(!rule.asset_code.source.is_empty());
                if rule.regime == Regime::ExchangeNetGain {
                    assert!(rule.common_pool.is_some(), "{instrument:?}");
                    assert!(rule.day_trade_pool.is_some(), "{instrument:?}");
                }
            }
            for pool in Pool::ALL {
                assert!(!rules.pool(pool).unwrap().rate.source.is_empty());
            }
            rules.exemption(ExemptionId::StockSales).unwrap();
            rules.exemption(ExemptionId::CryptoSales).unwrap();
            assert_eq!(rules.reviewed_on, date!(2026 - 11 - 25));
        }
    }

    #[test]
    fn rates_and_limits_match_the_law() {
        for year in [2025, 2026] {
            let rules = rules_for(year).unwrap();
            assert_eq!(rules.pool(Pool::Common).unwrap().rate.value, dec!(0.15));
            assert_eq!(rules.pool(Pool::DayTrade).unwrap().rate.value, dec!(0.20));
            assert_eq!(
                rules.pool(Pool::RealEstateFund).unwrap().rate.value,
                dec!(0.20)
            );
            assert_eq!(
                rules.pool(Pool::AgribusinessFund).unwrap().rate.value,
                dec!(0.20)
            );
            let stocks = rules.exemption(ExemptionId::StockSales).unwrap();
            assert_eq!(stocks.limit.value, dec!(20000));
            assert_eq!(stocks.counted, &[Instrument::Stock]);
            let crypto = rules.exemption(ExemptionId::CryptoSales).unwrap();
            assert_eq!(crypto.limit.value, dec!(35000));
            assert_eq!(rules.darf_code(Regime::ExchangeNetGain), "6015");
            assert_eq!(rules.darf_code(Regime::CapitalGain), "4600");
            assert_eq!(rules.darf_minimum.value, dec!(10));
            // IR retido: 0,005% das vendas comuns (dispensado até R$ 1 no
            // mês) e 1% do resultado positivo do day trade.
            assert_eq!(rules.withholding.common_rate.value, dec!(0.00005));
            assert_eq!(rules.withholding.common_dispensed_up_to.value, dec!(1));
            assert_eq!(rules.withholding.day_trade_rate.value, dec!(0.01));
            // Atraso: 0,33% ao dia até 20%, mais 1% no mês do pagamento.
            assert_eq!(rules.late_payment.daily_penalty.value, dec!(0.0033));
            assert_eq!(rules.late_payment.penalty_cap.value, dec!(0.20));
            assert_eq!(rules.late_payment.payment_month_interest.value, dec!(0.01));
            assert_eq!(rules.late_payment.selic_series.value, "4390");
            // Só ação tem a isenção de R$ 20 mil: ETF, BDR, FII e Fiagro não.
            for instrument in [
                Instrument::Etf,
                Instrument::Bdr,
                Instrument::RealEstateFund,
                Instrument::AgribusinessFund,
            ] {
                assert_eq!(rules.instrument(instrument).unwrap().exemption, None);
            }
        }
    }

    #[test]
    fn interest_on_equity_went_to_17_5_percent_in_2026() {
        assert_eq!(
            rules_for(2025).unwrap().interest_on_equity_rate.value,
            dec!(0.15)
        );
        assert_eq!(
            rules_for(2026).unwrap().interest_on_equity_rate.value,
            dec!(0.175)
        );
    }

    #[test]
    fn dividend_withholding_starts_in_2026_above_50k_on_the_whole_amount() {
        let r2025 = rules_for(2025).unwrap();
        let r2026 = rules_for(2026).unwrap();
        assert_eq!(dividend_withholding(&r2025, dec!(80000)), dec!(0));
        assert_eq!(dividend_withholding(&r2026, dec!(50000)), dec!(0));
        assert_eq!(dividend_withholding(&r2026, dec!(50000.01)), dec!(5000.001));
        assert_eq!(dividend_withholding(&r2026, dec!(80000)), dec!(8000));
    }

    #[test]
    fn capital_gain_brackets_are_progressive_by_slice() {
        // P&R 574: 15% até 5 mi, 17,5% de 5 a 10 mi, 20% de 10 a 30 mi, 22,5% acima.
        let rules = rules_for(2026).unwrap();
        assert_eq!(rules.capital_gain_tax(dec!(-1)), dec!(0));
        assert_eq!(rules.capital_gain_tax(dec!(1000)), dec!(150));
        assert_eq!(rules.capital_gain_tax(dec!(5000000)), dec!(750000));
        assert_eq!(
            rules.capital_gain_tax(dec!(6000000)),
            dec!(750000) + dec!(175000)
        );
        assert_eq!(
            rules.capital_gain_tax(dec!(40000000)),
            dec!(750000) + dec!(875000) + dec!(4000000) + dec!(2250000)
        );
    }

    #[test]
    fn crypto_codes_follow_the_type() {
        let rules = rules_for(2025).unwrap();
        assert_eq!(rules.crypto_asset_code("BTC").code, "01");
        assert_eq!(rules.crypto_asset_code("BTC-USD").code, "01");
        assert_eq!(rules.crypto_asset_code("usdt").code, "03");
        assert_eq!(rules.crypto_asset_code("ETH-USD").code, "02");
        assert_eq!(rules.crypto_asset_code("SOL").group, "08");
    }
}
