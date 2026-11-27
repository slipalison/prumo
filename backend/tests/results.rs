//! Resultados trimestrais contra uma CVM falsa (wiremock, arquivos no formato
//! real: zip, CSV com `;` em latin1, o índice com `DT_RECEB`, o ITR com o
//! trimestre e o acumulado) e um Postgres de verdade: a rodada dos
//! fundamentos grava, a rota monta os trimestres de uma indústria, de uma
//! holding e de um banco, e a série mensal de um FII. E a migração 0031 sobre
//! um banco que já tinha fundamentos.
//!
//! Companhias, fundos, códigos CVM e números são inventados.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::http::{Method, StatusCode};
use common::*;
use prumo::agenda::cvm::FilingReceipt;
use prumo::domain::asset_class::AssetClass;
use prumo::domain::fundamentals::FilingKind;
use prumo::fundamentals::b3::{IssuerDirectory, IssuerInfo};
use prumo::fundamentals::download::Validators;
use prumo::fundamentals::FundamentalsService;
use prumo::quotes::Quote;
use prumo::store::agenda::FileVersion;
use prumo::store::{NewAsset, Store};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::macros::datetime;
use time::OffsetDateTime;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NOW: OffsetDateTime = datetime!(2026-11-26 15:00 UTC);

fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

fn zip(files: &[(String, String)]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in files {
            writer.start_file(name.as_str(), options).unwrap();
            writer.write_all(&latin1(content)).unwrap();
        }
        writer.finish().unwrap();
    }
    out.into_inner()
}

const FLOW: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_INI_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";
const BALANCE: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";
const INDEX: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;CATEG_DOC;ID_DOC;DT_RECEB;LINK_DOC";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Industrial,
    Holding,
    Bank,
}

/// Uma companhia fictícia: código, tipo e os trimestres (linha de cima, lucro)
/// de 2025 e 2026, em milhares.
struct Company {
    cvm: i32,
    kind: Kind,
    y2025: [(i64, i64); 4],
    y2026: [(i64, i64); 3],
}

fn companies() -> Vec<Company> {
    vec![
        Company {
            cvm: 99921,
            kind: Kind::Industrial,
            y2025: [(1000, 100), (1100, -50), (1200, 120), (1300, 150)],
            y2026: [(1100, -20), (1250, 80), (1500, 150)],
        },
        Company {
            cvm: 99922,
            kind: Kind::Holding,
            y2025: [(100, 300), (100, 320), (100, 340), (100, 360)],
            y2026: [(110, 350), (110, 380), (120, 400)],
        },
        Company {
            cvm: 99923,
            kind: Kind::Bank,
            y2025: [(500, 100), (520, 110), (540, 120), (560, 130)],
            y2026: [(600, 140), (610, 150), (650, 160)],
        },
    ]
}

const ENDS: [&str; 4] = ["03-31", "06-30", "09-30", "12-31"];
const STARTS: [&str; 4] = ["01-01", "04-01", "07-01", "10-01"];

/// As linhas de resultado de uma coluna (`order`) de um documento de data
/// `refer`, com os `quarters` do exercício `year` até ali: o acumulado e, no
/// ITR do 2º e do 3º trimestre, o trimestre sozinho (que o leitor descarta).
fn result_rows(
    c: &Company,
    refer: &str,
    order: &str,
    year: i32,
    quarters: &[(i64, i64)],
) -> Vec<String> {
    let k = quarters.len();
    let end = format!("{year}-{}", ENDS[k - 1]);
    let mut periods = vec![(format!("{year}-01-01"), quarters.to_vec())];
    if (2..4).contains(&k) {
        periods.push((format!("{year}-{}", STARTS[k - 1]), vec![quarters[k - 1]]));
    }
    let mut rows = Vec::new();
    for (start, part) in periods {
        let top: i64 = part.iter().map(|q| q.0).sum();
        let profit: i64 = part.iter().map(|q| q.1).sum();
        let mut accounts: Vec<(&str, &str, i64)> = match c.kind {
            Kind::Bank => vec![
                ("3.01", "Receitas da Intermediação Financeira", top * 3),
                ("3.03", "Resultado Bruto de Intermediação Financeira", top),
                (
                    "3.09",
                    "Lucro ou Prejuízo Líquido Consolidado do Período",
                    profit,
                ),
                (
                    "3.09.01",
                    "Atribuído a Sócios da Empresa Controladora",
                    profit,
                ),
                ("3.09.02", "Atribuído a Sócios Não Controladores", 0),
            ],
            _ => vec![
                ("3.01", "Receita de Venda de Bens e/ou Serviços", top),
                ("3.11", "Lucro/Prejuízo Consolidado do Período", profit),
                (
                    "3.11.01",
                    "Atribuído a Sócios da Empresa Controladora",
                    profit,
                ),
                ("3.11.02", "Atribuído a Sócios Não Controladores", 0),
            ],
        };
        if c.kind == Kind::Holding {
            accounts.push((
                "3.04.06",
                "Resultado de Equivalência Patrimonial",
                profit * 3 / 2,
            ));
        }
        let cvm = c.cvm;
        for (code, desc, value) in accounts {
            rows.push(format!(
                "00.000.000/0001-00;{refer};1;CIA FICTICIA {cvm} SA;{cvm:06};DF Consolidado;REAL;MIL;{order};{start};{end};{code};{desc};{value}.0000000000;S"
            ));
        }
    }
    rows
}

/// Um `.zip` de DFP ou ITR de `year` com os documentos dados (data, linhas) e
/// o índice: cada documento recebido pela CVM 40 dias depois da data.
fn archive(prefix: &str, year: i32, documents: &[(String, i32, Vec<String>)]) -> Vec<u8> {
    let mut dre = vec![FLOW.to_owned()];
    let mut index = vec![INDEX.to_owned()];
    for (refer, cvm, rows) in documents {
        dre.extend(rows.iter().cloned());
        let date = time::Date::parse(
            refer,
            time::macros::format_description!("[year]-[month]-[day]"),
        )
        .unwrap();
        let received = date + time::Duration::days(40);
        index.push(format!(
            "00.000.000/0001-00;{refer};1;CIA FICTICIA {cvm} SA;{cvm:06};{};1;{received};http://exemplo",
            prefix.to_uppercase()
        ));
    }
    let name = |s: &str| format!("{prefix}_cia_aberta_{s}_{year}.csv");
    zip(&[
        (
            format!("{prefix}_cia_aberta_{year}.csv"),
            index.join("\r\n"),
        ),
        (name("BPA_con"), BALANCE.to_owned()),
        (name("BPP_con"), BALANCE.to_owned()),
        (name("DRE_con"), dre.join("\r\n")),
    ])
}

fn itr(year: i32) -> Vec<u8> {
    let mut documents = Vec::new();
    for c in companies() {
        let current: &[(i64, i64)] = if year == 2026 {
            &c.y2026
        } else {
            &c.y2025[..3]
        };
        for k in 1..=current.len() {
            let refer = format!("{year}-{}", ENDS[k - 1]);
            let mut rows = result_rows(&c, &refer, "ÚLTIMO", year, &current[..k]);
            if year == 2026 {
                rows.extend(result_rows(&c, &refer, "PENÚLTIMO", 2025, &c.y2025[..k]));
            }
            documents.push((refer, c.cvm, rows));
        }
    }
    archive("itr", year, &documents)
}

fn dfp_2025() -> Vec<u8> {
    let documents: Vec<(String, i32, Vec<String>)> = companies()
        .iter()
        .map(|c| {
            (
                "2025-12-31".to_owned(),
                c.cvm,
                result_rows(c, "2025-12-31", "ÚLTIMO", 2025, &c.y2025),
            )
        })
        .collect();
    archive("dfp", 2025, &documents)
}

const REGISTRY: &str = "CNPJ_CIA;DENOM_SOCIAL;DENOM_COMERC;DT_REG;DT_CONST;DT_CANCEL;MOTIVO_CANCEL;SIT;DT_INI_SIT;CD_CVM;SETOR_ATIV;TP_MERC;CATEG_REG;DT_INI_CATEG;SIT_EMISSOR";

fn registry() -> String {
    [
        REGISTRY,
        "00.000.000/0001-00;INDUSTRIA FICTICIA S.A.;UM;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99921;Máquinas, Equipamentos, Veículos e Peças;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL",
        "11.111.111/0001-11;HOLDING FICTICIA S.A.;DOIS;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99922;Emp. Adm. Part. - Sem Setor Principal;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL",
        "22.222.222/0001-22;BANCO FICTICIO S.A.;TRES;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99923;Bancos;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL",
    ]
    .join("\r\n")
}

fn fund_monthly_2026() -> Vec<u8> {
    let general = [
        "Tipo_Fundo_Classe;CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe;Data_Funcionamento;Publico_Alvo;Codigo_ISIN;Quantidade_Cotas_Emitidas",
        "FII;33.333.333/0001-33;2026-10-01;1;2026-11-10;FUNDO FICTICIO FII;2010-01-01;Geral;BRFICTCTF000;10000",
    ]
    .join("\r\n");
    let complement = [
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Informacao_Numero_Cotistas;Total_Numero_Cotistas;Valor_Ativo;Patrimonio_Liquido;Cotas_Emitidas;Valor_Patrimonial_Cotas;Percentual_Rentabilidade_Efetiva_Mes;Percentual_Rentabilidade_Patrimonial_Mes;Percentual_Dividend_Yield_Mes",
        "33.333.333/0001-33;2026-08-01;1;2026-08-01;10;0;1000000;10000;100;0.008;0.001;0.007",
        // O fundo escreveu 0,89 querendo dizer 0,89%: fora do possível.
        "33.333.333/0001-33;2026-09-01;1;2026-09-01;10;0;1010000;10000;101;0.9;0.01;0.89",
        "33.333.333/0001-33;2026-10-01;1;2026-10-01;10;0;990000;10000;99;0.005;-0.002;0.0069",
    ]
    .join("\r\n");
    zip(&[
        ("inf_mensal_fii_complemento_2026.csv".to_owned(), complement),
        ("inf_mensal_fii_geral_2026.csv".to_owned(), general),
    ])
}

async fn fake_cvm() -> MockServer {
    let server = MockServer::start().await;
    let files: Vec<(String, Vec<u8>)> = vec![
        (
            "/dados/CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv".into(),
            latin1(&registry()),
        ),
        (
            "/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_2025.zip".into(),
            dfp_2025(),
        ),
        (
            "/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_2025.zip".into(),
            itr(2025),
        ),
        (
            "/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_2026.zip".into(),
            itr(2026),
        ),
        (
            "/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2026.zip".into(),
            fund_monthly_2026(),
        ),
    ];
    for (route, body) in files {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_bytes(body),
            )
            .mount(&server)
            .await;
    }
    server
}

struct FakeCodes(HashMap<&'static str, Option<i32>>);

#[async_trait]
impl IssuerDirectory for FakeCodes {
    async fn issuer(&self, issuer: &str) -> Result<IssuerInfo, String> {
        let code = *self
            .0
            .get(issuer)
            .ok_or_else(|| "B3 fora do ar".to_owned())?;
        Ok(IssuerInfo {
            cvm_code: code,
            total_shares: code.map(|_| dec!(1000)),
        })
    }
}

async fn holding(
    store: &Store,
    user: Uuid,
    class: AssetClass,
    symbol: &str,
    price: rust_decimal::Decimal,
) {
    store
        .insert_asset(
            user,
            &NewAsset {
                asset_class: class,
                symbol: Some(symbol.to_owned()),
                display_name: symbol.to_owned(),
                quantity: dec!(10),
                manual_price: None,
                score_override: None,
                listed_on_b3: true,
                treasury: false,
                ceiling: prumo::domain::ceiling::CeilingChoice::None,
            },
        )
        .await
        .unwrap();
    store
        .upsert_quote(
            &Quote {
                symbol: format!("{symbol}.SA"),
                price,
                currency: "BRL".into(),
                name: None,
                traded_at: None,
            },
            NOW,
        )
        .await
        .unwrap();
}

fn find<'a>(assets: &'a [Value], symbol: &str) -> &'a Value {
    assets
        .iter()
        .find(|a| a["symbol"] == symbol)
        .unwrap_or_else(|| panic!("sem {symbol}"))
}

#[tokio::test]
async fn the_route_builds_the_quarters_of_the_portfolio() {
    let db = database().await;
    let cvm = fake_cvm().await;
    let user = db
        .store
        .ensure_user("resultados@teste.local")
        .await
        .unwrap();
    holding(&db.store, user, AssetClass::BrStocks, "UMUM3", dec!(20)).await;
    holding(&db.store, user, AssetClass::BrStocks, "HOLD4", dec!(10)).await;
    holding(&db.store, user, AssetClass::BrStocks, "BANC4", dec!(30)).await;
    holding(&db.store, user, AssetClass::BrReits, "FICT11", dec!(90)).await;
    let codes = Arc::new(FakeCodes(HashMap::from([
        ("UMUM", Some(99921)),
        ("HOLD", Some(99922)),
        ("BANC", Some(99923)),
    ])));
    let report = FundamentalsService::new(db.store.clone(), reqwest::Client::new(), codes)
        .with_base(&cvm.uri())
        .with_timing(Duration::ZERO, Duration::ZERO)
        .round(NOW)
        .await;
    assert_eq!(report.failed, 0, "{report:?}");
    // A entrega de cada documento, como a Agenda grava do índice dos arquivos
    // (`cvm_filing_receipts`): 40 dias depois da data de referência.
    let receipts: Vec<FilingReceipt> = companies()
        .iter()
        .flat_map(|c| {
            [
                (FilingKind::Annual, time::macros::date!(2025 - 12 - 31)),
                (FilingKind::Quarterly, time::macros::date!(2026 - 09 - 30)),
            ]
            .map(|(kind, reference_date)| FilingReceipt {
                cvm_code: c.cvm,
                kind,
                reference_date,
                version: 1,
                received_on: reference_date + time::Duration::days(40),
                url: None,
            })
        })
        .collect();
    db.store
        .save_filing_receipts(
            FileVersion {
                key: "receipts_quarterly:2026",
                validators: &Validators::default(),
                scanned: &BTreeSet::new(),
                now: NOW,
            },
            &receipts,
        )
        .await
        .unwrap();

    let app = app(
        &db.store,
        "resultados@teste.local",
        FakeQuotes::with_market(),
        "static",
    );
    let reply = call(&app.router, Method::GET, "/api/v1/results", None).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let assets = reply.body["assets"].as_array().unwrap().clone();
    assert_eq!(assets.len(), 4);

    // A indústria: sete trimestres (2025 inteiro e três de 2026), em reais.
    let industrial = find(&assets, "UMUM3");
    assert_eq!(industrial["kind"], "company");
    // 200 de 1.500 da carteira.
    assert_eq!(industrial["weight"], 0.133333);
    let c = &industrial["company"];
    assert_eq!(c["top_line"], "revenue");
    assert_eq!(c["holding"], false);
    let quarters = c["quarters"].as_array().unwrap();
    assert_eq!(quarters.len(), 7);
    // O 4º trimestre de 2025: a DFP (4.600) menos os nove meses (3.300).
    assert_eq!(quarters[3]["period_end"], "2025-12-31");
    assert_eq!(quarters[3]["method"], "annual_less_nine_months");
    assert_eq!(quarters[3]["top_line"]["value"], 1_300_000.0);
    assert_eq!(quarters[3]["net_income"]["value"], 150_000.0);
    assert_eq!(quarters[3]["source"]["kind"], "annual");
    assert_eq!(quarters[3]["received_on"], "2026-02-09");
    let latest = &quarters[6];
    assert_eq!(latest["period_start"], "2026-07-01");
    assert_eq!(latest["period_end"], "2026-09-30");
    assert_eq!(latest["fiscal_quarter"], 3);
    assert_eq!(latest["top_line"]["value"], 1_500_000.0);
    assert_eq!(latest["net_income"]["value"], 150_000.0);
    assert_eq!(latest["net_margin"]["value"], 0.1);
    assert_eq!(latest["received_on"], "2026-11-09");
    // Um ano antes: 1.200 e 120 (lucro +25%, receita +25%, margem igual).
    assert_eq!(c["year_ago"]["period_end"], "2025-09-30");
    assert_eq!(
        c["year_ago"]["top_line"],
        json!({"kind": "percent", "value": 0.25, "gap": null})
    );
    assert_eq!(c["year_ago"]["net_income"]["value"], 0.25);
    assert_eq!(c["year_ago"]["net_margin_points"]["value"], 0.0);
    // O trimestre anterior (80 de lucro sobre 1.250): +87,5%.
    assert_eq!(c["previous_quarter"]["net_income"]["value"], 0.875);
    // O 1º de 2026 (prejuízo de 20) contra o 4º de 2025 (lucro de 150).
    assert_eq!(quarters[4]["net_income"]["value"], -20_000.0);
    assert_eq!(quarters[1]["net_income"]["value"], -50_000.0);

    // A holding: receita e lucro, mas margem "não se aplica".
    let holding = &find(&assets, "HOLD4")["company"];
    assert_eq!(holding["holding"], true);
    let last = holding["quarters"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["top_line"]["value"], 120_000.0);
    assert_eq!(last["net_income"]["value"], 400_000.0);
    assert_eq!(last["net_margin"], json!({"value": null, "gap": "holding"}));
    assert_eq!(holding["year_ago"]["net_margin_points"]["gap"], "holding");
    // E nos fundamentos, a mesma régua.
    let fundamentals = call(&app.router, Method::GET, "/api/v1/fundamentals", None).await;
    let hold = find(fundamentals.body["assets"].as_array().unwrap(), "HOLD4");
    assert_eq!(hold["company"]["holding"], true);
    assert_eq!(hold["company"]["trailing"]["net_margin"]["gap"], "holding");
    assert_eq!(hold["company"]["revenue_cagr"]["gap"], "holding");
    let industry = find(fundamentals.body["assets"].as_array().unwrap(), "UMUM3");
    assert_eq!(industry["company"]["holding"], false);

    // O banco: a linha de cima é o resultado bruto da intermediação (3.03).
    let bank = &find(&assets, "BANC4")["company"];
    assert_eq!(bank["template"], "bank");
    assert_eq!(bank["top_line"], "intermediation_result");
    let last = bank["quarters"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["top_line"]["value"], 650_000.0);
    assert_eq!(last["net_margin"]["gap"], "financial_template");
    assert_eq!(bank["year_ago"]["net_income"]["value"], 0.333333);

    // O FII: a série do informe mensal, com o mês impossível dito como tal.
    let fund = find(&assets, "FICT11");
    assert_eq!(fund["kind"], "fund");
    assert_eq!(fund["weight"], 0.6);
    let months = fund["fund"]["months"].as_array().unwrap();
    assert_eq!(months.len(), 3);
    assert_eq!(months[0]["reference_date"], "2026-08-01");
    assert_eq!(months[0]["dividend_yield"]["value"], 0.007);
    assert_eq!(months[0]["nav_per_share"]["value"], 100.0);
    assert_eq!(months[1]["dividend_yield"]["gap"], "implausible");
    assert_eq!(months[2]["nav_per_share"]["value"], 99.0);
}

#[tokio::test]
async fn the_route_is_empty_without_a_portfolio() {
    let db = database().await;
    let app = app(
        &db.store,
        "vazia-resultados@teste.local",
        FakeQuotes::with_market(),
        "static",
    );
    let reply = call(&app.router, Method::GET, "/api/v1/results", None).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["assets"], json!([]));
}

/// Uma linha de `cvm_sources`: o arquivo, os códigos lidos, a conferência e o ETag.
type SourceRow = (String, Vec<i32>, Option<OffsetDateTime>, Option<String>);

/// Produção já tem fundamentos gravados antes da 0031: monta o banco até a
/// 0030, grava um documento e o estado dos arquivos, aplica a 0031 e confere
/// que o documento continua, que os arquivos de companhia serão lidos de novo
/// inteiros (para a conta de equivalência) e o informe mensal também.
#[tokio::test]
async fn migration_0031_keeps_the_filings_and_asks_for_a_full_reread() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(30, &pool).await.unwrap();

    sqlx::query(
        "insert into cvm_filings (cvm_code, kind, reference_date, version, scope, company_name, source, fetched_at)
         values (99921, 'quarterly', '2026-09-30', 1, 'consolidated', 'CIA FICTICIA SA', 'quarterly:2026', now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (source, scanned) in [
        ("annual:2025", "{99921}"),
        ("quarterly:2026", "{99921}"),
        ("fund_monthly:2026", "{}"),
        ("registry", "{}"),
    ] {
        sqlx::query(
            "insert into cvm_sources (source, etag, scanned_codes, attempted_at, checked_at)
             values ($1, '\"v1\"', $2::int[], now(), now())",
        )
        .bind(source)
        .bind(scanned)
        .execute(&pool)
        .await
        .unwrap();
    }

    migrator.run(&pool).await.unwrap();

    let filing: (String,) =
        sqlx::query_as("select company_name from cvm_filings where cvm_code = 99921")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(filing.0, "CIA FICTICIA SA");
    let states: Vec<SourceRow> = sqlx::query_as(
        "select source, scanned_codes, checked_at, etag from cvm_sources order by source",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for (source, scanned, checked, etag) in states {
        match source.as_str() {
            "annual:2025" | "quarterly:2026" => {
                assert!(scanned.is_empty(), "{source}");
                assert!(checked.is_some(), "{source}");
            }
            "fund_monthly:2026" => assert!(checked.is_none()),
            _ => assert!(checked.is_some()),
        }
        assert_eq!(etag.as_deref(), Some("\"v1\""), "{source}");
    }
    let months: (i64,) = sqlx::query_as("select count(*) from cvm_fund_months")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(months.0, 0);
}
