//! A nota de um ativo: de onde ela vem e quanto vale.

use serde::Serialize;

use super::asset_class::Checklist;

/// Nota máxima aceita, manual ou de checklist.
pub const MAX_SCORE: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreSource {
    Manual,
    Checklist,
    None,
}

/// A nota e a origem dela.
///
/// - `score_override` vence tudo (é a nota que a pessoa escolheu à mão);
/// - numa classe com checklist, cada "sim" soma 1 e cada "não" tira 1, com
///   piso em zero — pergunta sem resposta não pesa;
/// - sem nenhum dos dois, a nota é zero, e ativo com nota zero não recebe aporte.
pub fn score(
    score_override: Option<u8>,
    checklist: Option<Checklist>,
    answers: impl IntoIterator<Item = bool>,
) -> (u8, ScoreSource) {
    if let Some(manual) = score_override {
        return (manual.min(MAX_SCORE), ScoreSource::Manual);
    }
    if checklist.is_none() {
        return (0, ScoreSource::None);
    }
    let (mut yes, mut no, mut answered) = (0i64, 0i64, false);
    for answer in answers {
        answered = true;
        if answer {
            yes += 1;
        } else {
            no += 1;
        }
    }
    if !answered {
        return (0, ScoreSource::None);
    }
    let net = (yes - no).clamp(0, i64::from(u8::MAX));
    (u8::try_from(net).unwrap_or(u8::MAX), ScoreSource::Checklist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_score_wins_and_is_capped() {
        assert_eq!(
            score(Some(7), Some(Checklist::Stocks), [false, false]),
            (7, ScoreSource::Manual)
        );
        assert_eq!(score(Some(42), None, []), (10, ScoreSource::Manual));
        assert_eq!(score(Some(0), None, []), (0, ScoreSource::Manual));
    }

    #[test]
    fn checklist_score_is_yes_minus_no_with_floor_zero() {
        assert_eq!(
            score(
                None,
                Some(Checklist::RealEstate),
                [true, true, true, true, true, false]
            ),
            (4, ScoreSource::Checklist)
        );
        assert_eq!(
            score(
                None,
                Some(Checklist::RealEstate),
                [false, true, false, true, true, false]
            ),
            (0, ScoreSource::Checklist)
        );
        assert_eq!(
            score(None, Some(Checklist::Stocks), [false, false]),
            (0, ScoreSource::Checklist)
        );
    }

    #[test]
    fn no_answers_or_no_checklist_means_no_score() {
        assert_eq!(
            score(None, Some(Checklist::Stocks), []),
            (0, ScoreSource::None)
        );
        assert_eq!(score(None, None, [true, true]), (0, ScoreSource::None));
    }
}
