//! O título do Tesouro Direto como a carteira o enxerga: um tipo e um
//! vencimento. É isso que o identifica ("Tesouro IPCA+ 2035" é o tipo IPCA+
//! vencendo em 15/05/2035), e dele sai a chave estável que liga o ativo à
//! cotação do dia.
//!
//! Regra pura: sem rede, sem banco. A tabela de preços que o Prumo baixa mora
//! em [`crate::treasury`].

use time::{Date, Month};

/// Prefixo do símbolo de um título. Ticker da B3 e símbolo do Yahoo não têm
/// hífen logo depois de duas letras e um código, então a chave não esbarra em
/// nenhum deles.
pub const SYMBOL_PREFIX: &str = "TD-";

/// Onde a busca diz que o título é negociado.
pub const VENUE: &str = "Tesouro Direto";

/// Os tipos de título que o Tesouro Direto vende.
///
/// A ordem da declaração é a da lista de sugestões: do mais simples ao mais
/// longo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BondKind {
    /// Tesouro Selic (LFT).
    Selic,
    /// Tesouro Prefixado (LTN).
    Prefixed,
    /// Tesouro Prefixado com Juros Semestrais (NTN-F).
    PrefixedWithCoupons,
    /// Tesouro IPCA+ (NTN-B Principal).
    IpcaPlus,
    /// Tesouro IPCA+ com Juros Semestrais (NTN-B).
    IpcaPlusWithCoupons,
    /// Tesouro IGPM+ com Juros Semestrais (NTN-C), já fora de venda.
    IgpmPlusWithCoupons,
    /// Tesouro Renda+ Aposentadoria Extra.
    RendaPlus,
    /// Tesouro Educa+.
    EducaPlus,
}

impl BondKind {
    pub const ALL: [BondKind; 8] = [
        BondKind::Selic,
        BondKind::Prefixed,
        BondKind::PrefixedWithCoupons,
        BondKind::IpcaPlus,
        BondKind::IpcaPlusWithCoupons,
        BondKind::IgpmPlusWithCoupons,
        BondKind::RendaPlus,
        BondKind::EducaPlus,
    ];

    /// O nome do tipo como o Tesouro o escreve, e como a tabela de preços o traz.
    pub fn title(self) -> &'static str {
        match self {
            BondKind::Selic => "Tesouro Selic",
            BondKind::Prefixed => "Tesouro Prefixado",
            BondKind::PrefixedWithCoupons => "Tesouro Prefixado com Juros Semestrais",
            BondKind::IpcaPlus => "Tesouro IPCA+",
            BondKind::IpcaPlusWithCoupons => "Tesouro IPCA+ com Juros Semestrais",
            BondKind::IgpmPlusWithCoupons => "Tesouro IGPM+ com Juros Semestrais",
            BondKind::RendaPlus => "Tesouro Renda+ Aposentadoria Extra",
            BondKind::EducaPlus => "Tesouro Educa+",
        }
    }

    /// O código curto do tipo dentro do símbolo.
    pub fn code(self) -> &'static str {
        match self {
            BondKind::Selic => "SELIC",
            BondKind::Prefixed => "PRE",
            BondKind::PrefixedWithCoupons => "PREJS",
            BondKind::IpcaPlus => "IPCA",
            BondKind::IpcaPlusWithCoupons => "IPCAJS",
            BondKind::IgpmPlusWithCoupons => "IGPMJS",
            BondKind::RendaPlus => "RENDA",
            BondKind::EducaPlus => "EDUCA",
        }
    }

    /// O tipo a partir do nome na tabela de preços (sem diferenciar maiúscula
    /// nem espaço nas pontas).
    pub fn from_title(title: &str) -> Option<Self> {
        let title = title.trim();
        BondKind::ALL
            .into_iter()
            .find(|k| k.title().eq_ignore_ascii_case(title))
    }

    fn from_code(code: &str) -> Option<Self> {
        BondKind::ALL.into_iter().find(|k| k.code() == code)
    }
}

/// Um título: tipo e vencimento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Bond {
    pub kind: BondKind,
    pub maturity: Date,
}

impl Bond {
    pub fn new(kind: BondKind, maturity: Date) -> Self {
        Self { kind, maturity }
    }

    /// A chave estável do título: `TD-IPCA-20350515`. É o símbolo do ativo e
    /// da cotação, com até 18 caracteres (o limite do símbolo é 20).
    pub fn symbol(&self) -> String {
        format!(
            "{SYMBOL_PREFIX}{}-{:04}{:02}{:02}",
            self.kind.code(),
            self.maturity.year(),
            u8::from(self.maturity.month()),
            self.maturity.day()
        )
    }

    /// O título de uma chave. `None` para o que não é uma chave de título:
    /// outro símbolo, tipo desconhecido ou data que não existe.
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        let rest = symbol.strip_prefix(SYMBOL_PREFIX)?;
        let (code, date) = rest.split_once('-')?;
        let kind = BondKind::from_code(code)?;
        if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let year: i32 = date[..4].parse().ok()?;
        let month: u8 = date[4..6].parse().ok()?;
        let day: u8 = date[6..].parse().ok()?;
        let maturity = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
        Some(Self { kind, maturity })
    }

    /// Se o texto tem a forma de uma chave de título.
    pub fn is_symbol(symbol: &str) -> bool {
        Bond::from_symbol(symbol).is_some()
    }

    /// O nome que a pessoa lê: `Tesouro IPCA+ 2035`.
    pub fn name(&self) -> String {
        format!("{} {}", self.kind.title(), self.maturity.year())
    }

    /// O nome com o dia do vencimento, para distinguir dois títulos do mesmo
    /// tipo que vencem no mesmo ano: `Tesouro IPCA+ 2035 (venc. 15/05/2035)`.
    pub fn name_with_date(&self) -> String {
        format!(
            "{} (venc. {:02}/{:02}/{:04})",
            self.name(),
            self.maturity.day(),
            u8::from(self.maturity.month()),
            self.maturity.year()
        )
    }
}

/// Os nomes de uma lista de títulos, sem repetição: o nome curto, e o nome com
/// a data só quando outro título da lista teria o mesmo.
pub fn unique_names(bonds: &[Bond]) -> Vec<String> {
    bonds
        .iter()
        .map(|bond| {
            let twins = bonds
                .iter()
                .filter(|other| {
                    other.kind == bond.kind && other.maturity.year() == bond.maturity.year()
                })
                .count();
            if twins > 1 {
                bond.name_with_date()
            } else {
                bond.name()
            }
        })
        .collect()
}

/// Quantos caracteres a busca precisa ter para sugerir títulos: com menos,
/// "a" sugeriria o catálogo inteiro.
const MIN_QUERY: usize = 3;

/// Se o que a pessoa digitou pede este título: cada palavra da busca aparece
/// no nome (ou em "Tesouro Direto"), em qualquer ordem e sem diferenciar
/// maiúscula. "ipca 2035", "renda+" e "selic" acham o título; "KNRI11" não.
pub fn matches_query(query: &str, name: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.chars().count() < MIN_QUERY {
        return false;
    }
    let haystack = format!("{} {}", name.to_lowercase(), VENUE.to_lowercase());
    query.split_whitespace().all(|word| haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use time::macros::date;

    #[test]
    fn symbols_are_stable_and_fit_the_symbol_limit() {
        let bond = Bond::new(BondKind::IpcaPlus, date!(2035 - 05 - 15));
        assert_eq!(bond.symbol(), "TD-IPCA-20350515");
        let longest = Bond::new(BondKind::IpcaPlusWithCoupons, date!(2060 - 08 - 15));
        assert_eq!(longest.symbol(), "TD-IPCAJS-20600815");
        assert!(longest.symbol().len() <= 20);
        // A chave passa pela normalização de símbolo sem mudar.
        assert_eq!(
            crate::domain::asset_class::normalize_symbol(&longest.symbol()).as_deref(),
            Some("TD-IPCAJS-20600815")
        );
    }

    #[test]
    fn symbols_that_are_not_bond_keys_are_refused() {
        for bad in [
            "",
            "KNRI11",
            "TD-",
            "TD-IPCA",
            "TD-IPCA-",
            "TD-NADA-20350515",
            "TD-IPCA-2035051",
            "TD-IPCA-203505150",
            "TD-IPCA-20351315",
            "TD-IPCA-20350230",
            "TD-IPCA-2035O515",
            "td-ipca-20350515",
            "TD-IPCA-+0350515",
        ] {
            assert_eq!(Bond::from_symbol(bad), None, "{bad:?}");
            assert!(!Bond::is_symbol(bad));
        }
    }

    #[test]
    fn titles_round_trip_through_their_names() {
        for kind in BondKind::ALL {
            assert_eq!(BondKind::from_title(kind.title()), Some(kind));
            assert_eq!(BondKind::from_code(kind.code()), Some(kind));
            assert_eq!(
                BondKind::from_title(&format!("  {}  ", kind.title().to_uppercase())),
                Some(kind)
            );
        }
        assert_eq!(BondKind::from_title("Tesouro Novo"), None);
        assert_eq!(BondKind::from_title(""), None);
    }

    #[test]
    fn names_say_the_type_and_the_year() {
        let bond = Bond::new(BondKind::IpcaPlus, date!(2035 - 05 - 15));
        assert_eq!(bond.name(), "Tesouro IPCA+ 2035");
        assert_eq!(
            bond.name_with_date(),
            "Tesouro IPCA+ 2035 (venc. 15/05/2035)"
        );
        assert_eq!(
            Bond::new(BondKind::RendaPlus, date!(2049 - 12 - 15)).name(),
            "Tesouro Renda+ Aposentadoria Extra 2049"
        );
    }

    #[test]
    fn the_date_enters_the_name_only_when_the_name_would_repeat() {
        let may = Bond::new(BondKind::IpcaPlus, date!(2035 - 05 - 15));
        let aug = Bond::new(BondKind::IpcaPlus, date!(2035 - 08 - 15));
        let other_kind = Bond::new(BondKind::IpcaPlusWithCoupons, date!(2035 - 05 - 15));
        let other_year = Bond::new(BondKind::IpcaPlus, date!(2040 - 08 - 15));
        let names = unique_names(&[may, aug, other_kind, other_year]);
        assert_eq!(
            names,
            [
                "Tesouro IPCA+ 2035 (venc. 15/05/2035)",
                "Tesouro IPCA+ 2035 (venc. 15/08/2035)",
                "Tesouro IPCA+ com Juros Semestrais 2035",
                "Tesouro IPCA+ 2040",
            ]
        );
        assert!(unique_names(&[]).is_empty());
    }

    #[test]
    fn the_search_finds_titles_by_any_word_in_any_order() {
        let ipca = "Tesouro IPCA+ 2035";
        let renda = "Tesouro Renda+ Aposentadoria Extra 2049";
        for query in [
            "tesouro",
            "ipca",
            "IPCA+",
            "ipca 2035",
            "2035 ipca",
            " Tesouro IPCA+ 2035 ",
            "tesouro direto",
        ] {
            assert!(matches_query(query, ipca), "{query:?}");
        }
        assert!(matches_query("renda+", renda));
        assert!(matches_query("aposentadoria", renda));
        assert!(matches_query("selic", "Tesouro Selic 2029"));
        assert!(matches_query("prefixado", "Tesouro Prefixado 2031"));
        assert!(matches_query("educa+", "Tesouro Educa+ 2037"));

        assert!(!matches_query("selic", ipca));
        assert!(!matches_query("ipca 2040", ipca));
        assert!(!matches_query("KNRI11", ipca));
        // Curta demais ou vazia não sugere o catálogo inteiro.
        assert!(!matches_query("", ipca));
        assert!(!matches_query("  ", ipca));
        assert!(!matches_query("te", ipca));
    }

    fn arb_bond() -> impl Strategy<Value = Bond> {
        (0usize..8, 2000i32..2200, 1u8..=12, 1u8..=28).prop_map(|(kind, year, month, day)| {
            Bond::new(
                BondKind::ALL[kind],
                Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap(),
            )
        })
    }

    proptest! {
        #[test]
        fn every_symbol_reads_back_to_the_same_bond(bond in arb_bond()) {
            let symbol = bond.symbol();
            prop_assert!(symbol.len() <= 20);
            prop_assert_eq!(Bond::from_symbol(&symbol), Some(bond));
        }

        #[test]
        fn different_bonds_never_share_a_symbol(a in arb_bond(), b in arb_bond()) {
            prop_assert_eq!(a == b, a.symbol() == b.symbol());
        }

        #[test]
        fn unique_names_never_repeat(bonds in prop::collection::btree_set(arb_bond(), 0..30)) {
            let bonds: Vec<Bond> = bonds.into_iter().collect();
            let names = unique_names(&bonds);
            prop_assert_eq!(names.len(), bonds.len());
            let distinct: std::collections::BTreeSet<_> = names.iter().collect();
            prop_assert_eq!(distinct.len(), names.len());
        }
    }
}
