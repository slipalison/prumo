//! Saúde da carteira: avisos sobre o que a meta por classe não enxerga —
//! concentração num ativo, nos cinco maiores ou num setor, poucos ativos numa
//! classe e classe com meta e sem nada dentro.
//!
//! Não há nota de 0 a 100 nem pesos: cada regra é uma pergunta de sim ou não
//! com um limite que a pessoa escolhe, e o resultado é a lista do que passou
//! do limite, com o número de agora. O desenho e o porquê de cada regra estão
//! em `docs/saude.md`.
//!
//! Tudo aqui é puro: recebe os ativos já avaliados em reais e devolve avisos.
//! "Ativo" é o que tem valor (`value > 0`); um ativo cadastrado e ainda não
//! comprado não diversifica nada.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::allocation::{percent, Targets};
use super::asset_class::AssetClass;

/// Quantos ativos entram na regra dos maiores.
pub const TOP_COUNT: usize = 5;

/// As classes em que a regra de setor vale: as que têm fonte de setor.
const SECTOR_CLASSES: [AssetClass; 2] = [AssetClass::BrStocks, AssetClass::BrReits];

const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

/// Maior quantidade aceita em `min_assets`: acima disso a regra não faz sentido.
const MAX_MIN_ASSETS: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleId {
    /// Nenhum ativo acima de X% da carteira.
    MaxAsset,
    /// Os cinco maiores juntos abaixo de Y%.
    TopFive,
    /// Nenhum setor acima de Z% dentro de Ações Nacionais e de FIIs.
    MaxSector,
    /// Mínimo de N ativos em cada classe que tem meta.
    MinAssets,
    /// Classe com meta e nenhum ativo.
    EmptyClass,
}

/// Em que o limite de uma regra é medido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Count,
}

impl RuleId {
    /// A ordem do painel.
    pub const ALL: [RuleId; 5] = [
        RuleId::MaxAsset,
        RuleId::TopFive,
        RuleId::MaxSector,
        RuleId::MinAssets,
        RuleId::EmptyClass,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleId::MaxAsset => "max_asset",
            RuleId::TopFive => "top_five",
            RuleId::MaxSector => "max_sector",
            RuleId::MinAssets => "min_assets",
            RuleId::EmptyClass => "empty_class",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        RuleId::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// Em que o limite é medido; `None` na regra que não tem limite.
    pub fn unit(self) -> Option<Unit> {
        match self {
            RuleId::MaxAsset | RuleId::TopFive | RuleId::MaxSector => Some(Unit::Percent),
            RuleId::MinAssets => Some(Unit::Count),
            RuleId::EmptyClass => None,
        }
    }

    /// O limite de fábrica, antes de a pessoa mexer.
    pub fn default_threshold(self) -> Option<Decimal> {
        match self {
            RuleId::MaxAsset => Some(Decimal::from(15)),
            RuleId::TopFive => Some(Decimal::from(60)),
            RuleId::MaxSector => Some(Decimal::from(40)),
            RuleId::MinAssets => Some(Decimal::from(3)),
            RuleId::EmptyClass => None,
        }
    }

    /// Confere o limite que a pessoa mandou e o devolve normalizado. A mensagem
    /// de erro vai sem acento (atravessa log e JSON de erro).
    pub fn validate_threshold(self, value: Decimal) -> Result<Decimal, String> {
        match self.unit() {
            None => Err("Essa regra nao tem limite.".to_owned()),
            Some(Unit::Percent) => {
                if value > Decimal::ZERO && value <= HUNDRED && value.normalize().scale() <= 2 {
                    Ok(value.normalize())
                } else {
                    Err(
                        "O limite precisa ser um percentual maior que 0 e ate 100, com ate duas casas."
                            .to_owned(),
                    )
                }
            }
            Some(Unit::Count) => {
                let whole = value.fract().is_zero();
                if whole && value >= Decimal::ONE && value <= Decimal::from(MAX_MIN_ASSETS) {
                    Ok(value.normalize())
                } else {
                    Err(format!(
                        "O minimo de ativos precisa ser um numero inteiro de 1 a {MAX_MIN_ASSETS}."
                    ))
                }
            }
        }
    }
}

/// Uma regra como a pessoa a deixou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSetting {
    pub enabled: bool,
    /// Limite em [`RuleId::unit`]; `None` na regra sem limite.
    pub threshold: Option<Decimal>,
}

/// As cinco regras. O padrão tem todas ligadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthConfig {
    settings: [RuleSetting; 5],
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            settings: RuleId::ALL.map(|rule| RuleSetting {
                enabled: true,
                threshold: rule.default_threshold(),
            }),
        }
    }
}

impl HealthConfig {
    pub fn get(&self, rule: RuleId) -> RuleSetting {
        self.settings[rule as usize]
    }

    pub fn set(&mut self, rule: RuleId, setting: RuleSetting) {
        self.settings[rule as usize] = setting;
    }
}

/// Um ativo como a regra o enxerga.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthAsset {
    pub id: Uuid,
    /// Símbolo ou nome, para o aviso.
    pub label: String,
    pub asset_class: AssetClass,
    /// Valor atual em reais.
    pub value: Decimal,
    /// Setor da fonte pública; `None` quando não há.
    pub sector: Option<String>,
}

/// Sobre o que é o aviso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Asset {
        id: Uuid,
        label: String,
    },
    /// Os maiores, do maior para o menor.
    TopAssets {
        labels: Vec<String>,
    },
    Sector {
        class: AssetClass,
        sector: String,
    },
    Class(AssetClass),
}

/// Um aviso: o que é (`rule` + `subject`), o número de agora, o limite e,
/// quando dá para dizer, quanto de dinheiro novo fora dali traz o número de
/// volta ao limite.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub rule: RuleId,
    pub subject: Subject,
    /// Percentual ou quantidade, conforme [`RuleId::unit`].
    pub current: Decimal,
    pub limit: Decimal,
    /// Aporte (em reais) em outros ativos que faz o percentual voltar ao
    /// limite sem vender nada. `None` nas regras de contagem.
    pub needed_contribution: Option<Decimal>,
}

/// Como uma regra terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    /// Desligada pela pessoa.
    Off,
    /// Ligada, mas a carteira ainda não dá o que medir (poucos ativos, sem setor...).
    NotApplicable,
    Ok,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleReport {
    pub rule: RuleId,
    pub enabled: bool,
    pub threshold: Option<Decimal>,
    pub status: RuleStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// Uma linha por regra, na ordem de [`RuleId::ALL`].
    pub rules: Vec<RuleReport>,
    pub warnings: Vec<Warning>,
}

/// Roda as regras ligadas.
pub fn evaluate(config: &HealthConfig, assets: &[HealthAsset], targets: Targets) -> Evaluation {
    let mut owned: Vec<&HealthAsset> = assets.iter().filter(|a| a.value > Decimal::ZERO).collect();
    // Do maior para o menor; o desempate pelo nome mantém a saída estável.
    owned.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    let total: Decimal = owned.iter().map(|a| a.value).sum();

    let mut rules = Vec::with_capacity(RuleId::ALL.len());
    let mut warnings = Vec::new();
    for rule in RuleId::ALL {
        let setting = config.get(rule);
        let outcome = if setting.enabled {
            let limit = setting.threshold.or_else(|| rule.default_threshold());
            match rule {
                RuleId::MaxAsset => limit.and_then(|l| max_asset(&owned, total, l)),
                RuleId::TopFive => limit.and_then(|l| top_five(&owned, total, l)),
                RuleId::MaxSector => limit.and_then(|l| max_sector(&owned, l)),
                RuleId::MinAssets => limit.and_then(|l| min_assets(&owned, targets, l)),
                RuleId::EmptyClass => empty_class(&owned, targets),
            }
        } else {
            None
        };
        let status = match (&outcome, setting.enabled) {
            (_, false) => RuleStatus::Off,
            (None, true) => RuleStatus::NotApplicable,
            (Some(found), true) if found.is_empty() => RuleStatus::Ok,
            (Some(_), true) => RuleStatus::Warning,
        };
        rules.push(RuleReport {
            rule,
            enabled: setting.enabled,
            threshold: setting.threshold,
            status,
        });
        warnings.extend(outcome.unwrap_or_default());
    }
    Evaluation { rules, warnings }
}

/// Aporte em outros ativos para `part` deixar de passar de `limit`% de
/// `whole`: `part / (whole + x) = limit / 100`.
fn contribution_to_reach(part: Decimal, whole: Decimal, limit: Decimal) -> Decimal {
    let needed = part * HUNDRED / limit - whole;
    needed.max(Decimal::ZERO).round_dp(2)
}

fn max_asset(owned: &[&HealthAsset], total: Decimal, limit: Decimal) -> Option<Vec<Warning>> {
    if total <= Decimal::ZERO {
        return None;
    }
    Some(
        owned
            .iter()
            .filter_map(|asset| {
                let share = percent(asset.value, total);
                (share > limit).then(|| Warning {
                    rule: RuleId::MaxAsset,
                    subject: Subject::Asset {
                        id: asset.id,
                        label: asset.label.clone(),
                    },
                    current: share,
                    limit,
                    needed_contribution: Some(contribution_to_reach(asset.value, total, limit)),
                })
            })
            .collect(),
    )
}

fn top_five(owned: &[&HealthAsset], total: Decimal, limit: Decimal) -> Option<Vec<Warning>> {
    // Com cinco ativos ou menos os cinco maiores são a carteira toda (100%):
    // a regra não diz nada, quem fala é `min_assets`.
    if owned.len() <= TOP_COUNT || total <= Decimal::ZERO {
        return None;
    }
    let top = &owned[..TOP_COUNT];
    let sum: Decimal = top.iter().map(|a| a.value).sum();
    let share = percent(sum, total);
    if share <= limit {
        return Some(Vec::new());
    }
    Some(vec![Warning {
        rule: RuleId::TopFive,
        subject: Subject::TopAssets {
            labels: top.iter().map(|a| a.label.clone()).collect(),
        },
        current: share,
        limit,
        needed_contribution: Some(contribution_to_reach(sum, total, limit)),
    }])
}

fn max_sector(owned: &[&HealthAsset], limit: Decimal) -> Option<Vec<Warning>> {
    let mut evaluated = false;
    let mut found = Vec::new();
    for class in SECTOR_CLASSES {
        let group: Vec<&&HealthAsset> = owned.iter().filter(|a| a.asset_class == class).collect();
        // Um ativo só é sempre 100% de um setor: dizer isso não ajuda.
        if group.len() < 2 || group.iter().all(|a| a.sector.is_none()) {
            continue;
        }
        evaluated = true;
        let class_total: Decimal = group.iter().map(|a| a.value).sum();
        let mut by_sector: BTreeMap<&str, Decimal> = BTreeMap::new();
        for asset in &group {
            if let Some(sector) = asset.sector.as_deref() {
                *by_sector.entry(sector).or_default() += asset.value;
            }
        }
        for (sector, value) in by_sector {
            let share = percent(value, class_total);
            if share > limit {
                found.push(Warning {
                    rule: RuleId::MaxSector,
                    subject: Subject::Sector {
                        class,
                        sector: sector.to_owned(),
                    },
                    current: share,
                    limit,
                    needed_contribution: Some(contribution_to_reach(value, class_total, limit)),
                });
            }
        }
    }
    evaluated.then(|| {
        found.sort_by_key(|w| std::cmp::Reverse(w.current));
        found
    })
}

fn count_by_class(owned: &[&HealthAsset]) -> [usize; 7] {
    let mut counts = [0usize; 7];
    for asset in owned {
        counts[asset.asset_class as usize] += 1;
    }
    counts
}

fn min_assets(owned: &[&HealthAsset], targets: Targets, limit: Decimal) -> Option<Vec<Warning>> {
    let counts = count_by_class(owned);
    // Classe sem nenhum ativo é assunto de `empty_class`: aqui só a que já tem
    // algum e ainda é rasa.
    let candidates: Vec<AssetClass> = AssetClass::ALL
        .into_iter()
        .filter(|c| targets.get(*c) > 0 && counts[*c as usize] > 0)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    Some(
        candidates
            .into_iter()
            .filter(|c| Decimal::from(counts[*c as usize]) < limit)
            .map(|class| Warning {
                rule: RuleId::MinAssets,
                subject: Subject::Class(class),
                current: Decimal::from(counts[class as usize]),
                limit,
                needed_contribution: None,
            })
            .collect(),
    )
}

fn empty_class(owned: &[&HealthAsset], targets: Targets) -> Option<Vec<Warning>> {
    let counts = count_by_class(owned);
    let with_target: Vec<AssetClass> = AssetClass::ALL
        .into_iter()
        .filter(|c| targets.get(*c) > 0)
        .collect();
    if with_target.is_empty() {
        return None;
    }
    Some(
        with_target
            .into_iter()
            .filter(|c| counts[*c as usize] == 0)
            .map(|class| Warning {
                rule: RuleId::EmptyClass,
                subject: Subject::Class(class),
                current: Decimal::ZERO,
                limit: Decimal::ONE,
                needed_contribution: None,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn asset(label: &str, class: AssetClass, value: Decimal, sector: Option<&str>) -> HealthAsset {
        HealthAsset {
            id: Uuid::new_v4(),
            label: label.to_owned(),
            asset_class: class,
            value,
            sector: sector.map(str::to_owned),
        }
    }

    fn targets(pairs: &[(AssetClass, u32)]) -> Targets {
        let mut t = Targets::default();
        for (class, bp) in pairs {
            t.set(*class, *bp);
        }
        t
    }

    /// Só `rule` ligada, com `threshold` (ou o padrão dela).
    fn only(rule: RuleId, threshold: Option<Decimal>) -> HealthConfig {
        let mut config = HealthConfig::default();
        for other in RuleId::ALL {
            config.set(
                other,
                RuleSetting {
                    enabled: other == rule,
                    threshold: other.default_threshold(),
                },
            );
        }
        if let Some(value) = threshold {
            config.set(
                rule,
                RuleSetting {
                    enabled: true,
                    threshold: Some(value),
                },
            );
        }
        config
    }

    fn status(evaluation: &Evaluation, rule: RuleId) -> RuleStatus {
        evaluation
            .rules
            .iter()
            .find(|r| r.rule == rule)
            .map(|r| r.status)
            .unwrap()
    }

    #[test]
    fn rules_round_trip_and_have_defaults() {
        for rule in RuleId::ALL {
            assert_eq!(RuleId::parse(rule.as_str()), Some(rule));
            assert_eq!(
                serde_json::to_string(&rule).unwrap(),
                format!("\"{}\"", rule.as_str())
            );
            assert_eq!(rule.default_threshold().is_some(), rule.unit().is_some());
        }
        assert_eq!(RuleId::parse("nota"), None);
        let config = HealthConfig::default();
        assert!(RuleId::ALL.iter().all(|r| config.get(*r).enabled));
        assert_eq!(config.get(RuleId::MaxAsset).threshold, Some(dec!(15)));
    }

    #[test]
    fn thresholds_are_validated_per_unit() {
        assert_eq!(
            RuleId::MaxAsset.validate_threshold(dec!(12.50)),
            Ok(dec!(12.5))
        );
        assert_eq!(RuleId::TopFive.validate_threshold(dec!(100)), Ok(dec!(100)));
        for bad in [dec!(0), dec!(-1), dec!(100.01), dec!(10.123)] {
            assert!(RuleId::MaxSector.validate_threshold(bad).is_err(), "{bad}");
        }
        assert_eq!(RuleId::MinAssets.validate_threshold(dec!(3)), Ok(dec!(3)));
        assert_eq!(RuleId::MinAssets.validate_threshold(dec!(3.0)), Ok(dec!(3)));
        for bad in [dec!(0), dec!(2.5), dec!(51), dec!(-3)] {
            assert!(RuleId::MinAssets.validate_threshold(bad).is_err(), "{bad}");
        }
        assert!(RuleId::EmptyClass.validate_threshold(dec!(1)).is_err());
    }

    #[test]
    fn an_asset_above_the_limit_is_a_warning_with_the_number_and_the_fix() {
        let assets = vec![
            asset("AAAA3", AssetClass::BrStocks, dec!(300), None),
            asset("BBBB3", AssetClass::BrStocks, dec!(700), None),
        ];
        let evaluation = evaluate(
            &only(RuleId::MaxAsset, Some(dec!(50))),
            &assets,
            Targets::default(),
        );
        assert_eq!(evaluation.warnings.len(), 1);
        let warning = &evaluation.warnings[0];
        assert_eq!(warning.rule, RuleId::MaxAsset);
        assert!(matches!(&warning.subject, Subject::Asset { label, .. } if label == "BBBB3"));
        assert_eq!(warning.current, dec!(70));
        assert_eq!(warning.limit, dec!(50));
        // 700 / (1000 + x) = 0,5  =>  x = 400.
        assert_eq!(warning.needed_contribution, Some(dec!(400)));
        assert_eq!(status(&evaluation, RuleId::MaxAsset), RuleStatus::Warning);
        assert_eq!(status(&evaluation, RuleId::TopFive), RuleStatus::Off);
    }

    #[test]
    fn exactly_at_the_limit_is_fine() {
        let assets = vec![
            asset("AAAA3", AssetClass::BrStocks, dec!(500), None),
            asset("BBBB3", AssetClass::BrStocks, dec!(500), None),
        ];
        let evaluation = evaluate(
            &only(RuleId::MaxAsset, Some(dec!(50))),
            &assets,
            Targets::default(),
        );
        assert!(evaluation.warnings.is_empty());
        assert_eq!(status(&evaluation, RuleId::MaxAsset), RuleStatus::Ok);
    }

    #[test]
    fn an_empty_portfolio_has_nothing_to_measure() {
        let evaluation = evaluate(&HealthConfig::default(), &[], Targets::default());
        assert!(evaluation.warnings.is_empty());
        assert_eq!(evaluation.rules.len(), 5);
        for rule in RuleId::ALL {
            assert_eq!(status(&evaluation, rule), RuleStatus::NotApplicable);
        }
        // Ativo cadastrado e ainda não comprado não conta.
        let wishlist = vec![asset("AAAA3", AssetClass::BrStocks, dec!(0), None)];
        let evaluation = evaluate(&HealthConfig::default(), &wishlist, Targets::default());
        assert_eq!(
            status(&evaluation, RuleId::MaxAsset),
            RuleStatus::NotApplicable
        );
    }

    #[test]
    fn top_five_needs_more_than_five_assets() {
        let mut assets: Vec<HealthAsset> = (0..5)
            .map(|i| asset(&format!("T{i}"), AssetClass::BrStocks, dec!(100), None))
            .collect();
        let config = only(RuleId::TopFive, Some(dec!(60)));
        let evaluation = evaluate(&config, &assets, Targets::default());
        assert_eq!(
            status(&evaluation, RuleId::TopFive),
            RuleStatus::NotApplicable
        );

        // Sexto ativo pequeno: os cinco maiores seguem com ~98%.
        assets.push(asset("SMALL", AssetClass::BrStocks, dec!(10), None));
        let evaluation = evaluate(&config, &assets, Targets::default());
        assert_eq!(evaluation.warnings.len(), 1);
        let warning = &evaluation.warnings[0];
        assert_eq!(warning.current, dec!(98.04));
        assert!(
            matches!(&warning.subject, Subject::TopAssets { labels } if labels.len() == 5 && !labels.contains(&"SMALL".to_owned()))
        );
        // 500 / (510 + x) = 0,6  =>  x = 323,33...
        assert_eq!(warning.needed_contribution, Some(dec!(323.33)));

        // Carteira bem espalhada: dentro do limite.
        let spread: Vec<HealthAsset> = (0..10)
            .map(|i| asset(&format!("S{i}"), AssetClass::BrStocks, dec!(100), None))
            .collect();
        let evaluation = evaluate(&config, &spread, Targets::default());
        assert_eq!(status(&evaluation, RuleId::TopFive), RuleStatus::Ok);
    }

    #[test]
    fn sector_concentration_is_measured_inside_stocks_and_inside_reits() {
        let assets = vec![
            asset("ITUB4", AssetClass::BrStocks, dec!(300), Some("Financeiro")),
            asset("BBAS3", AssetClass::BrStocks, dec!(300), Some("Financeiro")),
            asset(
                "WEGE3",
                AssetClass::BrStocks,
                dec!(400),
                Some("Bens Industriais"),
            ),
            asset("KNRI11", AssetClass::BrReits, dec!(500), Some("Tijolo")),
            asset("HGLG11", AssetClass::BrReits, dec!(500), Some("Tijolo")),
            // Classes sem fonte de setor nunca entram.
            asset(
                "AAPL",
                AssetClass::IntlStocks,
                dec!(5000),
                Some("Tecnologia"),
            ),
        ];
        let evaluation = evaluate(
            &only(RuleId::MaxSector, Some(dec!(50))),
            &assets,
            Targets::default(),
        );
        assert_eq!(evaluation.warnings.len(), 2);
        // O maior primeiro: 100% de Tijolo nos FIIs, depois 60% de Financeiro.
        let first = &evaluation.warnings[0];
        assert!(
            matches!(&first.subject, Subject::Sector { class: AssetClass::BrReits, sector } if sector == "Tijolo")
        );
        assert_eq!(first.current, dec!(100));
        let second = &evaluation.warnings[1];
        assert!(
            matches!(&second.subject, Subject::Sector { class: AssetClass::BrStocks, sector } if sector == "Financeiro")
        );
        assert_eq!(second.current, dec!(60));
        // 600 / (1000 + x) = 0,5  =>  x = 200.
        assert_eq!(second.needed_contribution, Some(dec!(200)));
    }

    #[test]
    fn sector_rule_skips_single_asset_groups_and_unclassified_groups() {
        let one = vec![asset(
            "ITUB4",
            AssetClass::BrStocks,
            dec!(300),
            Some("Financeiro"),
        )];
        let evaluation = evaluate(&only(RuleId::MaxSector, None), &one, Targets::default());
        assert_eq!(
            status(&evaluation, RuleId::MaxSector),
            RuleStatus::NotApplicable
        );

        let unknown = vec![
            asset("AAAA3", AssetClass::BrStocks, dec!(300), None),
            asset("BBBB3", AssetClass::BrStocks, dec!(300), None),
        ];
        let evaluation = evaluate(&only(RuleId::MaxSector, None), &unknown, Targets::default());
        assert_eq!(
            status(&evaluation, RuleId::MaxSector),
            RuleStatus::NotApplicable
        );
        assert!(evaluation.warnings.is_empty());
    }

    #[test]
    fn unclassified_assets_dilute_the_sector_share() {
        let assets = vec![
            asset("ITUB4", AssetClass::BrStocks, dec!(400), Some("Financeiro")),
            asset("AAAA3", AssetClass::BrStocks, dec!(600), None),
        ];
        let evaluation = evaluate(
            &only(RuleId::MaxSector, Some(dec!(50))),
            &assets,
            Targets::default(),
        );
        assert!(evaluation.warnings.is_empty());
        assert_eq!(status(&evaluation, RuleId::MaxSector), RuleStatus::Ok);
    }

    #[test]
    fn shallow_and_empty_classes_are_separate_warnings() {
        let assets = vec![
            asset("KNRI11", AssetClass::BrReits, dec!(500), None),
            asset("ITUB4", AssetClass::BrStocks, dec!(500), None),
            asset("BBAS3", AssetClass::BrStocks, dec!(500), None),
            asset("WEGE3", AssetClass::BrStocks, dec!(500), None),
            // Sem valor: não conta como ativo da classe.
            asset("BTC", AssetClass::Crypto, dec!(0), None),
        ];
        let goals = targets(&[
            (AssetClass::BrStocks, 4000),
            (AssetClass::BrReits, 4000),
            (AssetClass::Crypto, 1000),
            (AssetClass::FixedIncome, 1000),
        ]);

        let shallow = evaluate(&only(RuleId::MinAssets, Some(dec!(3))), &assets, goals);
        assert_eq!(shallow.warnings.len(), 1);
        assert_eq!(
            shallow.warnings[0].subject,
            Subject::Class(AssetClass::BrReits)
        );
        assert_eq!(shallow.warnings[0].current, dec!(1));
        assert_eq!(shallow.warnings[0].limit, dec!(3));
        assert_eq!(shallow.warnings[0].needed_contribution, None);

        let empty = evaluate(&only(RuleId::EmptyClass, None), &assets, goals);
        let classes: Vec<_> = empty.warnings.iter().map(|w| w.subject.clone()).collect();
        assert_eq!(
            classes,
            vec![
                Subject::Class(AssetClass::Crypto),
                Subject::Class(AssetClass::FixedIncome)
            ]
        );
        assert_eq!(empty.warnings[0].current, dec!(0));
        assert_eq!(empty.warnings[0].limit, dec!(1));
    }

    #[test]
    fn a_class_without_target_is_never_flagged() {
        let assets = vec![asset("KNRI11", AssetClass::BrReits, dec!(500), None)];
        let evaluation = evaluate(&HealthConfig::default(), &assets, Targets::default());
        assert!(evaluation
            .warnings
            .iter()
            .all(|w| !matches!(w.rule, RuleId::MinAssets | RuleId::EmptyClass)));
        assert_eq!(
            status(&evaluation, RuleId::EmptyClass),
            RuleStatus::NotApplicable
        );
        assert_eq!(
            status(&evaluation, RuleId::MinAssets),
            RuleStatus::NotApplicable
        );
    }

    #[test]
    fn disabled_rules_stay_silent_and_say_so() {
        let assets = vec![asset("AAAA3", AssetClass::BrStocks, dec!(1000), None)];
        let mut config = HealthConfig::default();
        config.set(
            RuleId::MaxAsset,
            RuleSetting {
                enabled: false,
                threshold: Some(dec!(10)),
            },
        );
        let evaluation = evaluate(&config, &assets, Targets::default());
        assert!(evaluation
            .warnings
            .iter()
            .all(|w| w.rule != RuleId::MaxAsset));
        let report = evaluation
            .rules
            .iter()
            .find(|r| r.rule == RuleId::MaxAsset)
            .unwrap();
        assert_eq!(report.status, RuleStatus::Off);
        assert!(!report.enabled);
        assert_eq!(report.threshold, Some(dec!(10)));
    }

    fn values() -> impl Strategy<Value = Vec<i64>> {
        proptest::collection::vec(0i64..5_000_000, 0..14)
    }

    fn portfolio(values: &[i64]) -> Vec<HealthAsset> {
        values
            .iter()
            .enumerate()
            .map(|(i, cents)| {
                let sector = match i % 3 {
                    0 => Some("Financeiro"),
                    1 => Some("Saude"),
                    _ => None,
                };
                asset(
                    &format!("A{i:02}"),
                    if i % 2 == 0 {
                        AssetClass::BrStocks
                    } else {
                        AssetClass::BrReits
                    },
                    Decimal::new(*cents, 2),
                    sector,
                )
            })
            .collect()
    }

    proptest! {
        /// `max_asset` avisa exatamente os ativos cuja fatia passa do limite.
        #[test]
        fn max_asset_flags_exactly_the_assets_over_the_limit(
            values in values(),
            limit in 1u32..=100,
        ) {
            let assets = portfolio(&values);
            let limit = Decimal::from(limit);
            let evaluation = evaluate(&only(RuleId::MaxAsset, Some(limit)), &assets, Targets::default());
            let total: Decimal = assets.iter().map(|a| a.value).sum();
            let flagged: Vec<Uuid> = evaluation.warnings.iter().filter_map(|w| match &w.subject {
                Subject::Asset { id, .. } => Some(*id),
                _ => None,
            }).collect();
            for a in &assets {
                let over = a.value > Decimal::ZERO && percent(a.value, total) > limit;
                prop_assert_eq!(flagged.contains(&a.id), over);
            }
        }

        /// Subir o limite nunca cria aviso novo.
        #[test]
        fn a_looser_limit_never_adds_warnings(
            values in values(),
            low in 1u32..=100,
            extra in 0u32..=99,
        ) {
            let assets = portfolio(&values);
            let high = (low + extra).min(100);
            for rule in [RuleId::MaxAsset, RuleId::TopFive, RuleId::MaxSector] {
                let tight = evaluate(&only(rule, Some(Decimal::from(low))), &assets, Targets::default());
                let loose = evaluate(&only(rule, Some(Decimal::from(high))), &assets, Targets::default());
                prop_assert!(loose.warnings.len() <= tight.warnings.len());
            }
        }

        /// O aporte sugerido leva o percentual de volta ao limite (até o
        /// arredondamento de centavos).
        #[test]
        fn the_suggested_contribution_clears_the_warning(
            values in values(),
            limit in 1u32..=99,
        ) {
            let assets = portfolio(&values);
            let limit = Decimal::from(limit);
            let total: Decimal = assets.iter().map(|a| a.value).sum();
            let evaluation = evaluate(&only(RuleId::MaxAsset, Some(limit)), &assets, Targets::default());
            for w in &evaluation.warnings {
                let Subject::Asset { id, .. } = &w.subject else { continue };
                let value = assets.iter().find(|a| a.id == *id).unwrap().value;
                let needed = w.needed_contribution.unwrap();
                prop_assert!(needed >= Decimal::ZERO);
                // Um centavo a mais de aporte já deixa a fatia no limite.
                let after = percent(value, total + needed + dec!(0.01));
                prop_assert!(after <= limit, "{after} > {limit}");
            }
        }

        /// Regra desligada nunca avisa, seja qual for a carteira.
        #[test]
        fn disabled_rules_never_warn(values in values()) {
            let assets = portfolio(&values);
            let mut config = HealthConfig::default();
            for rule in RuleId::ALL {
                config.set(rule, RuleSetting { enabled: false, threshold: rule.default_threshold() });
            }
            let evaluation = evaluate(&config, &assets, Targets::default());
            prop_assert!(evaluation.warnings.is_empty());
            prop_assert!(evaluation.rules.iter().all(|r| r.status == RuleStatus::Off));
        }

        /// A ordem e o conjunto das regras do relatório são sempre os mesmos.
        #[test]
        fn the_report_always_lists_every_rule_in_order(values in values()) {
            let evaluation = evaluate(&HealthConfig::default(), &portfolio(&values), Targets::default());
            let ids: Vec<RuleId> = evaluation.rules.iter().map(|r| r.rule).collect();
            prop_assert_eq!(ids, RuleId::ALL.to_vec());
        }
    }
}
