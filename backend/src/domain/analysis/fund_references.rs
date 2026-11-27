//! As referências de preço dos FIIs, no lugar de Bazin e Graham: o **valor
//! patrimonial por cota** (o preço de P/VP 1) e o **Rendimento** (o preço em que
//! os proventos de 12 meses pagariam o rendimento-alvo do tipo do fundo).
//! Referência pública, não é preço justo nem recomendação. As fórmulas, as
//! fontes e o porquê de cada prêmio estão em `docs/analise.md`.
//!
//! - **VP**: patrimônio líquido ÷ cotas, do informe mensal da CVM.
//! - **Rendimento**: `proventos por cota de 12 meses ÷ alvo`, com o alvo pelo
//!   tipo (o mesmo `sector` que a CVM dá ao fundo, `sectors::cvm`):
//!   - tijolo: juro real do Tesouro IPCA+ de ~10 anos + prêmio (padrão
//!     [`DEFAULT_BRICK_PREMIUM`]);
//!   - papel: CDI de 12 meses × (1 − 15% de IR) + prêmio (padrão
//!     [`DEFAULT_PAPER_PREMIUM`]);
//!   - híbrido: os dois alvos ponderados pela composição ([`weights`]);
//!   - fundo de fundos: a média simples dos dois (a carteira de baixo não é
//!     conhecida).
//! - **Pelas duas**: o menor de VP e Rendimento ([`combined_ceiling`]).

use rust_decimal::{Decimal, RoundingStrategy};

use super::references::{combined_ceiling, ReferenceGap};
use crate::domain::allocation::round_cents;
use crate::domain::fundamentals::AssetFundamentals;
use crate::domain::market_rates::{Cdi12m, RealRate};
use crate::domain::sector::{FundComposition, FundType};

/// Prêmio de fábrica sobre o juro real para o FII de tijolo, em pontos
/// percentuais.
pub const DEFAULT_BRICK_PREMIUM: Decimal = Decimal::from_parts(25, 0, 0, false, 1);
/// Prêmio de fábrica sobre o CDI líquido para o FII de papel, em pontos
/// percentuais.
pub const DEFAULT_PAPER_PREMIUM: Decimal = Decimal::from_parts(1, 0, 0, false, 0);
/// Maior prêmio aceito, em pontos percentuais.
pub const MAX_PREMIUM: Decimal = Decimal::from_parts(20, 0, 0, false, 0);
/// O IR que o CDI paga numa aplicação de mais de 2 anos (a menor alíquota da
/// tabela regressiva); o provento de FII para pessoa física é isento, então o
/// alvo do papel compara com o CDI líquido.
pub const PAPER_INCOME_TAX: Decimal = Decimal::from_parts(15, 0, 0, false, 2);

const HALF: Decimal = Decimal::from_parts(5, 0, 0, false, 1);

/// Duas casas, com o meio para longe do zero (11,345 → 11,35), como a conta
/// que a tela mostra.
fn round2(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Confere um prêmio que a pessoa mandou: de 0 a 20 pontos percentuais, até
/// duas casas. A mensagem vai sem acento.
pub fn validate_premium(value: Decimal) -> Result<Decimal, String> {
    if value >= Decimal::ZERO && value <= MAX_PREMIUM && value.normalize().scale() <= 2 {
        Ok(value.normalize())
    } else {
        Err(
            "O premio precisa estar entre 0 e 20 pontos percentuais, com ate duas casas."
                .to_owned(),
        )
    }
}

/// Os prêmios da pessoa, em pontos percentuais ao ano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Premiums {
    pub brick: Decimal,
    pub paper: Decimal,
}

impl Default for Premiums {
    fn default() -> Self {
        Self {
            brick: DEFAULT_BRICK_PREMIUM,
            paper: DEFAULT_PAPER_PREMIUM,
        }
    }
}

/// As taxas de mercado de agora; cada uma pode faltar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarketRates {
    pub real_rate: Option<RealRate>,
    pub cdi_12m: Option<Cdi12m>,
}

/// O CDI de 12 meses depois do IR de 15%, com duas casas.
pub fn net_cdi(cdi_percent: Decimal) -> Decimal {
    round2(cdi_percent * (Decimal::ONE - PAPER_INCOME_TAX))
}

/// O alvo do tijolo: juro real + prêmio. `None` sem o juro real.
pub fn brick_target(rates: &MarketRates, premiums: Premiums) -> Option<Decimal> {
    rates.real_rate.map(|r| r.percent + premiums.brick)
}

/// O alvo do papel: CDI de 12 meses líquido + prêmio. `None` sem o CDI.
pub fn paper_target(rates: &MarketRates, premiums: Premiums) -> Option<Decimal> {
    rates.cdi_12m.map(|c| net_cdi(c.percent) + premiums.paper)
}

/// Quanto do alvo vem do tijolo e quanto do papel. Somam 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    pub brick: Decimal,
    pub paper: Decimal,
}

/// Os pesos de cada tipo: tijolo e papel são inteiros; o fundo de fundos é
/// meio a meio (a carteira de baixo não é conhecida); o híbrido é a própria
/// composição, com as cotas de FII contando metade para cada lado. Híbrido sem
/// composição fica sem pesos.
pub fn weights(
    kind: FundType,
    composition: Option<FundComposition>,
) -> Result<Weights, ReferenceGap> {
    Ok(match kind {
        FundType::Brick => Weights {
            brick: Decimal::ONE,
            paper: Decimal::ZERO,
        },
        FundType::Paper => Weights {
            brick: Decimal::ZERO,
            paper: Decimal::ONE,
        },
        FundType::FundOfFunds => Weights {
            brick: HALF,
            paper: HALF,
        },
        FundType::Hybrid => {
            let mix = composition.ok_or(ReferenceGap::NoComposition)?;
            let brick = mix.brick + mix.funds * HALF;
            Weights {
                brick,
                paper: Decimal::ONE - brick,
            }
        }
    })
}

/// O rendimento-alvo de um FII e de onde ele saiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetYield {
    /// O alvo, em % ao ano, com duas casas.
    pub percent: Decimal,
    pub weights: Weights,
    /// O alvo do tijolo, quando entra (peso maior que zero).
    pub brick: Option<Decimal>,
    /// O alvo do papel, quando entra.
    pub paper: Option<Decimal>,
}

/// O alvo pelo tipo. Sem tipo, sem composição (híbrido), sem a taxa de que o
/// alvo precisa ou com alvo não positivo, o motivo.
pub fn target_yield(
    kind: Option<FundType>,
    composition: Option<FundComposition>,
    rates: &MarketRates,
    premiums: Premiums,
) -> Result<TargetYield, ReferenceGap> {
    let kind = kind.ok_or(ReferenceGap::UnknownFundType)?;
    let weights = weights(kind, composition)?;
    let needed = |weight: Decimal, target: Option<Decimal>| {
        if weight > Decimal::ZERO {
            target.map(Some).ok_or(ReferenceGap::NoMarketRate)
        } else {
            Ok(None)
        }
    };
    let brick = needed(weights.brick, brick_target(rates, premiums))?;
    let paper = needed(weights.paper, paper_target(rates, premiums))?;
    let percent = round2(
        weights.brick * brick.unwrap_or_default() + weights.paper * paper.unwrap_or_default(),
    );
    if percent <= Decimal::ZERO {
        return Err(ReferenceGap::NonPositiveTarget);
    }
    Ok(TargetYield {
        percent,
        weights,
        brick,
        paper,
    })
}

/// O Rendimento de um FII: o preço em que os proventos de 12 meses pagariam o
/// alvo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundYield {
    /// Proventos por cota dos últimos 12 meses, em reais.
    pub dividends_12m: Decimal,
    pub target: TargetYield,
    /// `proventos ÷ alvo`, em reais por cota.
    pub price: Decimal,
}

/// O Rendimento, ou o motivo de não haver. O alvo vem primeiro (tipo,
/// composição, taxa): sem ele, os proventos não dizem nada.
pub fn fund_yield(
    dividends_12m: Option<Decimal>,
    kind: Option<FundType>,
    composition: Option<FundComposition>,
    rates: &MarketRates,
    premiums: Premiums,
) -> Result<FundYield, ReferenceGap> {
    let target = target_yield(kind, composition, rates, premiums)?;
    let dividends_12m = dividends_12m
        .filter(|d| *d > Decimal::ZERO)
        .ok_or(ReferenceGap::NoDividends)?;
    Ok(FundYield {
        dividends_12m,
        target,
        price: round_cents(dividends_12m * Decimal::ONE_HUNDRED / target.percent),
    })
}

/// O valor patrimonial por cota (P/VP 1), dos fundamentos do FII.
pub fn nav_reference(fundamentals: Option<&AssetFundamentals>) -> Result<Decimal, ReferenceGap> {
    let Some(AssetFundamentals::Fund(metrics)) = fundamentals else {
        return Err(ReferenceGap::NoFundamentals);
    };
    match metrics.nav_per_share.value() {
        Some(nav) if nav > Decimal::ZERO => Ok(round_cents(nav)),
        Some(_) => Err(ReferenceGap::NegativeEquity),
        None => Err(ReferenceGap::NoFundamentals),
    }
}

/// As referências de um FII: o tipo e a composição que entraram, VP e
/// Rendimento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundReferences {
    pub fund_type: Option<FundType>,
    pub composition: Option<FundComposition>,
    pub nav: Result<Decimal, ReferenceGap>,
    pub fund_yield: Result<FundYield, ReferenceGap>,
}

impl FundReferences {
    /// "Pelas duas": o menor de VP e Rendimento, ou o que existir.
    pub fn combined(&self) -> Result<Decimal, ReferenceGap> {
        combined_ceiling(self.nav.ok(), self.fund_yield.ok().map(|y| y.price))
            .ok_or(ReferenceGap::NoReference)
    }

    /// Se o FII tem ao menos uma das duas.
    pub fn any(&self) -> bool {
        self.nav.is_ok() || self.fund_yield.is_ok()
    }
}

/// As referências de um FII a partir do que já está guardado: os fundamentos
/// (VP), os proventos por cota de 12 meses (a mesma conta da estimativa de
/// renda), o tipo e a composição da CVM, as taxas e os prêmios da pessoa.
pub fn fund_references(
    fundamentals: Option<&AssetFundamentals>,
    dividends_12m: Option<Decimal>,
    fund_type: Option<FundType>,
    composition: Option<FundComposition>,
    rates: &MarketRates,
    premiums: Premiums,
) -> FundReferences {
    FundReferences {
        fund_type,
        composition,
        nav: nav_reference(fundamentals),
        fund_yield: fund_yield(dividends_12m, fund_type, composition, rates, premiums),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fundamentals::{fund_metrics, FundReport};
    use crate::domain::treasury::{Bond, BondKind};
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn rates(real: Option<Decimal>, cdi: Option<Decimal>) -> MarketRates {
        MarketRates {
            real_rate: real.map(|percent| RealRate {
                bond: Bond::new(BondKind::IpcaPlusWithCoupons, date!(2037 - 05 - 15)),
                percent,
                base_date: date!(2026 - 11 - 19),
            }),
            cdi_12m: cdi.map(|percent| Cdi12m {
                percent,
                from: date!(2025 - 11 - 19),
                to: date!(2026 - 11 - 18),
            }),
        }
    }

    fn market() -> MarketRates {
        rates(Some(dec!(7.12)), Some(dec!(14.20)))
    }

    fn fund(net_assets: Decimal, shares: Decimal) -> AssetFundamentals {
        AssetFundamentals::Fund(Box::new(fund_metrics(
            &FundReport {
                cnpj: "00.000.000/0001-91".into(),
                issuer: "FICT".into(),
                isin: "BRFICTCTF000".into(),
                name: "FUNDO FICTICIO FII".into(),
                reference_date: date!(2026 - 10 - 31),
                version: 1,
                net_assets: Some(net_assets),
                shares: Some(shares),
                total_assets: None,
                liquid_assets: None,
            },
            None,
            None,
            None,
        )))
    }

    #[test]
    fn a_brick_fund_uses_the_real_rate_plus_the_premium() {
        // Alvo 7,12% + 2,5 p.p. = 9,62%; proventos 12m R$ 13,20 ÷ 9,62% = R$ 137,21.
        let found = fund_yield(
            Some(dec!(13.20)),
            Some(FundType::Brick),
            None,
            &market(),
            Premiums::default(),
        )
        .unwrap();
        assert_eq!(found.target.percent, dec!(9.62));
        assert_eq!(found.target.brick, Some(dec!(9.62)));
        assert_eq!(found.target.paper, None);
        assert_eq!(found.price, dec!(137.21));
    }

    #[test]
    fn a_paper_fund_uses_the_net_cdi_plus_the_premium() {
        // CDI 14,20% × 0,85 = 12,07%; + 1,0 p.p. = 13,07%; R$ 1,30 ÷ 13,07% = R$ 9,95.
        assert_eq!(net_cdi(dec!(14.20)), dec!(12.07));
        let found = fund_yield(
            Some(dec!(1.30)),
            Some(FundType::Paper),
            None,
            &market(),
            Premiums::default(),
        )
        .unwrap();
        assert_eq!(found.target.percent, dec!(13.07));
        assert_eq!(found.target.brick, None);
        assert_eq!(found.price, dec!(9.95));
    }

    #[test]
    fn a_hybrid_weighs_the_two_targets_and_half_of_the_fund_shares_go_each_way() {
        // 50% imóveis, 30% recebíveis, 20% cotas: 60% tijolo, 40% papel.
        let mix = FundComposition::from_amounts(dec!(50), dec!(30), dec!(20)).unwrap();
        let w = weights(FundType::Hybrid, Some(mix)).unwrap();
        assert_eq!((w.brick, w.paper), (dec!(0.6), dec!(0.4)));
        let target = target_yield(
            Some(FundType::Hybrid),
            Some(mix),
            &market(),
            Premiums::default(),
        )
        .unwrap();
        // 0,6 × 9,62 + 0,4 × 13,07 = 5,772 + 5,228 = 11,00.
        assert_eq!(target.percent, dec!(11.00));
        assert_eq!(
            target_yield(Some(FundType::Hybrid), None, &market(), Premiums::default()),
            Err(ReferenceGap::NoComposition)
        );
    }

    #[test]
    fn a_fund_of_funds_is_the_simple_average() {
        let target = target_yield(
            Some(FundType::FundOfFunds),
            None,
            &market(),
            Premiums::default(),
        )
        .unwrap();
        // (9,62 + 13,07) ÷ 2 = 11,345 → 11,35.
        assert_eq!(target.percent, dec!(11.35));
        assert_eq!(target.weights.brick, dec!(0.5));
    }

    #[test]
    fn missing_pieces_have_reasons() {
        let p = Premiums::default();
        assert_eq!(
            fund_yield(Some(dec!(1)), None, None, &market(), p),
            Err(ReferenceGap::UnknownFundType)
        );
        assert_eq!(
            fund_yield(
                Some(dec!(1)),
                Some(FundType::Brick),
                None,
                &rates(None, Some(dec!(10))),
                p
            ),
            Err(ReferenceGap::NoMarketRate)
        );
        // O papel não precisa do juro real.
        assert!(fund_yield(
            Some(dec!(1)),
            Some(FundType::Paper),
            None,
            &rates(None, Some(dec!(10))),
            p
        )
        .is_ok());
        assert_eq!(
            fund_yield(
                Some(dec!(1)),
                Some(FundType::Paper),
                None,
                &rates(Some(dec!(7)), None),
                p
            ),
            Err(ReferenceGap::NoMarketRate)
        );
        for dividends in [None, Some(Decimal::ZERO)] {
            assert_eq!(
                fund_yield(dividends, Some(FundType::Brick), None, &market(), p),
                Err(ReferenceGap::NoDividends)
            );
        }
        // Juro real negativo e prêmio zero: não há alvo.
        let zero = Premiums {
            brick: Decimal::ZERO,
            paper: Decimal::ZERO,
        };
        assert_eq!(
            fund_yield(
                Some(dec!(1)),
                Some(FundType::Brick),
                None,
                &rates(Some(dec!(-0.5)), None),
                zero
            ),
            Err(ReferenceGap::NonPositiveTarget)
        );
    }

    #[test]
    fn the_nav_is_the_book_value_per_share() {
        assert_eq!(
            nav_reference(Some(&fund(dec!(1_000_000), dec!(10_000)))),
            Ok(dec!(100))
        );
        assert_eq!(
            nav_reference(Some(&fund(dec!(1_000_000), dec!(30_000)))),
            Ok(dec!(33.33))
        );
        assert_eq!(
            nav_reference(Some(&fund(dec!(-10), dec!(10)))),
            Err(ReferenceGap::NegativeEquity)
        );
        assert_eq!(
            nav_reference(Some(&fund(dec!(10), dec!(0)))),
            Err(ReferenceGap::NoFundamentals)
        );
        assert_eq!(nav_reference(None), Err(ReferenceGap::NoFundamentals));
    }

    #[test]
    fn the_combined_is_the_lower_of_nav_and_yield() {
        let refs = fund_references(
            Some(&fund(dec!(1_000_000), dec!(10_000))),
            Some(dec!(13.20)),
            Some(FundType::Brick),
            None,
            &market(),
            Premiums::default(),
        );
        assert_eq!(refs.nav, Ok(dec!(100)));
        assert_eq!(refs.fund_yield.map(|y| y.price), Ok(dec!(137.21)));
        assert_eq!(refs.combined(), Ok(dec!(100)));
        assert!(refs.any());
        let none = fund_references(
            None,
            None,
            None,
            None,
            &MarketRates::default(),
            Premiums::default(),
        );
        assert_eq!(none.combined(), Err(ReferenceGap::NoReference));
        assert!(!none.any());
    }

    #[test]
    fn premiums_are_validated() {
        assert_eq!(validate_premium(dec!(2.50)), Ok(dec!(2.5)));
        assert_eq!(validate_premium(dec!(0)), Ok(dec!(0)));
        assert_eq!(validate_premium(dec!(20)), Ok(dec!(20)));
        assert!(validate_premium(dec!(20.01)).is_err());
        assert!(validate_premium(dec!(-0.1)).is_err());
        assert!(validate_premium(dec!(1.234)).is_err());
        assert_eq!(validate_premium(DEFAULT_BRICK_PREMIUM), Ok(dec!(2.5)));
        assert_eq!(validate_premium(DEFAULT_PAPER_PREMIUM), Ok(dec!(1)));
    }

    fn arb_type() -> impl Strategy<Value = FundType> {
        prop::sample::select(FundType::ALL.to_vec())
    }

    fn arb_mix() -> impl Strategy<Value = FundComposition> {
        (0i64..1000, 0i64..1000, 0i64..1000, 1i64..1000).prop_map(|(b, p, f, extra)| {
            FundComposition::from_amounts(
                Decimal::from(b + extra),
                Decimal::from(p),
                Decimal::from(f),
            )
            .unwrap()
        })
    }

    proptest! {
        /// Prêmio maior, alvo maior, referência menor (ou igual, no centavo).
        #[test]
        fn a_higher_premium_never_raises_the_reference(
            kind in arb_type(),
            mix in arb_mix(),
            dividends in 1i64..5_000,
            real in 100i64..1200,
            cdi in 200i64..2500,
            low in 0i64..1000,
            extra in 1i64..1000,
            brick_side in any::<bool>(),
        ) {
            let market = rates(Some(Decimal::new(real, 2)), Some(Decimal::new(cdi, 2)));
            let base = Premiums { brick: Decimal::new(low, 2), paper: Decimal::new(low, 2) };
            let higher = if brick_side {
                Premiums { brick: base.brick + Decimal::new(extra, 2), ..base }
            } else {
                Premiums { paper: base.paper + Decimal::new(extra, 2), ..base }
            };
            let dividends = Some(Decimal::new(dividends, 2));
            let cheap = fund_yield(dividends, Some(kind), Some(mix), &market, base).unwrap();
            let dear = fund_yield(dividends, Some(kind), Some(mix), &market, higher).unwrap();
            prop_assert!(dear.target.percent >= cheap.target.percent);
            prop_assert!(dear.price <= cheap.price, "{} > {}", dear.price, cheap.price);
        }

        /// "Pelas duas" nunca passa nenhuma das duas, e é uma delas.
        #[test]
        fn the_combined_never_passes_either(
            nav_cents in prop::option::of(1i64..1_000_000),
            dividends in prop::option::of(1i64..5_000),
            kind in prop::option::of(arb_type()),
            mix in arb_mix(),
        ) {
            let fundamentals = nav_cents.map(|c| fund(Decimal::new(c, 2), Decimal::ONE));
            let refs = fund_references(
                fundamentals.as_ref(),
                dividends.map(|d| Decimal::new(d, 2)),
                kind,
                Some(mix),
                &market(),
                Premiums::default(),
            );
            match refs.combined() {
                Ok(combined) => {
                    prop_assert!(refs.nav.is_ok() || refs.fund_yield.is_ok());
                    if let Ok(nav) = refs.nav { prop_assert!(combined <= nav); }
                    if let Ok(found) = refs.fund_yield { prop_assert!(combined <= found.price); }
                    prop_assert!(refs.nav == Ok(combined) || refs.fund_yield.map(|y| y.price) == Ok(combined));
                }
                Err(gap) => {
                    prop_assert_eq!(gap, ReferenceGap::NoReference);
                    prop_assert!(refs.nav.is_err() && refs.fund_yield.is_err());
                }
            }
        }

        /// Os pesos do híbrido somam 1 e ficam entre 0 e 1, qualquer que seja a carteira.
        #[test]
        fn hybrid_weights_sum_to_one(mix in arb_mix()) {
            let w = weights(FundType::Hybrid, Some(mix)).unwrap();
            prop_assert_eq!(w.brick + w.paper, Decimal::ONE);
            prop_assert!(w.brick >= Decimal::ZERO && w.brick <= Decimal::ONE);
            prop_assert!(w.paper >= Decimal::ZERO && w.paper <= Decimal::ONE);
            for kind in FundType::ALL {
                let w = weights(kind, Some(mix)).unwrap();
                prop_assert_eq!(w.brick + w.paper, Decimal::ONE);
            }
        }

        /// Tipo desconhecido: nunca há Rendimento, e o motivo é o tipo.
        #[test]
        fn an_unknown_type_has_no_yield(
            dividends in prop::option::of(0i64..5_000),
            real in prop::option::of(-200i64..1500),
            cdi in prop::option::of(0i64..2500),
            mix in prop::option::of(arb_mix()),
        ) {
            let market = rates(real.map(|r| Decimal::new(r, 2)), cdi.map(|c| Decimal::new(c, 2)));
            prop_assert_eq!(
                fund_yield(dividends.map(|d| Decimal::new(d, 2)), None, mix, &market, Premiums::default()),
                Err(ReferenceGap::UnknownFundType)
            );
        }
    }
}
