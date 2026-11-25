//! Perguntas que toda carteira nova recebe. A pessoa edita, remove e
//! acrescenta; "restaurar padrão" volta a esta lista.
//!
//! O texto é o que a pessoa lê, então vai com acento — ele só sai do servidor
//! como dado do banco, nunca em log.

use super::asset_class::Checklist;

const STOCKS: &[&str] = &[
    "Teve lucro líquido em pelo menos 4 dos últimos 5 anos?",
    "O ROE médio dos últimos 5 anos ficou acima de 10%?",
    "Receita ou lucro cresceram acima da inflação nos últimos 5 anos?",
    "A dívida líquida é menor que 2 vezes o EBITDA?",
    "Paga dividendos há mais de 5 anos sem interromper?",
    "Está entre as líderes do setor em que atua?",
    "Tem governança sólida, sem escândalos recentes?",
    "O setor é perene, sem risco claro de ficar obsoleto?",
    "É livre de controle estatal e de dependência de um único cliente?",
    "O preço está abaixo da média histórica do próprio P/L?",
];

const REAL_ESTATE: &[&str] = &[
    "Os imóveis ficam em regiões de alta demanda?",
    "Paga rendimentos todo mês há mais de 4 anos, sem cortes bruscos?",
    "Os imóveis são modernos, sem reforma pesada à vista?",
    "O dividend yield está na média do segmento ou acima?",
    "Nenhum inquilino ou imóvel passa de 25% da receita?",
    "A vacância está abaixo de 10%?",
    "Está negociado abaixo do valor patrimonial (P/VP menor que 1)?",
    "A gestora tem bom histórico e taxa de administração de até 1% ao ano?",
];

pub fn questions(checklist: Checklist) -> &'static [&'static str] {
    match checklist {
        Checklist::Stocks => STOCKS,
        Checklist::RealEstate => REAL_ESTATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_a_short_question() {
        for checklist in Checklist::ALL {
            let list = questions(checklist);
            assert!(list.len() >= 6);
            for prompt in list {
                assert!(prompt.ends_with('?'));
                assert!(prompt.chars().count() <= 120);
            }
        }
    }
}
