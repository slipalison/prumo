//! Risco da carteira e de cada ativo: volatilidade, queda máxima, correlação
//! e a contribuição de cada ativo para o risco. Regra pura: sem banco, sem
//! rede. Fórmulas, fontes e limites em `docs/risco.md`.
//!
//! Tudo parte de uma série de **nível** por data (a cota da carteira, o
//! Ibovespa em pontos, o índice de retorno total de cada ativo em reais),
//! alinhada às datas de pregão da B3. O retorno de um pregão é o nível dele
//! sobre o do pregão anterior, menos 1. É informação: nada aqui diz o que
//! fazer com a carteira.
//!
//! As contas são em `f64`: são estimativas estatísticas (um desvio-padrão, uma
//! correlação), não dinheiro. O que vai para a tela é arredondado no DTO.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use time::{Date, Duration};

use super::performance::months_before;
use super::treasury::BondKind;

/// Pregões num ano, para anualizar a volatilidade diária (`σ × √252`). É a
/// convenção do mercado brasileiro; a B3 tem de 246 a 252 pregões por ano.
pub const TRADING_DAYS_PER_YEAR: f64 = 252.0;
/// Retornos diários mínimos para a volatilidade, o retorno e a queda máxima de
/// uma série: com menos, o desvio-padrão erra demais (≈ 16% para cima ou para
/// baixo com 20 pontos) e a série aparece como "histórico curto".
pub const MIN_RETURNS: usize = 20;
/// Retornos em comum mínimos para a correlação de um par e para a
/// contribuição para o risco: uns três meses de pregões. Com 60 pontos, a
/// correlação de dois ativos sem relação nenhuma ainda oscila ±0,13 por acaso.
pub const MIN_COMMON_RETURNS: usize = 60;
/// Até quantos dias corridos o último fechamento vale para as datas seguintes
/// (o fim de semana do ativo dos EUA, o pregão sem negócio de um FII pouco
/// líquido). Depois disso a série para: ativo deslistado ou fonte que parou não
/// vira uma linha reta de retorno zero.
pub const MAX_STALE_DAYS: i64 = 10;
/// Quantos pares a tela destaca como os que mais andam juntos.
pub const TOP_PAIRS: usize = 5;
/// Folga antes do começo da janela: o fechamento de base fica antes dela.
const BASE_MARGIN_DAYS: i64 = 10;

/// As janelas da tela. A de 12 meses é a padrão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskWindow {
    SixMonths,
    TwelveMonths,
    TwoYears,
    ThreeYears,
    FiveYears,
}

impl RiskWindow {
    pub const ALL: [RiskWindow; 5] = [
        RiskWindow::SixMonths,
        RiskWindow::TwelveMonths,
        RiskWindow::TwoYears,
        RiskWindow::ThreeYears,
        RiskWindow::FiveYears,
    ];
    /// A mais longa: até onde o histórico de preço precisa ir.
    pub const LONGEST: RiskWindow = RiskWindow::FiveYears;

    pub fn parse(value: &str) -> Option<Self> {
        RiskWindow::ALL.into_iter().find(|w| w.as_str() == value)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RiskWindow::SixMonths => "6m",
            RiskWindow::TwelveMonths => "12m",
            RiskWindow::TwoYears => "24m",
            RiskWindow::ThreeYears => "36m",
            RiskWindow::FiveYears => "60m",
        }
    }

    pub fn months(self) -> u32 {
        match self {
            RiskWindow::SixMonths => 6,
            RiskWindow::TwelveMonths => 12,
            RiskWindow::TwoYears => 24,
            RiskWindow::ThreeYears => 36,
            RiskWindow::FiveYears => 60,
        }
    }

    /// O começo da janela que termina em `today` (a mesma data, `months`
    /// meses antes).
    pub fn start(self, today: Date) -> Date {
        months_before(today, self.months())
    }
}

/// Desde quando o histórico de preço (e o do Ibovespa) precisa existir para a
/// janela mais longa terminar em `today`, com o fechamento de base dentro.
pub fn history_start(today: Date) -> Date {
    RiskWindow::LONGEST.start(today) - Duration::days(BASE_MARGIN_DAYS)
}

/// Uma série de nível por data, em ordem crescente e sem data repetida.
pub type Levels = Vec<(Date, f64)>;

/// O título do Tesouro que paga no caminho (juros semestrais, a renda mensal
/// do Renda+ e do Educa+): o preço do Tesouro cai a cada pagamento, e a
/// tabela não traz o pagamento. A queda pareceria perda, então o título fica
/// fora das contas por ativo (na cota da carteira o pagamento entra como
/// provento, se estiver lançado).
pub fn pays_before_maturity(kind: BondKind) -> bool {
    matches!(
        kind,
        BondKind::PrefixedWithCoupons
            | BondKind::IpcaPlusWithCoupons
            | BondKind::IgpmPlusWithCoupons
            | BondKind::RendaPlus
            | BondKind::EducaPlus
    )
}

fn to_f64(value: Decimal) -> Option<f64> {
    value.to_f64().filter(|v| v.is_finite())
}

/// Uma série em `Decimal` como nível: a cota, os pontos do Ibovespa, o câmbio
/// e o índice de retorno total de um ativo. O ativo chega aqui já no pé de hoje
/// e com os proventos dentro, pelas mesmas contas do comparador
/// ([`super::compare::adjust_for_splits`] e
/// [`super::compare::total_return_levels`]): `price_history` guarda o
/// fechamento **real** do dia, e sem desfazer o desdobramento um 1 para 2
/// viraria uma queda de 50% num pregão.
pub fn levels(series: &[(Date, Decimal)]) -> Levels {
    series
        .iter()
        .filter_map(|(date, value)| to_f64(*value).filter(|v| *v > 0.0).map(|v| (*date, v)))
        .collect()
}

/// As datas de pregão: todas as datas das séries dadas (o Ibovespa e os
/// fechamentos dos ativos da B3), sem repetição e em ordem.
pub fn trading_calendar(sources: &[&[Date]]) -> Vec<Date> {
    let mut dates: Vec<Date> = sources.iter().flat_map(|s| s.iter().copied()).collect();
    dates.sort();
    dates.dedup();
    dates
}

/// A série nas datas do calendário: em cada data, o último nível em ou antes
/// dela, desde que de no máximo [`MAX_STALE_DAYS`] dias antes. Antes do
/// primeiro nível (e depois de a série parar), `None`.
pub fn align(levels: &[(Date, f64)], calendar: &[Date]) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(calendar.len());
    let mut next = 0;
    let mut last: Option<(Date, f64)> = None;
    for date in calendar {
        while let Some(point) = levels.get(next).filter(|(d, _)| d <= date) {
            last = Some(*point);
            next += 1;
        }
        out.push(
            last.filter(|(seen, _)| (*date - *seen).whole_days() <= MAX_STALE_DAYS)
                .map(|(_, v)| v),
        );
    }
    out
}

/// O produto de duas séries alinhadas, data a data: o nível na moeda do ativo
/// vezes o câmbio dá o nível em reais. Falta de qualquer um é falta.
pub fn times(levels: &[Option<f64>], fx: &[Option<f64>]) -> Vec<Option<f64>> {
    levels
        .iter()
        .zip(fx)
        .map(|(level, rate)| Some(level.as_ref()? * rate.as_ref()?))
        .collect()
}

/// Os retornos diários de uma série alinhada: a posição `i` é
/// `L_i / L_{i-1} - 1` quando as duas datas têm nível. A posição 0 nunca tem
/// retorno (é a base).
pub fn returns(levels: &[Option<f64>]) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(levels.len());
    let mut previous: Option<f64> = None;
    for level in levels {
        out.push(match (previous, level) {
            (Some(before), Some(now)) if before > 0.0 => Some(now / before - 1.0),
            _ => None,
        });
        previous = *level;
    }
    out
}

/// O desvio-padrão amostral (`n - 1`) dos retornos diários vezes `√252`.
/// `None` com menos de [`MIN_RETURNS`] retornos.
pub fn annualized_volatility(returns: &[f64]) -> Option<f64> {
    if returns.len() < MIN_RETURNS {
        return None;
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    Some((variance.max(0.0) * TRADING_DAYS_PER_YEAR).sqrt())
}

/// A maior queda de um pico até um fundo seguinte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawdown {
    /// Fração negativa: −0,25 = o nível chegou a 25% abaixo do pico.
    pub depth: f64,
    /// O pico de onde a queda começou.
    pub peak: Date,
    /// O ponto mais baixo da queda.
    pub trough: Date,
    /// O primeiro dia, depois do fundo, em que o nível voltou ao do pico.
    /// `None`: ainda não recuperou até o fim da série.
    pub recovered: Option<Date>,
}

/// A queda máxima da série. `None` se ela nunca ficou abaixo de um pico
/// anterior.
pub fn max_drawdown(points: &[(Date, f64)]) -> Option<Drawdown> {
    let mut iter = points.iter();
    let &(first_date, first) = iter.next()?;
    let (mut peak_date, mut peak) = (first_date, first);
    let mut worst: Option<(f64, Date, Date, f64)> = None;
    for &(date, level) in iter {
        if level > peak {
            (peak_date, peak) = (date, level);
            continue;
        }
        let depth = level / peak - 1.0;
        if depth < 0.0 && worst.is_none_or(|(w, ..)| depth < w) {
            worst = Some((depth, peak_date, date, peak));
        }
    }
    let (depth, peak, trough, level) = worst?;
    let recovered = points
        .iter()
        .find(|(date, value)| *date > trough && *value >= level)
        .map(|(date, _)| *date);
    Some(Drawdown {
        depth,
        peak,
        trough,
        recovered,
    })
}

/// O que a tela mostra de uma série na janela.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// A primeira e a última data com nível na janela.
    pub from: Date,
    pub to: Date,
    /// A série tem nível já no fechamento de base: cobre a janela inteira.
    pub full: bool,
    /// Quantos retornos diários entraram.
    pub returns: usize,
    /// Anualizada, em fração. `None` com histórico curto.
    pub volatility: Option<f64>,
    /// Do primeiro ao último nível, em fração. `None` com histórico curto.
    pub ret: Option<f64>,
    /// `None` com histórico curto ou sem queda nenhuma (ver `returns`).
    pub drawdown: Option<Drawdown>,
}

impl Metrics {
    /// Tem retornos bastantes para os números.
    pub fn enough(&self) -> bool {
        self.returns >= MIN_RETURNS
    }
}

/// Volatilidade, retorno e queda máxima de uma série alinhada ao calendário
/// da janela (a posição 0 é o fechamento de base). `None` sem dois níveis.
pub fn metrics(calendar: &[Date], levels: &[Option<f64>]) -> Option<Metrics> {
    let points: Levels = calendar
        .iter()
        .zip(levels)
        .filter_map(|(date, level)| level.map(|v| (*date, v)))
        .collect();
    let (&(from, first), &(to, last)) = (points.first()?, points.last()?);
    if points.len() < 2 {
        return None;
    }
    let daily: Vec<f64> = returns(levels).into_iter().flatten().collect();
    let enough = daily.len() >= MIN_RETURNS;
    Some(Metrics {
        from,
        to,
        full: levels.first().is_some_and(Option::is_some),
        returns: daily.len(),
        volatility: annualized_volatility(&daily),
        ret: enough.then(|| last / first - 1.0),
        drawdown: if enough { max_drawdown(&points) } else { None },
    })
}

/// A correlação de Pearson dos retornos de duas séries alinhadas, só nas
/// datas em que as duas têm retorno, e quantas datas foram. `None` com menos
/// de [`MIN_COMMON_RETURNS`] datas em comum ou com uma série parada (variância
/// zero, a correlação não existe).
pub fn correlation(a: &[Option<f64>], b: &[Option<f64>]) -> Option<(f64, usize)> {
    let pairs: Vec<(f64, f64)> = a
        .iter()
        .zip(b)
        .filter_map(|(x, y)| Some(((*x)?, (*y)?)))
        .collect();
    if pairs.len() < MIN_COMMON_RETURNS {
        return None;
    }
    let n = pairs.len() as f64;
    let (mean_a, mean_b) = pairs
        .iter()
        .fold((0.0, 0.0), |(sa, sb), (x, y)| (sa + x / n, sb + y / n));
    let (mut cov, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
    for (x, y) in &pairs {
        cov += (x - mean_a) * (y - mean_b);
        var_a += (x - mean_a).powi(2);
        var_b += (y - mean_b).powi(2);
    }
    let scale = (var_a * var_b).sqrt();
    if scale.is_nan() || scale <= f64::EPSILON {
        return None;
    }
    Some(((cov / scale).clamp(-1.0, 1.0), pairs.len()))
}

/// A matriz de correlação de várias séries: simétrica, 1 na diagonal (se a
/// série tem retornos bastantes) e cada par com as datas que os dois têm.
pub fn correlation_matrix(series: &[&[Option<f64>]]) -> Vec<Vec<Option<(f64, usize)>>> {
    let n = series.len();
    let mut out = vec![vec![None; n]; n];
    for i in 0..n {
        for j in i..n {
            let value = if i == j {
                correlation(series[i], series[i]).map(|(_, days)| (1.0, days))
            } else {
                correlation(series[i], series[j])
            };
            out[i][j] = value;
            out[j][i] = value;
        }
    }
    out
}

/// Um par da matriz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub value: f64,
    pub days: usize,
}

/// Os `n` pares (fora da diagonal) com a maior correlação, do maior para o
/// menor; no empate, a ordem das linhas.
pub fn top_pairs(matrix: &[Vec<Option<(f64, usize)>>], n: usize) -> Vec<Pair> {
    let mut pairs: Vec<Pair> = Vec::new();
    for (a, row) in matrix.iter().enumerate() {
        for (b, cell) in row.iter().enumerate().skip(a + 1) {
            if let Some((value, days)) = cell {
                pairs.push(Pair {
                    a,
                    b,
                    value: *value,
                    days: *days,
                });
            }
        }
    }
    pairs.sort_by(|x, y| {
        y.value
            .total_cmp(&x.value)
            .then((x.a, x.b).cmp(&(y.a, y.b)))
    });
    pairs.truncate(n);
    pairs
}

/// Um ativo na conta da contribuição para o risco.
#[derive(Debug, Clone, Copy)]
pub struct Holding<'a> {
    /// O peso na carteira inteira hoje, em fração.
    pub weight: f64,
    /// Os retornos alinhados ao calendário da janela.
    pub returns: &'a [Option<f64>],
}

/// A carteira de hoje medida na janela: quanto ela oscilaria com os pesos de
/// hoje e quanto de cada ativo vem essa oscilação.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskModel {
    /// Volatilidade anualizada da carteira de hoje (`σ_p`), em fração.
    pub volatility: f64,
    /// Quantos pregões em comum entraram.
    pub days: usize,
    /// A parte de cada ativo na variância (`w_i · cov(r_i, R_p) / var(R_p)`),
    /// na ordem recebida. Soma 1; pode ser negativa (o ativo que anda contra
    /// o resto tira risco).
    pub shares: Vec<f64>,
}

/// A contribuição de cada ativo para o risco da carteira de hoje (decomposição
/// de Euler): `R_p = Σ w_i r_i` em cada pregão em que **todos** têm retorno;
/// a parte do ativo `i` é `w_i · cov(r_i, R_p) / var(R_p)`, e as partes somam 1.
/// O que não está na lista (renda fixa por valor, ativo sem histórico) entra
/// como se não oscilasse: o peso dele dilui `σ_p`, sem parte nenhuma.
/// `None` sem ativo, com menos de [`MIN_COMMON_RETURNS`] pregões em comum ou
/// com a carteira parada.
pub fn risk_model(holdings: &[Holding<'_>]) -> Option<RiskModel> {
    let positions = holdings.iter().map(|h| h.returns.len()).min()?;
    let common: Vec<usize> = (0..positions)
        .filter(|&t| holdings.iter().all(|h| h.returns[t].is_some()))
        .collect();
    if common.len() < MIN_COMMON_RETURNS {
        return None;
    }
    let value = |h: &Holding<'_>, t: usize| h.returns[t].unwrap_or(0.0);
    let n = common.len() as f64;
    let portfolio: Vec<f64> = common
        .iter()
        .map(|&t| holdings.iter().map(|h| h.weight * value(h, t)).sum())
        .collect();
    let mean_p = portfolio.iter().sum::<f64>() / n;
    let variance = portfolio.iter().map(|r| (r - mean_p).powi(2)).sum::<f64>() / (n - 1.0);
    if variance.is_nan() || variance <= f64::EPSILON * f64::EPSILON {
        return None;
    }
    let shares = holdings
        .iter()
        .map(|h| {
            let mean = common.iter().map(|&t| value(h, t)).sum::<f64>() / n;
            let cov = common
                .iter()
                .zip(&portfolio)
                .map(|(&t, rp)| (value(h, t) - mean) * (rp - mean_p))
                .sum::<f64>()
                / (n - 1.0);
            h.weight * cov / variance
        })
        .collect();
    Some(RiskModel {
        volatility: (variance * TRADING_DAYS_PER_YEAR).sqrt(),
        days: common.len(),
        shares,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn days(from: Date, n: usize) -> Vec<Date> {
        (0..n).map(|i| from + Duration::days(i as i64)).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn windows_round_trip_and_start_months_before() {
        for window in RiskWindow::ALL {
            assert_eq!(RiskWindow::parse(window.as_str()), Some(window));
        }
        assert_eq!(RiskWindow::parse("1d"), None);
        let today = date!(2026 - 11 - 27);
        assert_eq!(RiskWindow::TwelveMonths.start(today), date!(2025 - 11 - 27));
        assert_eq!(RiskWindow::SixMonths.start(today), date!(2026 - 05 - 27));
        assert_eq!(RiskWindow::FiveYears.start(today), date!(2021 - 11 - 27));
        assert_eq!(history_start(today), date!(2021 - 11 - 17));
    }

    #[test]
    fn bonds_that_pay_along_the_way_are_known() {
        let paying = BondKind::ALL
            .into_iter()
            .filter(|k| pays_before_maturity(*k))
            .count();
        assert_eq!(paying, 5);
        assert!(!pays_before_maturity(BondKind::Selic));
        assert!(!pays_before_maturity(BondKind::Prefixed));
        assert!(!pays_before_maturity(BondKind::IpcaPlus));
    }

    #[test]
    fn levels_drop_what_is_not_a_level() {
        let series = [
            (date!(2026 - 01 - 02), dec!(10.5)),
            (date!(2026 - 01 - 05), dec!(0)),
        ];
        assert_eq!(levels(&series), vec![(date!(2026 - 01 - 02), 10.5)]);
    }

    #[test]
    fn alignment_fills_forward_but_not_forever() {
        let calendar = days(date!(2026 - 01 - 01), 15);
        let series = [(date!(2026 - 01 - 02), 10.0), (date!(2026 - 01 - 03), 11.0)];
        let aligned = align(&series, &calendar);
        assert_eq!(aligned[0], None);
        assert_eq!(aligned[1], Some(10.0));
        assert_eq!(aligned[2], Some(11.0));
        // Dez dias depois do último fechamento ainda vale; o décimo primeiro não.
        assert_eq!(aligned[12], Some(11.0));
        assert_eq!(aligned[13], None);
        let reais = times(&aligned, &[Some(5.0); 15]);
        assert_eq!(reais[2], Some(55.0));
        assert_eq!(reais[0], None);
    }

    #[test]
    fn calendar_merges_the_sources() {
        let a = [date!(2026 - 01 - 05), date!(2026 - 01 - 02)];
        let b = [date!(2026 - 01 - 05), date!(2026 - 01 - 06)];
        assert_eq!(
            trading_calendar(&[&a, &b]),
            vec![
                date!(2026 - 01 - 02),
                date!(2026 - 01 - 05),
                date!(2026 - 01 - 06)
            ]
        );
    }

    #[test]
    fn returns_skip_gaps() {
        let levels = [Some(100.0), Some(110.0), None, Some(99.0), Some(99.0)];
        let r = returns(&levels);
        assert_eq!(r[0], None);
        assert!(close(r[1].unwrap(), 0.1));
        assert_eq!(r[2], None);
        assert_eq!(r[3], None);
        assert_eq!(r[4], Some(0.0));
    }

    #[test]
    fn volatility_by_hand() {
        // Alterna +1% e −1%: média 0, desvio amostral √(n/(n−1)) × 1%.
        let daily: Vec<f64> = (0..20)
            .map(|i| if i % 2 == 0 { 0.01 } else { -0.01 })
            .collect();
        let expected = (20.0_f64 / 19.0).sqrt() * 0.01 * 252.0_f64.sqrt();
        assert!(close(annualized_volatility(&daily).unwrap(), expected));
        assert_eq!(annualized_volatility(&daily[..19]), None);
        assert_eq!(annualized_volatility(&[0.0; 30]), Some(0.0));
    }

    #[test]
    fn drawdown_finds_peak_trough_and_recovery() {
        let d = days(date!(2026 - 01 - 01), 7);
        let points: Levels = [100.0, 120.0, 90.0, 96.0, 84.0, 121.0, 110.0]
            .into_iter()
            .enumerate()
            .map(|(i, v)| (d[i], v))
            .collect();
        let dd = max_drawdown(&points).unwrap();
        assert!(close(dd.depth, 84.0 / 120.0 - 1.0));
        assert_eq!((dd.peak, dd.trough, dd.recovered), (d[1], d[4], Some(d[5])));
        // Sem voltar ao pico: ainda não recuperou.
        let open = max_drawdown(&points[..5]).unwrap();
        assert_eq!(open.recovered, None);
        // Só sobe: sem queda.
        assert_eq!(max_drawdown(&[(d[0], 1.0), (d[1], 2.0)]), None);
        assert_eq!(max_drawdown(&[]), None);
    }

    #[test]
    fn metrics_need_enough_returns() {
        let calendar = days(date!(2026 - 01 - 01), 30);
        let levels: Vec<Option<f64>> = (0..30)
            .map(|i| (i >= 5).then_some(if i % 2 == 0 { 101.0 } else { 100.0 }))
            .collect();
        let m = metrics(&calendar, &levels).unwrap();
        assert!(!m.full);
        assert_eq!(m.from, calendar[5]);
        assert_eq!(m.to, calendar[29]);
        assert_eq!(m.returns, 24);
        assert!(m.enough());
        assert!(m.volatility.unwrap() > 0.0);
        // Do primeiro (100) ao último (100).
        assert!(close(m.ret.unwrap(), 0.0));
        assert!(m.drawdown.is_some());
        let short = metrics(&calendar[..10], &levels[..10]).unwrap();
        assert!(!short.enough());
        assert_eq!(
            (short.volatility, short.ret, short.drawdown),
            (None, None, None)
        );
        assert_eq!(metrics(&calendar[..1], &[Some(1.0)]), None);
    }

    #[test]
    fn correlation_of_opposite_and_unrelated_series() {
        let a: Vec<Option<f64>> = (0..80).map(|i| Some(((i * 7) % 11) as f64 - 5.0)).collect();
        let minus: Vec<Option<f64>> = a.iter().map(|x| x.map(|v| -2.0 * v)).collect();
        let (value, n) = correlation(&a, &minus).unwrap();
        assert!(close(value, -1.0));
        assert_eq!(n, 80);
        assert!(close(correlation(&a, &a).unwrap().0, 1.0));
        // Parada: sem correlação. Poucas datas em comum: sem correlação.
        assert_eq!(correlation(&a, &[Some(0.0); 80]), None);
        let late: Vec<Option<f64>> = (0..80)
            .map(|i| (i >= 30).then_some(1.0 + i as f64))
            .collect();
        assert_eq!(correlation(&a, &late), None);
    }

    #[test]
    fn matrix_and_top_pairs() {
        let a: Vec<Option<f64>> = (0..70).map(|i| Some(((i * 7) % 11) as f64)).collect();
        let b: Vec<Option<f64>> = a.iter().map(|x| x.map(|v| v * 3.0 + 1.0)).collect();
        let c: Vec<Option<f64>> = (0..70).map(|i| Some(((i * 5) % 13) as f64)).collect();
        let matrix = correlation_matrix(&[&a, &b, &c]);
        assert_eq!(matrix[0][0].map(|c| c.0), Some(1.0));
        assert_eq!(matrix[0][2], matrix[2][0]);
        let top = top_pairs(&matrix, 2);
        assert_eq!(top.len(), 2);
        assert_eq!((top[0].a, top[0].b), (0, 1));
        assert!(close(top[0].value, 1.0));
        assert!(top[0].value >= top[1].value);
    }

    #[test]
    fn risk_shares_by_hand() {
        // Dois ativos sem relação, mesma volatilidade, pesos 30% e 10% (o resto
        // é renda fixa): a parte de cada um é w² σ² / Σ w² σ² = 90% e 10%.
        let up_down: Vec<Option<f64>> = (0..80)
            .map(|i| Some(if i % 2 == 0 { 0.01 } else { -0.01 }))
            .collect();
        let pairs: Vec<Option<f64>> = (0..80)
            .map(|i| Some(if (i / 2) % 2 == 0 { 0.01 } else { -0.01 }))
            .collect();
        let model = risk_model(&[
            Holding {
                weight: 0.3,
                returns: &up_down,
            },
            Holding {
                weight: 0.1,
                returns: &pairs,
            },
        ])
        .unwrap();
        assert_eq!(model.days, 80);
        assert!((model.shares[0] - 0.9).abs() < 1e-9, "{:?}", model.shares);
        assert!((model.shares[1] - 0.1).abs() < 1e-9);
        // σ_p = √(0,3² + 0,1²) × σ.
        let sigma = (80.0_f64 / 79.0).sqrt() * 0.01 * 252.0_f64.sqrt();
        assert!((model.volatility - (0.1_f64).sqrt() * sigma).abs() < 1e-9);
        // Sem ativo, poucos pregões ou carteira parada: sem conta.
        assert_eq!(risk_model(&[]), None);
        assert_eq!(
            risk_model(&[Holding {
                weight: 1.0,
                returns: &up_down[..50],
            }]),
            None
        );
        let flat = [Some(0.0); 80];
        assert_eq!(
            risk_model(&[Holding {
                weight: 1.0,
                returns: &flat,
            }]),
            None
        );
    }

    fn arb_levels() -> impl Strategy<Value = Vec<f64>> {
        prop::collection::vec(-0.08f64..0.08, 21..120).prop_map(|daily| {
            let mut level = 100.0;
            let mut out = vec![level];
            for r in daily {
                level *= 1.0 + r;
                out.push(level);
            }
            out
        })
    }

    proptest! {
        /// A volatilidade não depende do preço da cota: dobrar todos os
        /// níveis não muda os retornos.
        #[test]
        fn volatility_ignores_the_price_scale(levels in arb_levels(), scale in 0.01f64..1000.0) {
            let a: Vec<Option<f64>> = levels.iter().map(|v| Some(*v)).collect();
            let b: Vec<Option<f64>> = levels.iter().map(|v| Some(v * scale)).collect();
            let va: Vec<f64> = returns(&a).into_iter().flatten().collect();
            let vb: Vec<f64> = returns(&b).into_iter().flatten().collect();
            let (x, y) = (annualized_volatility(&va).unwrap(), annualized_volatility(&vb).unwrap());
            prop_assert!((x - y).abs() < 1e-9 * x.max(1.0));
        }

        /// A queda fica entre −100% e 0, e as datas andam para a frente.
        #[test]
        fn drawdown_is_bounded_and_ordered(levels in arb_levels()) {
            let points: Levels = levels
                .iter()
                .enumerate()
                .map(|(i, v)| (date!(2020 - 01 - 01) + Duration::days(i as i64), *v))
                .collect();
            if let Some(dd) = max_drawdown(&points) {
                prop_assert!(dd.depth < 0.0 && dd.depth > -1.0);
                prop_assert!(dd.peak < dd.trough);
                prop_assert!(dd.recovered.is_none_or(|r| r > dd.trough));
            }
        }

        /// A correlação fica entre −1 e 1 e é simétrica.
        #[test]
        fn correlation_is_bounded_and_symmetric(
            a in prop::collection::vec(-0.1f64..0.1, 60..150),
            b in prop::collection::vec(-0.1f64..0.1, 60..150),
        ) {
            let a: Vec<Option<f64>> = a.into_iter().map(Some).collect();
            let b: Vec<Option<f64>> = b.into_iter().map(Some).collect();
            if let Some((ab, n)) = correlation(&a, &b) {
                let (ba, m) = correlation(&b, &a).unwrap();
                prop_assert!((-1.0..=1.0).contains(&ab));
                prop_assert!((ab - ba).abs() < 1e-12);
                prop_assert_eq!(n, m);
            }
        }

        /// As partes do risco somam 100%, quaisquer que sejam os pesos.
        #[test]
        fn risk_shares_add_up(
            series in prop::collection::vec(prop::collection::vec(-0.05f64..0.05, 70), 1..6),
            weights in prop::collection::vec(0.01f64..1.0, 6),
        ) {
            let aligned: Vec<Vec<Option<f64>>> = series
                .iter()
                .map(|s| s.iter().map(|v| Some(*v)).collect())
                .collect();
            let holdings: Vec<Holding<'_>> = aligned
                .iter()
                .zip(&weights)
                .map(|(r, w)| Holding { weight: *w / 6.0, returns: r })
                .collect();
            if let Some(model) = risk_model(&holdings) {
                let total: f64 = model.shares.iter().sum();
                prop_assert!((total - 1.0).abs() < 1e-6, "{total}");
                prop_assert!(model.volatility > 0.0);
            }
        }
    }
}
