//! Qual fundo (CNPJ) é o de um ticker de FII. Regra pura: sem banco, sem rede.
//! O método está em `docs/fundamentos.md` ("Ticker de FII → fundo").
//!
//! A CVM não traz o ticker no informe mensal; o Prumo liga os dois pela raiz do
//! ISIN da cota (`BRKNRICTF007` → `KNRI` → `KNRI11`). Há fundo que preenche o
//! ISIN de outro (medido em 27/11/2026: um fundo novo declarou o ISIN de um
//! fundo antigo e mais novo informe), e mais de um CNPJ passa a reivindicar o
//! mesmo emissor. Pegar o informe mais novo, como antes, trocava o fundo: o
//! valor patrimonial, o tipo, a série mensal, o CNPJ do Fundos.NET e o CNPJ da
//! fonte pagadora no informe de rendimentos do IR saíam do fundo errado.
//!
//! [`choose_fund`] decide, nesta ordem:
//!
//! 1. **A B3**, quando ela informou o CNPJ do fundo pelo código de negociação
//!    (a fonte do ticker): vale esse CNPJ, mesmo que nenhum informe da CVM o
//!    tenha (aí o fundo fica sem números da CVM, nunca com os de outro);
//! 2. sem a B3, entre os fundos **em dia** (último informe a até
//!    [`CURRENT_MONTHS`] meses do mais novo deles: um fundo que virou classe e
//!    mudou de CNPJ deixa o antigo para trás), o que declara o emissor **há mais
//!    tempo** (primeiro informe mais antigo nos dois anos de informe que o
//!    Prumo lê); empate, o informe mais novo; depois, o menor CNPJ
//!    (determinístico).
//!
//! **Por que esse desempate.** Medido em 27/11/2026 nos informes de 2025 e
//! 2026: 31 raízes de ISIN tinham mais de um CNPJ; das 20 que a B3 lista com um
//! fundo que está no informe, "declara há mais tempo" acertou 17 (o maior
//! número de cotistas, 16; "negociado em bolsa" e depois cotistas, 15). As três
//! que ele erra são de fundos que declaram o mesmo ISIN desde o começo da
//! série: só a B3 resolve, e ela vem primeiro.

use time::Date;

use super::performance::months_before;

/// Quantos meses atrás do informe mais novo um fundo ainda conta como em dia.
pub const CURRENT_MONTHS: u32 = 3;

/// Um fundo que declara uma cota com a raiz de ISIN do emissor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundClaim {
    /// Como a CVM escreve (`00.000.000/0001-91`).
    pub cnpj: String,
    /// O primeiro e o último mês com informe conhecido.
    pub first_reported: Date,
    pub last_reported: Date,
}

/// De onde veio a escolha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenBy {
    /// A B3 informou o CNPJ pelo código de negociação.
    B3,
    /// Só um fundo reivindica o emissor.
    Single,
    /// Mais de um, sem a B3: o desempate.
    Oldest,
}

/// O fundo de um emissor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundChoice {
    pub cnpj: String,
    pub by: ChosenBy,
    /// Os outros CNPJs que declaram o mesmo emissor (o conflito, para o log).
    pub rivals: Vec<String>,
}

/// Só os dígitos de um CNPJ.
pub fn cnpj_digits(cnpj: &str) -> String {
    cnpj.chars().filter(char::is_ascii_digit).collect()
}

/// Os 14 dígitos como a CVM escreve (`00.000.000/0001-91`).
pub fn format_cnpj(cnpj: &str) -> Option<String> {
    let d = cnpj_digits(cnpj);
    (d.len() == 14).then(|| {
        format!(
            "{}.{}.{}/{}-{}",
            &d[0..2],
            &d[2..5],
            &d[5..8],
            &d[8..12],
            &d[12..14]
        )
    })
}

/// O fundo do emissor: o que a B3 diz (`b3`, o CNPJ em qualquer formato) ou,
/// sem ela, o desempate entre os que reivindicam o emissor. `None` sem nenhum
/// dos dois.
pub fn choose_fund(claims: &[FundClaim], b3: Option<&str>) -> Option<FundChoice> {
    let rivals = |chosen: &str| -> Vec<String> {
        claims
            .iter()
            .filter(|c| cnpj_digits(&c.cnpj) != cnpj_digits(chosen))
            .map(|c| c.cnpj.clone())
            .collect()
    };
    if let Some(b3) = b3.filter(|b| cnpj_digits(b).len() == 14) {
        let cnpj = claims
            .iter()
            .find(|c| cnpj_digits(&c.cnpj) == cnpj_digits(b3))
            .map(|c| c.cnpj.clone())
            .or_else(|| format_cnpj(b3))?;
        return Some(FundChoice {
            rivals: rivals(&cnpj),
            cnpj,
            by: ChosenBy::B3,
        });
    }
    let newest = claims.iter().map(|c| c.last_reported).max()?;
    let cutoff = months_before(newest, CURRENT_MONTHS);
    let chosen = claims
        .iter()
        .filter(|c| c.last_reported >= cutoff)
        .min_by(|a, b| {
            a.first_reported
                .cmp(&b.first_reported)
                .then(b.last_reported.cmp(&a.last_reported))
                .then(a.cnpj.cmp(&b.cnpj))
        })?;
    let others = rivals(&chosen.cnpj);
    Some(FundChoice {
        cnpj: chosen.cnpj.clone(),
        by: if others.is_empty() {
            ChosenBy::Single
        } else {
            ChosenBy::Oldest
        },
        rivals: others,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    /// O caso de 27/11/2026, com nomes e números fictícios: o fundo antigo
    /// declara o emissor desde 2019 e entregou o informe de setembro; um fundo
    /// novo preencheu o mesmo ISIN e entregou o de outubro.
    fn conflict() -> Vec<FundClaim> {
        vec![
            FundClaim {
                cnpj: "11.111.111/0001-11".into(),
                first_reported: date!(2019 - 08 - 01),
                last_reported: date!(2026 - 09 - 01),
            },
            FundClaim {
                cnpj: "99.999.999/0001-99".into(),
                first_reported: date!(2026 - 10 - 01),
                last_reported: date!(2026 - 10 - 01),
            },
        ]
    }

    #[test]
    fn the_b3_decides_when_it_knows_the_fund() {
        let claims = conflict();
        let chosen = choose_fund(&claims, Some("11111111000111")).unwrap();
        assert_eq!(chosen.cnpj, "11.111.111/0001-11");
        assert_eq!(chosen.by, ChosenBy::B3);
        assert_eq!(chosen.rivals, vec!["99.999.999/0001-99".to_owned()]);
        // A B3 vale mesmo contra o desempate (e com o CNPJ em outro formato).
        let other = choose_fund(&claims, Some("99.999.999/0001-99")).unwrap();
        assert_eq!(other.cnpj, "99.999.999/0001-99");
        // Um CNPJ que nenhum informe tem: vale o da B3, formatado.
        let unseen = choose_fund(&claims, Some("22222222000122")).unwrap();
        assert_eq!(unseen.cnpj, "22.222.222/0001-22");
        assert_eq!(unseen.rivals.len(), 2);
        assert_eq!(
            choose_fund(&[], Some("22222222000122")).unwrap().cnpj,
            "22.222.222/0001-22"
        );
        // CNPJ da B3 que não tem 14 dígitos não vale: o desempate decide.
        assert_eq!(
            choose_fund(&claims, Some("123")).unwrap().by,
            ChosenBy::Oldest
        );
    }

    #[test]
    fn without_the_b3_the_fund_that_declares_the_issuer_longest_wins() {
        let chosen = choose_fund(&conflict(), None).unwrap();
        assert_eq!(chosen.cnpj, "11.111.111/0001-11");
        assert_eq!(chosen.by, ChosenBy::Oldest);
        assert_eq!(chosen.rivals, vec!["99.999.999/0001-99".to_owned()]);
        // A ordem de entrada não muda nada.
        let mut reversed = conflict();
        reversed.reverse();
        assert_eq!(
            choose_fund(&reversed, None).unwrap().cnpj,
            "11.111.111/0001-11"
        );
    }

    #[test]
    fn a_fund_that_became_a_class_leaves_the_old_cnpj_behind() {
        // O CNPJ antigo parou de informar há um ano; o novo é o fundo de hoje.
        let claims = vec![
            FundClaim {
                cnpj: "33.333.333/0001-33".into(),
                first_reported: date!(2015 - 01 - 01),
                last_reported: date!(2025 - 10 - 01),
            },
            FundClaim {
                cnpj: "44.444.444/0001-44".into(),
                first_reported: date!(2025 - 11 - 01),
                last_reported: date!(2026 - 10 - 01),
            },
        ];
        assert_eq!(
            choose_fund(&claims, None).unwrap().cnpj,
            "44.444.444/0001-44"
        );
        // Um mês de atraso no informe ainda é estar em dia.
        let late = vec![
            FundClaim {
                cnpj: "55.555.555/0001-55".into(),
                first_reported: date!(2018 - 01 - 01),
                last_reported: date!(2026 - 08 - 01),
            },
            FundClaim {
                cnpj: "66.666.666/0001-66".into(),
                first_reported: date!(2026 - 10 - 01),
                last_reported: date!(2026 - 10 - 01),
            },
        ];
        assert_eq!(choose_fund(&late, None).unwrap().cnpj, "55.555.555/0001-55");
    }

    #[test]
    fn ties_and_the_single_fund() {
        let one = vec![FundClaim {
            cnpj: "77.777.777/0001-77".into(),
            first_reported: date!(2020 - 01 - 01),
            last_reported: date!(2026 - 10 - 01),
        }];
        let chosen = choose_fund(&one, None).unwrap();
        assert_eq!(chosen.by, ChosenBy::Single);
        assert!(chosen.rivals.is_empty());
        // Mesmo começo: o informe mais novo; depois, o menor CNPJ.
        let tie = vec![
            FundClaim {
                cnpj: "88.888.888/0001-88".into(),
                first_reported: date!(2020 - 01 - 01),
                last_reported: date!(2026 - 10 - 01),
            },
            FundClaim {
                cnpj: "12.121.212/0001-12".into(),
                first_reported: date!(2020 - 01 - 01),
                last_reported: date!(2026 - 10 - 01),
            },
        ];
        assert_eq!(choose_fund(&tie, None).unwrap().cnpj, "12.121.212/0001-12");
        assert_eq!(choose_fund(&[], None), None);
    }

    #[test]
    fn cnpj_formats() {
        assert_eq!(cnpj_digits("11.111.111/0001-11"), "11111111000111");
        assert_eq!(
            format_cnpj("11111111000111").as_deref(),
            Some("11.111.111/0001-11")
        );
        assert_eq!(
            format_cnpj("11.111.111/0001-11").as_deref(),
            Some("11.111.111/0001-11")
        );
        assert_eq!(format_cnpj("123"), None);
    }
}
