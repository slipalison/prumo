//! O histórico de proventos flexível da tela Renda: regra pura, sem banco,
//! sem rede e sem HTTP. O mapa de calor (R$, DY e yield on cost por mês) está
//! em [`heatmap`]. O porquê de cada escolha está em `docs/renda.md`.
//!
//! O livro de proventos (`income_events`) é somado de quatro jeitos que a
//! pessoa escolhe:
//!
//! - **Intervalo**: mês, trimestre, semestre ou ano civil ([`Interval`]).
//! - **Data**: a de pagamento ou a **data com** ([`DateBasis`]). A data com é o
//!   último pregão antes da data ex ([`record_date_before`]); provento sem data
//!   ex conta pela data de pagamento.
//! - **Grupo**: o total, a classe, o segmento ou o ativo ([`Grouping`]).
//! - **Valor**: líquido ou bruto ([`AmountKind`]), os do livro. O líquido de
//!   cada provento já seguiu a regra da estimativa quando foi gravado — JCP
//!   pela data de pagamento (15% até 2025, 17,5% desde 2026), 30% em ação e
//!   REIT dos EUA, nada no BDR e no resto (`domain::income::default_withholding`)
//!   —, e a retenção que a pessoa digitou ou o extrato da B3 trouxe vale como
//!   está: é o dinheiro que caiu na conta.
//!
//! Recebido ou a receber é sempre pela data de **pagamento**, qualquer que seja
//! a data do agrupamento.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use super::asset_class::AssetClass;
use super::income::{status, IncomeStatus, YearMonth};
use super::income_auto::record_date_before;
use super::sector::SectorInfo;

pub mod heatmap;

/// De quanto em quanto tempo o histórico soma.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Interval {
    #[default]
    Month,
    Quarter,
    HalfYear,
    Year,
}

impl Interval {
    /// Quantos meses o intervalo tem.
    pub fn months(self) -> u8 {
        match self {
            Interval::Month => 1,
            Interval::Quarter => 3,
            Interval::HalfYear => 6,
            Interval::Year => 12,
        }
    }

    /// O primeiro mês do intervalo que contém `month` (o trimestre de maio
    /// começa em abril).
    pub fn start_of(self, month: YearMonth) -> YearMonth {
        let span = self.months();
        YearMonth {
            year: month.year,
            month: (month.month - 1) / span * span + 1,
        }
    }

    /// O intervalo seguinte ao que começa em `start`.
    pub fn next(self, start: YearMonth) -> YearMonth {
        start.shift(i32::from(self.months()))
    }
}

/// Que data põe o provento no mês.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DateBasis {
    /// A data de pagamento: quando o dinheiro cai.
    #[default]
    Payment,
    /// A data com: quando o direito foi garantido.
    Record,
}

/// Líquido (o que caiu na conta) ou bruto (antes do IR retido na fonte).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AmountKind {
    #[default]
    Net,
    Gross,
}

/// Como as barras se dividem.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    /// Uma barra só por intervalo.
    #[default]
    Total,
    Class,
    Segment,
    Asset,
}

/// Um provento do livro, com o que o histórico precisa.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEvent {
    pub pay_date: Date,
    pub ex_date: Option<Date>,
    pub gross: Decimal,
    pub net: Decimal,
    /// `None`: o ativo foi apagado da carteira (o evento guarda o símbolo).
    pub asset_id: Option<Uuid>,
    /// A chave e o nome do pagador, os de "Quem pagou"
    /// (`IncomeEvent::payer_key` e `payer_label`).
    pub payer_key: String,
    pub payer_label: String,
    /// A classe do ativo de hoje; `None` quando o ativo foi apagado.
    pub class: Option<AssetClass>,
    /// O segmento do ativo ([`segment_of`]); `None` sem classificação.
    pub segment: Option<String>,
}

impl HistoryEvent {
    /// A data com: o último pregão antes da data ex; sem data ex, o pagamento.
    pub fn record_date(&self) -> Date {
        self.ex_date.map_or(self.pay_date, record_date_before)
    }

    /// A data que põe o provento no mês, pela base escolhida.
    pub fn date(&self, basis: DateBasis) -> Date {
        match basis {
            DateBasis::Payment => self.pay_date,
            DateBasis::Record => self.record_date(),
        }
    }

    pub fn amount(&self, kind: AmountKind) -> Decimal {
        match kind {
            AmountKind::Net => self.net,
            AmountKind::Gross => self.gross,
        }
    }

    pub fn upcoming(&self, today: Date) -> bool {
        status(self.pay_date, today) == IncomeStatus::Upcoming
    }
}

/// O segmento de um papel para o agrupamento, da classificação pública que a
/// Saúde já guarda (`docs/saude.md`):
///
/// - **Ações Nacionais**: o segmento da B3 ("Bancos", "Energia Elétrica"); sem
///   ele, o subsetor e, por fim, o setor.
/// - **FIIs**: o **tipo** que o Prumo tira do balanço (tijolo, papel, fundo de
///   fundos, híbrido). O segmento que o administrador declara à CVM é
///   "Multicategoria" ou "Outros" em cerca de três quartos dos fundos e não
///   separa nada.
/// - O resto não tem fonte de segmento: `None`, e o grupo é a própria classe.
pub fn segment_of(class: AssetClass, info: Option<&SectorInfo>) -> Option<String> {
    let info = info?;
    match class {
        AssetClass::BrStocks => Some(
            info.segment
                .clone()
                .or_else(|| info.subsector.clone())
                .unwrap_or_else(|| info.sector.clone()),
        ),
        AssetClass::BrReits => Some(info.sector.clone()),
        _ => None,
    }
}

/// O grupo de uma barra.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GroupKey {
    Total,
    Class(AssetClass),
    /// A classe e o segmento; `None` é a classe sem segmento (sem fonte, ou o
    /// papel que a fonte não classificou).
    Segment(AssetClass, Option<String>),
    /// O pagador (`HistoryEvent::payer_key`).
    Asset(String),
    /// Ativo apagado da carteira, que ainda tem renda: sem classe nem segmento.
    Removed,
}

impl GroupKey {
    fn of(event: &HistoryEvent, grouping: Grouping) -> GroupKey {
        match (grouping, event.class) {
            (Grouping::Total, _) => GroupKey::Total,
            (Grouping::Asset, _) => GroupKey::Asset(event.payer_key.clone()),
            (Grouping::Class, Some(class)) => GroupKey::Class(class),
            (Grouping::Segment, Some(class)) => GroupKey::Segment(class, event.segment.clone()),
            (Grouping::Class | Grouping::Segment, None) => GroupKey::Removed,
        }
    }

    /// A chave no JSON: `total`, `class:br_reits`, `segment:br_stocks:Bancos`
    /// (`segment:crypto:` sem segmento), `asset:<pagador>` ou `removed`.
    pub fn wire(&self) -> String {
        match self {
            GroupKey::Total => "total".to_owned(),
            GroupKey::Class(class) => format!("class:{}", class.as_str()),
            GroupKey::Segment(class, segment) => {
                format!(
                    "segment:{}:{}",
                    class.as_str(),
                    segment.as_deref().unwrap_or("")
                )
            }
            GroupKey::Asset(key) => format!("asset:{key}"),
            GroupKey::Removed => "removed".to_owned(),
        }
    }
}

/// Um grupo do histórico, com o que a legenda precisa.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub key: GroupKey,
    /// O nome do pagador (só no agrupamento por ativo).
    pub label: Option<String>,
    /// O ativo do pagador (só no agrupamento por ativo, e se ainda existe).
    pub asset_id: Option<Uuid>,
    /// Soma de todo o histórico (recebido e a receber): a ordem da legenda e
    /// das cores, que não muda quando a pessoa troca o período à vista.
    pub total: Decimal,
}

/// Recebido e a receber de um grupo num intervalo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Split {
    pub received: Decimal,
    pub upcoming: Decimal,
}

impl Split {
    fn add(&mut self, amount: Decimal, upcoming: bool) {
        if upcoming {
            self.upcoming += amount;
        } else {
            self.received += amount;
        }
    }

    pub fn total(self) -> Decimal {
        self.received + self.upcoming
    }
}

/// Um intervalo do histórico.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    /// O primeiro mês do intervalo.
    pub start: YearMonth,
    pub total: Split,
    /// Os grupos com valor no intervalo, na ordem de [`History::groups`].
    pub groups: Vec<(GroupKey, Split)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    /// Do maior para o menor no histórico inteiro.
    pub groups: Vec<Group>,
    /// Do mais velho ao mais novo, sem buraco: do intervalo do primeiro
    /// provento ao de hoje (ou ao do último a receber, se for depois).
    pub buckets: Vec<Bucket>,
}

/// O que se escolhe na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryOptions {
    pub interval: Interval,
    pub basis: DateBasis,
    pub grouping: Grouping,
    pub amount: AmountKind,
}

/// Soma o livro em intervalos, por grupo.
pub fn history(events: &[HistoryEvent], options: HistoryOptions, today: Date) -> History {
    let interval = options.interval;
    let mut sums: BTreeMap<YearMonth, HashMap<GroupKey, Split>> = BTreeMap::new();
    let mut groups: HashMap<GroupKey, Group> = HashMap::new();
    for event in events {
        let start = interval.start_of(YearMonth::of(event.date(options.basis)));
        let key = GroupKey::of(event, options.grouping);
        let amount = event.amount(options.amount);
        let upcoming = event.upcoming(today);
        sums.entry(start)
            .or_default()
            .entry(key.clone())
            .or_default()
            .add(amount, upcoming);
        let group = groups.entry(key.clone()).or_insert_with(|| Group {
            label: matches!(key, GroupKey::Asset(_)).then(|| event.payer_label.clone()),
            asset_id: if matches!(key, GroupKey::Asset(_)) {
                event.asset_id
            } else {
                None
            },
            key,
            total: Decimal::ZERO,
        });
        group.total += amount;
    }
    let (Some(first), Some(last)) = (sums.keys().next().copied(), sums.keys().last().copied())
    else {
        return History::default();
    };
    let mut groups: Vec<Group> = groups.into_values().collect();
    groups.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.key.cmp(&b.key))
    });
    let order: HashMap<&GroupKey, usize> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (&g.key, i))
        .collect();

    let end = last.max(interval.start_of(YearMonth::of(today)));
    let mut buckets = Vec::new();
    let mut start = first;
    while start <= end {
        let mut parts: Vec<(GroupKey, Split)> = sums
            .remove(&start)
            .map(|parts| parts.into_iter().collect())
            .unwrap_or_default();
        parts.sort_by_key(|(key, _)| order.get(key).copied().unwrap_or(usize::MAX));
        let total = parts.iter().fold(Split::default(), |acc, (_, s)| Split {
            received: acc.received + s.received,
            upcoming: acc.upcoming + s.upcoming,
        });
        buckets.push(Bucket {
            start,
            total,
            groups: parts,
        });
        start = interval.next(start);
    }
    History { groups, buckets }
}

#[cfg(test)]
mod tests;
