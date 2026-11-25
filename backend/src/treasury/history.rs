//! O histórico de preço de cada título do Tesouro Direto, para a série
//! retroativa do Desempenho.
//!
//! O CSV do Tesouro Transparente traz o preço de todos os títulos em todos os
//! dias desde 2004 (uns 15 MB). O Prumo lê o arquivo em fluxo, como na cotação
//! do dia, e guarda só as linhas dos títulos que alguma carteira tem, a partir
//! da data em que cada um entrou no livro — o resto passa e é esquecido. O
//! preço é o mesmo da cotação: o de resgate (`PU Venda Manha`).
//!
//! Quem decide quando baixar é a sincronia do Desempenho
//! (`performance.rs`): só quando falta data antes do que já está guardado.

use std::collections::BTreeMap;

use async_trait::async_trait;
use rust_decimal::Decimal;
use time::Date;

use super::{parse_date, parse_price, Columns, FeedError, TransparenteFeed};
use crate::domain::treasury::{Bond, BondKind};
use crate::history::Series;

/// De onde vem o histórico. Atrás de um trait para os testes não dependerem
/// da rede.
#[async_trait]
pub trait TreasuryHistory: Send + Sync {
    /// O preço de resgate de cada título pedido em cada data-base, desde a
    /// data pedida (inclusive) até a última divulgação. Título sem linha no
    /// período volta com a série vazia.
    async fn history(
        &self,
        wanted: &BTreeMap<Bond, Date>,
    ) -> Result<BTreeMap<Bond, Series>, FeedError>;
}

/// Lê a tabela linha a linha guardando só o que foi pedido.
pub struct HistoryBoard<'a> {
    wanted: &'a BTreeMap<Bond, Date>,
    columns: Option<Columns>,
    rows: BTreeMap<Bond, BTreeMap<Date, Decimal>>,
}

impl<'a> HistoryBoard<'a> {
    pub fn new(wanted: &'a BTreeMap<Bond, Date>) -> Self {
        Self {
            wanted,
            columns: None,
            rows: BTreeMap::new(),
        }
    }

    /// Uma linha do arquivo. A primeira não vazia é o cabeçalho; linha que não
    /// dá para ler é pulada (a cotação do dia já avisa no log).
    pub fn push_line(&mut self, line: &str) -> Result<(), FeedError> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Ok(());
        }
        let Some(columns) = self.columns else {
            self.columns = Some(Columns::from_header(line)?);
            return Ok(());
        };
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() <= columns.widest {
            return Ok(());
        }
        let Some(kind) = BondKind::from_title(fields[columns.title]) else {
            return Ok(());
        };
        let Some(maturity) = parse_date(fields[columns.maturity]) else {
            return Ok(());
        };
        let bond = Bond::new(kind, maturity);
        let Some(since) = self.wanted.get(&bond) else {
            return Ok(());
        };
        let (Some(base_date), Some(price)) = (
            parse_date(fields[columns.base_date]),
            parse_price(fields[columns.redemption]),
        ) else {
            return Ok(());
        };
        // Preço zero: título fora de venda no dia, sem preço de resgate.
        if base_date >= *since && price > Decimal::ZERO {
            self.rows.entry(bond).or_default().insert(base_date, price);
        }
        Ok(())
    }

    /// O fim do arquivo. Sem cabeçalho é erro (página de erro, formato novo).
    pub fn finish(self) -> Result<BTreeMap<Bond, Series>, FeedError> {
        if self.columns.is_none() {
            return Err(FeedError::Malformed("arquivo vazio".into()));
        }
        let mut rows = self.rows;
        Ok(self
            .wanted
            .keys()
            .map(|bond| {
                let series: Series = rows.remove(bond).unwrap_or_default().into_iter().collect();
                (*bond, series)
            })
            .collect())
    }
}

#[async_trait]
impl TreasuryHistory for TransparenteFeed {
    async fn history(
        &self,
        wanted: &BTreeMap<Bond, Date>,
    ) -> Result<BTreeMap<Bond, Series>, FeedError> {
        let path = self.csv_path().await;
        let mut board = HistoryBoard::new(wanted);
        self.for_each_line(&path, |line| board.push_line(line))
            .await?;
        board.finish()
    }
}

/// Sem rede (`QUOTES_ENABLED=false`): nada de histórico; o título vale pelo
/// que as fotos guardaram.
pub struct DisabledTreasuryHistory;

#[async_trait]
impl TreasuryHistory for DisabledTreasuryHistory {
    async fn history(&self, _: &BTreeMap<Bond, Date>) -> Result<BTreeMap<Bond, Series>, FeedError> {
        Err(FeedError::Unavailable(
            "historico do tesouro desligado".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const HEADER: &str = "Tipo Titulo;Data Vencimento;Data Base;Taxa Compra Manha;Taxa Venda Manha;PU Compra Manha;PU Venda Manha;PU Base Manha";

    #[test]
    fn keeps_only_the_wanted_bonds_from_the_wanted_date() {
        let ipca = Bond::new(BondKind::IpcaPlus, date!(2035 - 05 - 15));
        let selic = Bond::new(BondKind::Selic, date!(2029 - 03 - 01));
        let wanted = BTreeMap::from([
            (ipca, date!(2026 - 11 - 18)),
            (selic, date!(2026 - 11 - 19)),
        ]);
        let mut board = HistoryBoard::new(&wanted);
        for line in [
            HEADER,
            "Tesouro IPCA+;15/05/2035;19/11/2026;7,86;7,98;2526,53;2500,95;2500,95\r\n",
            "Tesouro IPCA+;15/05/2035;18/11/2026;7,77;7,89;2542,31;2517,51;2517,51",
            // Antes da data pedida: fica de fora.
            "Tesouro IPCA+;15/05/2035;17/11/2026;7,77;7,89;2542,31;2510,00;2510,00",
            // Título que ninguém tem.
            "Tesouro Prefixado;01/01/2027;19/11/2026;13,56;13,68;985,97;985,35;985,35",
            // Fora de venda no dia, ilegível, curta, tipo desconhecido.
            "Tesouro Selic;01/03/2029;19/11/2026;0,05;0,06;0,00;0,00;0,00",
            "Tesouro Selic;01/03/2029;sem data;0,05;0,06;1;1;1",
            "Tesouro Selic;curta",
            "Tesouro Novo;01/01/2040;19/11/2026;1;1;1;1;1",
            "",
        ] {
            board.push_line(line).unwrap();
        }
        let history = board.finish().unwrap();
        assert_eq!(
            history[&ipca],
            vec![
                (date!(2026 - 11 - 18), dec!(2517.51)),
                (date!(2026 - 11 - 19), dec!(2500.95))
            ]
        );
        assert!(history[&selic].is_empty());
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn a_file_without_header_is_an_error() {
        let wanted = BTreeMap::new();
        assert!(matches!(
            HistoryBoard::new(&wanted).finish(),
            Err(FeedError::Malformed(_))
        ));
        let mut board = HistoryBoard::new(&wanted);
        assert!(board.push_line("<html>erro</html>").is_err());
    }

    #[tokio::test]
    async fn disabled_history_fetches_nothing() {
        assert!(DisabledTreasuryHistory
            .history(&BTreeMap::new())
            .await
            .is_err());
    }
}
