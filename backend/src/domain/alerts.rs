//! Alertas fora do app: que tipos existem, o que vale um aviso e como o resumo
//! diário é escrito. Regra pura: sem banco, sem rede, sem relógio.
//!
//! O resumo sai uma vez por dia e só quando há algo a dizer. Dois jeitos de um
//! tipo entrar nele:
//!
//! - **por data** (`income_tomorrow`): o provento que cai amanhã;
//! - **por transição** (os demais): só o que passou a valer desde a última
//!   apuração. Uma classe que já estava fora da meta ontem não volta a ser
//!   avisada hoje; se sair e voltar, avisa de novo. É o que impede o spam.
//!
//! O texto é gerado no servidor e atravessa log: vai sem acento. O que
//! `hide_values` esconde está em [`render_digest`].

use std::collections::HashSet;

use rust_decimal::Decimal;
use time::{Date, Time};
use uuid::Uuid;

use super::asset_class::AssetClass;
use super::health::{RuleId, Subject, Warning};
use super::income::ObjectiveKind;

/// Limite do Telegram por mensagem é 4096; fica folga para o rodapé.
const MESSAGE_BUDGET: usize = 3800;

/// Horário padrão do resumo (Brasília).
pub const DEFAULT_SEND_AT: Time = time::macros::time!(08:00);

/// Quanto tempo vale um código de vinculação.
pub const LINK_CODE_TTL_MINUTES: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlertKind {
    /// Provento que cai amanhã.
    IncomeTomorrow,
    /// Classe X pontos percentuais fora da meta.
    ClassDrift,
    /// Ativo voltou a custar menos que o próprio teto de preço.
    PriceBelowCeiling,
    /// Objetivo atingido.
    ObjectiveAchieved,
    /// Isentômetro de ações acima de X% do limite mensal.
    TaxExemption,
    /// Aviso novo de saúde da carteira.
    HealthWarning,
}

impl AlertKind {
    /// A ordem da tela e do resumo.
    pub const ALL: [AlertKind; 6] = [
        AlertKind::IncomeTomorrow,
        AlertKind::ClassDrift,
        AlertKind::PriceBelowCeiling,
        AlertKind::ObjectiveAchieved,
        AlertKind::TaxExemption,
        AlertKind::HealthWarning,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AlertKind::IncomeTomorrow => "income_tomorrow",
            AlertKind::ClassDrift => "class_drift",
            AlertKind::PriceBelowCeiling => "price_below_ceiling",
            AlertKind::ObjectiveAchieved => "objective_achieved",
            AlertKind::TaxExemption => "tax_exemption",
            AlertKind::HealthWarning => "health_warning",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        AlertKind::ALL.into_iter().find(|k| k.as_str() == value)
    }

    /// `true` nos tipos que só avisam o que mudou desde a última apuração.
    pub fn is_transition(self) -> bool {
        self != AlertKind::IncomeTomorrow
    }

    /// Se o servidor já sabe apurar este tipo. O isentômetro depende de ligar o
    /// livro de lançamentos ao imposto (`domain/tax`), que é outra frente: até
    /// lá o tipo existe na tela, desligado e sem poder ser ligado.
    pub fn is_available(self) -> bool {
        self != AlertKind::TaxExemption
    }

    /// O limite de fábrica (em pontos percentuais ou % do limite), se o tipo tem.
    pub fn default_threshold(self) -> Option<Decimal> {
        match self {
            AlertKind::ClassDrift => Some(Decimal::from(5)),
            AlertKind::TaxExemption => Some(Decimal::from(80)),
            _ => None,
        }
    }

    /// Confere o limite que a pessoa mandou: de 0 (exclusive) a 100, até duas casas.
    pub fn validate_threshold(self, value: Decimal) -> Result<Decimal, String> {
        if self.default_threshold().is_none() {
            return Err("Esse alerta nao tem limite.".to_owned());
        }
        if value > Decimal::ZERO && value <= Decimal::ONE_HUNDRED && value.normalize().scale() <= 2
        {
            Ok(value.normalize())
        } else {
            Err(
                "O limite precisa ser maior que 0 e ate 100, com ate duas casas decimais."
                    .to_owned(),
            )
        }
    }
}

/// Um tipo como a pessoa o deixou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSetting {
    pub enabled: bool,
    pub threshold: Option<Decimal>,
}

/// Os seis tipos de uma pessoa. O padrão liga tudo o que o servidor sabe apurar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertConfig {
    settings: [TypeSetting; 6],
}

impl Default for AlertConfig {
    fn default() -> Self {
        let mut settings = [TypeSetting {
            enabled: false,
            threshold: None,
        }; 6];
        for kind in AlertKind::ALL {
            settings[kind as usize] = TypeSetting {
                enabled: kind.is_available(),
                threshold: kind.default_threshold(),
            };
        }
        Self { settings }
    }
}

impl AlertConfig {
    pub fn get(&self, kind: AlertKind) -> TypeSetting {
        self.settings[kind as usize]
    }

    pub fn set(&mut self, kind: AlertKind, setting: TypeSetting) {
        self.settings[kind as usize] = setting;
    }

    /// Os tipos ligados que o servidor sabe apurar.
    pub fn active(&self) -> impl Iterator<Item = AlertKind> + '_ {
        AlertKind::ALL
            .into_iter()
            .filter(|k| k.is_available() && self.get(*k).enabled)
    }
}

/// `HH:MM`, de 00:00 a 23:59.
pub fn parse_send_at(text: &str) -> Option<Time> {
    let (hour, minute) = text.split_once(':')?;
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    Time::from_hms(hour.parse().ok()?, minute.parse().ok()?, 0).ok()
}

pub fn format_send_at(at: Time) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

// ------------------------------------------------------------------ itens --

/// O que o resumo pode dizer.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    IncomeTomorrow {
        label: String,
        /// Líquido (depois do IR retido).
        net: Decimal,
    },
    ClassDrift {
        class: AssetClass,
        current: Decimal,
        target: Decimal,
    },
    PriceBelowCeiling {
        asset_id: Uuid,
        label: String,
        price: Decimal,
        ceiling: Decimal,
    },
    ObjectiveAchieved {
        id: Uuid,
        kind: ObjectiveKind,
        target: Decimal,
    },
    Health(Warning),
}

impl Item {
    pub fn kind(&self) -> AlertKind {
        match self {
            Item::IncomeTomorrow { .. } => AlertKind::IncomeTomorrow,
            Item::ClassDrift { .. } => AlertKind::ClassDrift,
            Item::PriceBelowCeiling { .. } => AlertKind::PriceBelowCeiling,
            Item::ObjectiveAchieved { .. } => AlertKind::ObjectiveAchieved,
            Item::Health(_) => AlertKind::HealthWarning,
        }
    }

    /// A identidade do item dentro do tipo, para saber se já foi avisado.
    pub fn key(&self) -> String {
        match self {
            Item::IncomeTomorrow { label, .. } => format!("income:{label}"),
            Item::ClassDrift { class, .. } => format!("class:{}", class.as_str()),
            Item::PriceBelowCeiling { asset_id, .. } => format!("asset:{asset_id}"),
            Item::ObjectiveAchieved { id, .. } => format!("objective:{id}"),
            Item::Health(warning) => format!(
                "{}:{}",
                warning.rule.as_str(),
                match &warning.subject {
                    Subject::Asset { id, .. } => format!("asset:{id}"),
                    Subject::TopAssets { .. } => "top".to_owned(),
                    Subject::Sector { class, sector } => {
                        format!("sector:{}:{sector}", class.as_str())
                    }
                    Subject::Class(class) => format!("class:{}", class.as_str()),
                }
            ),
        }
    }
}

/// Uma classe da carteira como a apuração de desvio a enxerga.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassShare {
    pub class: AssetClass,
    /// Meta, em % da carteira.
    pub target: Decimal,
    /// Fatia de agora, em % da carteira.
    pub current: Decimal,
}

/// As classes que estão a `limit` pontos percentuais ou mais da meta. Sem valor
/// na carteira (`total_is_zero`) nada está fora de meta: está vazia.
pub fn class_drifts(shares: &[ClassShare], limit: Decimal, total_is_zero: bool) -> Vec<Item> {
    if total_is_zero {
        return Vec::new();
    }
    shares
        .iter()
        .filter(|s| (s.current - s.target).abs() >= limit)
        .map(|s| Item::ClassDrift {
            class: s.class,
            current: s.current,
            target: s.target,
        })
        .collect()
}

/// Um ativo com teto de preço.
#[derive(Debug, Clone, PartialEq)]
pub struct CeilingAsset {
    pub asset_id: Uuid,
    pub label: String,
    /// Preço de agora, em reais; `None` sem cotação.
    pub price: Option<Decimal>,
    pub ceiling: Option<Decimal>,
}

/// Os ativos compráveis pelo teto: preço de agora no teto ou abaixo dele (o
/// motor do aporte só tira o ativo quando o preço passa do teto).
pub fn below_ceiling(assets: &[CeilingAsset]) -> Vec<Item> {
    assets
        .iter()
        .filter_map(|a| {
            let (price, ceiling) = (a.price?, a.ceiling?);
            (price > Decimal::ZERO && price <= ceiling).then(|| Item::PriceBelowCeiling {
                asset_id: a.asset_id,
                label: a.label.clone(),
                price,
                ceiling,
            })
        })
        .collect()
}

/// Do que está valendo agora, o que ainda não foi avisado.
pub fn new_items(active: Vec<Item>, seen: &HashSet<String>) -> Vec<Item> {
    active
        .into_iter()
        .filter(|item| !seen.contains(&item.key()))
        .collect()
}

// ----------------------------------------------------------------- resumo --

/// `R$ 1.234,56`.
fn brl(value: Decimal) -> String {
    let rounded = value.round_dp(2);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let text = format!("{:.2}", rounded.abs());
    let (whole, cents) = text.split_once('.').unwrap_or((text.as_str(), "00"));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    format!("{}R$ {grouped},{cents}", if negative { "-" } else { "" })
}

/// `12,5` (sem zeros à direita).
fn number(value: Decimal) -> String {
    value.round_dp(2).normalize().to_string().replace('.', ",")
}

fn pct(value: Decimal) -> String {
    format!("{}%", number(value))
}

fn describe(item: &Item, hide_values: bool) -> String {
    match item {
        Item::IncomeTomorrow { label, net } => {
            if hide_values {
                format!("{label} paga amanha.")
            } else {
                format!("{label} paga amanha: {} liquidos.", brl(*net))
            }
        }
        Item::ClassDrift {
            class,
            current,
            target,
        } => {
            let gap = (*current - *target).abs();
            let side = if current > target { "acima" } else { "abaixo" };
            format!(
                "{} esta com {} da carteira (meta {}), {} p.p. {side}.",
                class.label(),
                pct(*current),
                pct(*target),
                number(gap)
            )
        }
        Item::PriceBelowCeiling {
            label,
            price,
            ceiling,
            ..
        } => {
            if hide_values {
                format!("{label} voltou a ficar no seu teto de preco ou abaixo.")
            } else {
                format!(
                    "{label} voltou a ficar no seu teto de preco: {} (teto {}).",
                    brl(*price),
                    brl(*ceiling)
                )
            }
        }
        Item::ObjectiveAchieved { kind, target, .. } => {
            let what = match kind {
                ObjectiveKind::MonthlyIncome => "renda mensal",
                ObjectiveKind::NetWorth => "patrimonio",
                ObjectiveKind::ClassValue => "valor da classe",
            };
            if hide_values {
                format!("Objetivo de {what} atingido.")
            } else {
                format!("Objetivo de {what} atingido: {}.", brl(*target))
            }
        }
        Item::Health(warning) => describe_health(warning, hide_values),
    }
}

fn describe_health(warning: &Warning, hide_values: bool) -> String {
    let current = number(warning.current);
    let limit = number(warning.limit);
    let mut text = match (&warning.rule, &warning.subject) {
        (RuleId::MaxAsset, Subject::Asset { label, .. }) => {
            format!("{label} esta com {current}% da carteira (limite {limit}%).")
        }
        (RuleId::TopFive, Subject::TopAssets { labels }) => format!(
            "Os 5 maiores ativos ({}) somam {current}% da carteira (limite {limit}%).",
            labels.join(", ")
        ),
        (RuleId::MaxSector, Subject::Sector { class, sector }) => format!(
            "O setor {sector} tem {current}% de {} (limite {limit}%).",
            class.label()
        ),
        (RuleId::MinAssets, Subject::Class(class)) => format!(
            "{} tem {current} ativo(s); o minimo que voce definiu e {limit}.",
            class.label()
        ),
        (RuleId::EmptyClass, Subject::Class(class)) => {
            format!("{} tem meta e nenhum ativo.", class.label())
        }
        // Combinação que a regra não produz: o texto genérico não perde o aviso.
        (rule, _) => format!(
            "Aviso de saude ({}): {current} contra o limite {limit}.",
            rule.as_str()
        ),
    };
    if let (Some(needed), false) = (warning.needed_contribution, hide_values) {
        if needed > Decimal::ZERO {
            text.push_str(&format!(
                " Um aporte de cerca de {} em outros ativos volta ao limite.",
                brl(needed)
            ));
        }
    }
    text
}

/// A mensagem do dia, ou `None` sem nada a dizer. Com `hide_values`, nenhum
/// valor em reais: só nomes e percentuais (o teto de preço e o aporte
/// necessário somem; o provento diz só quem paga).
pub fn render_digest(today: Date, items: &[Item], hide_values: bool) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let mut text = format!(
        "Prumo - resumo de {:02}/{:02}",
        today.day(),
        u8::from(today.month())
    );
    let mut used = text.len();
    let mut shown = 0usize;
    'sections: for kind in AlertKind::ALL {
        let lines: Vec<String> = items
            .iter()
            .filter(|i| i.kind() == kind)
            .map(|i| format!("- {}", describe(i, hide_values)))
            .collect();
        if lines.is_empty() {
            continue;
        }
        let title = format!("\n\n{}", section_title(kind));
        let mut titled = false;
        for line in lines {
            let extra = line.len() + 1 + if titled { 0 } else { title.len() };
            if used + extra > MESSAGE_BUDGET {
                break 'sections;
            }
            if !titled {
                text.push_str(&title);
                titled = true;
            }
            used += extra;
            text.push('\n');
            text.push_str(&line);
            shown += 1;
        }
    }
    if shown < items.len() {
        text.push_str(&format!(
            "\n\n... e mais {} aviso(s). Abra o Prumo para ver tudo.",
            items.len() - shown
        ));
    }
    Some(text)
}

fn section_title(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::IncomeTomorrow => "Proventos amanha",
        AlertKind::ClassDrift => "Classes fora da meta",
        AlertKind::PriceBelowCeiling => "No seu teto de preco",
        AlertKind::ObjectiveAchieved => "Objetivos atingidos",
        AlertKind::TaxExemption => "Isentometro",
        AlertKind::HealthWarning => "Saude da carteira",
    }
}

/// A mensagem do botão "Enviar teste agora": diz que o canal funciona e o que
/// virá, sem dado nenhum da carteira.
pub fn test_message(send_at: Time) -> String {
    format!(
        "Prumo: teste de alerta. Este chat esta vinculado; o resumo diario chega as {} (horario de Brasilia), so quando houver algo a dizer.",
        format_send_at(send_at)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn warning(rule: RuleId, subject: Subject, current: Decimal, limit: Decimal) -> Warning {
        Warning {
            rule,
            subject,
            current,
            limit,
            needed_contribution: Some(dec!(1234.5)),
        }
    }

    fn one_of_each() -> Vec<Item> {
        vec![
            Item::IncomeTomorrow {
                label: "KNRI11".into(),
                net: dec!(90),
            },
            Item::ClassDrift {
                class: AssetClass::BrReits,
                current: dec!(28.4),
                target: dec!(20),
            },
            Item::PriceBelowCeiling {
                asset_id: id(1),
                label: "ITSA4".into(),
                price: dec!(9.5),
                ceiling: dec!(10),
            },
            Item::ObjectiveAchieved {
                id: id(2),
                kind: ObjectiveKind::MonthlyIncome,
                target: dec!(3000),
            },
            Item::Health(warning(
                RuleId::MaxAsset,
                Subject::Asset {
                    id: id(3),
                    label: "KNRI11".into(),
                },
                dec!(23.5),
                dec!(15),
            )),
        ]
    }

    #[test]
    fn kinds_round_trip_and_the_exemption_is_not_available_yet() {
        for kind in AlertKind::ALL {
            assert_eq!(AlertKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(AlertKind::parse("outro"), None);
        assert!(!AlertKind::TaxExemption.is_available());
        assert!(AlertKind::ALL
            .into_iter()
            .filter(|k| *k != AlertKind::TaxExemption)
            .all(AlertKind::is_available));
        assert!(!AlertKind::IncomeTomorrow.is_transition());
        assert!(AlertKind::HealthWarning.is_transition());
    }

    #[test]
    fn the_default_config_turns_on_what_the_server_can_measure() {
        let config = AlertConfig::default();
        let active: Vec<_> = config.active().collect();
        assert_eq!(
            active,
            [
                AlertKind::IncomeTomorrow,
                AlertKind::ClassDrift,
                AlertKind::PriceBelowCeiling,
                AlertKind::ObjectiveAchieved,
                AlertKind::HealthWarning,
            ]
        );
        assert_eq!(config.get(AlertKind::ClassDrift).threshold, Some(dec!(5)));
        assert!(!config.get(AlertKind::TaxExemption).enabled);
        assert_eq!(
            config.get(AlertKind::TaxExemption).threshold,
            Some(dec!(80))
        );
        // Ligado na mão, mas sem apuração no servidor: nunca entra.
        let mut forced = config;
        forced.set(
            AlertKind::TaxExemption,
            TypeSetting {
                enabled: true,
                threshold: Some(dec!(80)),
            },
        );
        assert!(!forced.active().any(|k| k == AlertKind::TaxExemption));
    }

    #[test]
    fn thresholds_are_validated() {
        let drift = AlertKind::ClassDrift;
        assert_eq!(drift.validate_threshold(dec!(7.50)), Ok(dec!(7.5)));
        assert_eq!(drift.validate_threshold(dec!(100)), Ok(dec!(100)));
        for bad in [dec!(0), dec!(-1), dec!(100.01), dec!(1.234)] {
            assert!(drift.validate_threshold(bad).is_err(), "{bad}");
        }
        assert!(AlertKind::HealthWarning
            .validate_threshold(dec!(5))
            .is_err());
    }

    #[test]
    fn send_at_is_hh_mm() {
        assert_eq!(parse_send_at("08:00"), Some(time::macros::time!(08:00)));
        assert_eq!(parse_send_at("23:59"), Some(time::macros::time!(23:59)));
        for bad in ["8:00", "24:00", "08:60", "0800", "08:00:00", "ab:cd", ""] {
            assert_eq!(parse_send_at(bad), None, "{bad}");
        }
        assert_eq!(format_send_at(time::macros::time!(07:05)), "07:05");
    }

    #[test]
    fn money_and_numbers_use_the_brazilian_format() {
        assert_eq!(brl(dec!(1234.5)), "R$ 1.234,50");
        assert_eq!(brl(dec!(0.126)), "R$ 0,13");
        assert_eq!(brl(dec!(1234567.891)), "R$ 1.234.567,89");
        assert_eq!(brl(dec!(-12.3)), "-R$ 12,30");
        assert_eq!(brl(dec!(999)), "R$ 999,00");
        assert_eq!(number(dec!(8.40)), "8,4");
        assert_eq!(number(dec!(20.00)), "20");
        assert_eq!(pct(dec!(28.4)), "28,4%");
    }

    #[test]
    fn class_drift_uses_the_limit_inclusive() {
        let shares = [
            ClassShare {
                class: AssetClass::BrStocks,
                target: dec!(50),
                current: dec!(44.9),
            },
            ClassShare {
                class: AssetClass::BrReits,
                target: dec!(30),
                current: dec!(35),
            },
            ClassShare {
                class: AssetClass::Crypto,
                target: dec!(0),
                current: dec!(0),
            },
        ];
        let found = class_drifts(&shares, dec!(5), false);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key(), "class:br_stocks");
        assert_eq!(found[1].key(), "class:br_reits");
        // Carteira sem valor: não há o que desviar.
        assert!(class_drifts(&shares, dec!(5), true).is_empty());
    }

    #[test]
    fn below_ceiling_needs_price_and_ceiling_and_accepts_equal() {
        let asset = |price, ceiling| CeilingAsset {
            asset_id: id(7),
            label: "ITSA4".into(),
            price,
            ceiling,
        };
        let found = below_ceiling(&[
            asset(Some(dec!(10)), Some(dec!(10))),
            asset(Some(dec!(10.01)), Some(dec!(10))),
            asset(None, Some(dec!(10))),
            asset(Some(dec!(5)), None),
            asset(Some(dec!(0)), Some(dec!(10))),
        ]);
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0], Item::PriceBelowCeiling { price, .. } if *price == dec!(10)));
    }

    #[test]
    fn only_what_was_not_seen_is_new() {
        let items = one_of_each();
        let seen: HashSet<String> = [items[1].key(), items[2].key()].into_iter().collect();
        let fresh = new_items(items.clone(), &seen);
        assert_eq!(fresh.len(), 3);
        assert!(fresh.iter().all(|i| !seen.contains(&i.key())));
    }

    #[test]
    fn health_keys_tell_subjects_apart() {
        let a = Item::Health(warning(
            RuleId::MaxAsset,
            Subject::Asset {
                id: id(1),
                label: "A".into(),
            },
            dec!(20),
            dec!(15),
        ));
        let b = Item::Health(warning(
            RuleId::MaxAsset,
            Subject::Asset {
                id: id(2),
                label: "B".into(),
            },
            dec!(20),
            dec!(15),
        ));
        let top = Item::Health(warning(
            RuleId::TopFive,
            Subject::TopAssets {
                labels: vec!["A".into()],
            },
            dec!(70),
            dec!(60),
        ));
        let sector = Item::Health(warning(
            RuleId::MaxSector,
            Subject::Sector {
                class: AssetClass::BrStocks,
                sector: "Financeiro".into(),
            },
            dec!(50),
            dec!(40),
        ));
        let keys: HashSet<String> = [&a, &b, &top, &sector].iter().map(|i| i.key()).collect();
        assert_eq!(keys.len(), 4);
        // O mesmo aviso com número diferente continua sendo o mesmo aviso.
        let a2 = Item::Health(warning(
            RuleId::MaxAsset,
            Subject::Asset {
                id: id(1),
                label: "A".into(),
            },
            dec!(31),
            dec!(15),
        ));
        assert_eq!(a.key(), a2.key());
    }

    #[test]
    fn the_digest_is_silent_without_items() {
        assert_eq!(render_digest(date!(2026 - 11 - 25), &[], false), None);
    }

    #[test]
    fn the_digest_with_values_says_the_numbers() {
        let text = render_digest(date!(2026 - 11 - 25), &one_of_each(), false).unwrap();
        assert_eq!(
            text,
            "Prumo - resumo de 25/11\n\
             \n\
             Proventos amanha\n\
             - KNRI11 paga amanha: R$ 90,00 liquidos.\n\
             \n\
             Classes fora da meta\n\
             - Fundos Imobiliarios esta com 28,4% da carteira (meta 20%), 8,4 p.p. acima.\n\
             \n\
             No seu teto de preco\n\
             - ITSA4 voltou a ficar no seu teto de preco: R$ 9,50 (teto R$ 10,00).\n\
             \n\
             Objetivos atingidos\n\
             - Objetivo de renda mensal atingido: R$ 3.000,00.\n\
             \n\
             Saude da carteira\n\
             - KNRI11 esta com 23,5% da carteira (limite 15%). Um aporte de cerca de R$ 1.234,50 em outros ativos volta ao limite."
        );
        assert!(text.is_ascii());
    }

    #[test]
    fn the_digest_without_values_keeps_names_and_percentages_only() {
        let text = render_digest(date!(2026 - 11 - 25), &one_of_each(), true).unwrap();
        assert!(!text.contains("R$"), "{text}");
        assert!(text.contains("KNRI11 paga amanha."));
        assert!(text.contains("28,4% da carteira (meta 20%)"));
        assert!(text.contains("ITSA4 voltou a ficar no seu teto de preco ou abaixo."));
        assert!(text.contains("Objetivo de renda mensal atingido."));
        assert!(text.contains("KNRI11 esta com 23,5% da carteira (limite 15%)."));
    }

    #[test]
    fn health_texts_cover_every_rule() {
        let cases = [
            (
                warning(
                    RuleId::TopFive,
                    Subject::TopAssets {
                        labels: vec!["A".into(), "B".into()],
                    },
                    dec!(71),
                    dec!(60),
                ),
                "Os 5 maiores ativos (A, B) somam 71% da carteira (limite 60%).",
            ),
            (
                warning(
                    RuleId::MaxSector,
                    Subject::Sector {
                        class: AssetClass::BrStocks,
                        sector: "Financeiro".into(),
                    },
                    dec!(55),
                    dec!(40),
                ),
                "O setor Financeiro tem 55% de Acoes Nacionais (limite 40%).",
            ),
            (
                Warning {
                    needed_contribution: None,
                    ..warning(
                        RuleId::MinAssets,
                        Subject::Class(AssetClass::BrReits),
                        dec!(2),
                        dec!(3),
                    )
                },
                "Fundos Imobiliarios tem 2 ativo(s); o minimo que voce definiu e 3.",
            ),
            (
                Warning {
                    needed_contribution: None,
                    ..warning(
                        RuleId::EmptyClass,
                        Subject::Class(AssetClass::Crypto),
                        dec!(0),
                        dec!(1),
                    )
                },
                "Criptomoedas tem meta e nenhum ativo.",
            ),
        ];
        for (warning, expected) in cases {
            let text = describe_health(&warning, true);
            assert_eq!(text, expected);
        }
        // Combinação que a regra não produz: cai no texto genérico.
        let odd = warning(
            RuleId::MaxAsset,
            Subject::Class(AssetClass::Crypto),
            dec!(1),
            dec!(2),
        );
        assert!(describe_health(&odd, true).starts_with("Aviso de saude (max_asset)"));
    }

    #[test]
    fn a_long_digest_is_cut_before_the_telegram_limit() {
        let items: Vec<Item> = (0..400)
            .map(|n| {
                Item::Health(warning(
                    RuleId::MaxAsset,
                    Subject::Asset {
                        id: id(n),
                        label: format!("ATIVO{n}"),
                    },
                    dec!(20),
                    dec!(15),
                ))
            })
            .collect();
        let text = render_digest(date!(2026 - 11 - 25), &items, false).unwrap();
        assert!(text.len() < 4096, "{}", text.len());
        assert!(text.contains("... e mais "));
        assert!(text.ends_with("Abra o Prumo para ver tudo."));
    }

    #[test]
    fn the_test_message_has_no_portfolio_data() {
        let text = test_message(time::macros::time!(08:00));
        assert!(text.contains("08:00"));
        assert!(!text.contains("R$"));
    }

    fn arbitrary_items() -> impl Strategy<Value = Vec<Item>> {
        let item = prop_oneof![
            ("[A-Z]{4}[0-9]{1,2}", 1u32..10_000_000).prop_map(|(label, cents)| {
                Item::IncomeTomorrow {
                    label,
                    net: Decimal::new(i64::from(cents), 2),
                }
            }),
            (0u32..10_000, 0u32..10_000).prop_map(|(current, target)| Item::ClassDrift {
                class: AssetClass::FixedIncome,
                current: Decimal::new(i64::from(current), 2),
                target: Decimal::new(i64::from(target), 2),
            }),
            (
                "[A-Z]{4}[0-9]",
                1u32..1_000_000,
                1u32..1_000_000,
                any::<u128>()
            )
                .prop_map(|(label, price, ceiling, n)| Item::PriceBelowCeiling {
                    asset_id: Uuid::from_u128(n),
                    label,
                    price: Decimal::new(i64::from(price), 2),
                    ceiling: Decimal::new(i64::from(ceiling), 2),
                }),
            (1u32..100_000_000, any::<u128>()).prop_map(|(target, n)| Item::ObjectiveAchieved {
                id: Uuid::from_u128(n),
                kind: ObjectiveKind::NetWorth,
                target: Decimal::new(i64::from(target), 2),
            }),
            (
                any::<u128>(),
                1u32..10_000,
                1u32..10_000,
                prop::option::of(1u32..100_000_000)
            )
                .prop_map(|(n, current, limit, needed)| Item::Health(Warning {
                    rule: RuleId::MaxAsset,
                    subject: Subject::Asset {
                        id: Uuid::from_u128(n),
                        label: "ABCD3".into(),
                    },
                    current: Decimal::new(i64::from(current), 2),
                    limit: Decimal::new(i64::from(limit), 2),
                    needed_contribution: needed.map(|v| Decimal::new(i64::from(v), 2)),
                })),
        ];
        prop::collection::vec(item, 0..60)
    }

    proptest! {
        #[test]
        fn the_digest_without_values_never_leaks_a_real_amount(items in arbitrary_items()) {
            if let Some(text) = render_digest(date!(2026 - 11 - 25), &items, true) {
                prop_assert!(!text.contains("R$"));
            }
        }

        #[test]
        fn the_digest_always_fits_one_message(items in arbitrary_items(), hide in any::<bool>()) {
            match render_digest(date!(2026 - 11 - 25), &items, hide) {
                None => prop_assert!(items.is_empty()),
                Some(text) => {
                    prop_assert!(text.len() < 4096);
                    prop_assert!(text.is_ascii());
                }
            }
        }

        #[test]
        fn new_items_never_repeat_what_was_seen(items in arbitrary_items(), take in 0usize..30) {
            let seen: HashSet<String> = items.iter().take(take).map(Item::key).collect();
            let fresh = new_items(items.clone(), &seen);
            prop_assert!(fresh.iter().all(|i| !seen.contains(&i.key())));
            prop_assert!(fresh.len() <= items.len());
        }
    }
}
