//! Fundamentos contra uma CVM falsa (wiremock, arquivos no formato real:
//! zip, CSV com `;` em latin1, `ÚLTIMO`/`PENÚLTIMO`) e um Postgres de verdade:
//! a rodada que lê só as companhias da carteira, o GET condicional (304), a
//! companhia nova que obriga a ler de novo, a fonte fora do ar que guarda o
//! último bom, duas réplicas ao mesmo tempo, a reapresentação e a rota.
//!
//! Companhias, fundos, códigos CVM e números são inventados.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::http::{Method, StatusCode};
use common::*;
use prumo::domain::asset_class::AssetClass;
use prumo::domain::fundamentals::{
    AssetFundamentals, Column, Filing, FilingKind, Gap, Line, Scope, Statement, Template,
    Unsupported,
};
use prumo::fundamentals::b3::{parse_issuer, B3Issuers, IssuerDirectory, IssuerInfo};
use prumo::fundamentals::download::Validators;
use prumo::fundamentals::FundamentalsService;
use prumo::quotes::Quote;
use prumo::store::{NewAsset, Store};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use time::macros::{date, datetime};
use time::OffsetDateTime;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const NOW: OffsetDateTime = datetime!(2026-11-26 15:00 UTC);

// ------------------------------------------------------------ arquivos --

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

/// Os números (em milhares) de um exercício de uma companhia fictícia.
#[derive(Clone, Copy)]
struct Numbers {
    revenue: i64,
    profit: i64,
    equity: i64,
}

/// As linhas de um exercício (coluna `order`) de uma DFP com data `refer`.
fn rows(
    cvm: i32,
    refer: &str,
    version: u32,
    order: &str,
    year: i32,
    n: Numbers,
) -> [Vec<String>; 5] {
    let start = format!("{year}-01-01");
    let end = format!("{year}-12-31");
    let flow = |code: &str, desc: &str, value: i64| {
        format!("00.000.000/0001-00;{refer};{version};CIA FICTICIA {cvm} SA;{cvm:06};DF Consolidado;REAL;MIL;{order};{start};{end};{code};{desc};{value}.0000000000;S")
    };
    let balance = |code: &str, desc: &str, value: i64| {
        format!("00.000.000/0001-00;{refer};{version};CIA FICTICIA {cvm} SA;{cvm:06};DF Consolidado;REAL;MIL;{order};{end};{code};{desc};{value}.0000000000;S")
    };
    [
        vec![
            flow("3.01", "Receita de Venda de Bens e/ou Serviços", n.revenue),
            flow(
                "3.05",
                "Resultado Antes do Resultado Financeiro e dos Tributos",
                n.profit * 3 / 2,
            ),
            flow("3.11", "Lucro/Prejuízo Consolidado do Período", n.profit),
            flow(
                "3.11.01",
                "Atribuído a Sócios da Empresa Controladora",
                n.profit,
            ),
        ],
        vec![
            balance("1.01.01", "Caixa e Equivalentes de Caixa", 200),
            balance("1.01.02", "Aplicações Financeiras", 100),
        ],
        vec![
            balance("2.01.04", "Empréstimos e Financiamentos", 300),
            balance("2.02.01", "Empréstimos e Financiamentos", 700),
            balance("2.03", "Patrimônio Líquido Consolidado", n.equity),
        ],
        vec![flow(
            "6.01",
            "Caixa Líquido Atividades Operacionais",
            n.profit + 50,
        )],
        vec![flow("7.04.01", "Depreciação, Amortização e Exaustão", -100)],
    ]
}

/// Uma DFP do ano com as companhias dadas (código, números do ano, do anterior).
fn dfp(year: i32, companies: &[(i32, u32, Numbers, Numbers)]) -> Vec<u8> {
    let mut dre = vec![FLOW.to_owned()];
    let mut bpa = vec![BALANCE.to_owned()];
    let mut bpp = vec![BALANCE.to_owned()];
    let mut dfc = vec![FLOW.to_owned()];
    let mut dva = vec![FLOW.to_owned()];
    let refer = format!("{year}-12-31");
    for (cvm, version, current, previous) in companies {
        for (order, y, n) in [("ÚLTIMO", year, current), ("PENÚLTIMO", year - 1, previous)] {
            let [a, b, c, d, e] = rows(*cvm, &refer, *version, order, y, *n);
            dre.extend(a);
            bpa.extend(b);
            bpp.extend(c);
            dfc.extend(d);
            dva.extend(e);
        }
    }
    let name = |s: &str| format!("dfp_cia_aberta_{s}_{year}.csv");
    zip(&[
        (
            format!("dfp_cia_aberta_{year}.csv"),
            "CNPJ_CIA;DT_REFER\r\n".to_owned(),
        ),
        (name("BPA_con"), bpa.join("\r\n")),
        (name("BPA_ind"), BALANCE.to_owned()),
        (name("BPP_con"), bpp.join("\r\n")),
        (name("BPP_ind"), BALANCE.to_owned()),
        (name("DFC_MI_con"), dfc.join("\r\n")),
        (name("DMPL_con"), "muita coisa que nao e lida".to_owned()),
        (name("DRE_con"), dre.join("\r\n")),
        (name("DRE_ind"), FLOW.to_owned()),
        (name("DVA_con"), dva.join("\r\n")),
    ])
}

fn numbers(revenue: i64, profit: i64) -> Numbers {
    Numbers {
        revenue,
        profit,
        equity: 5000,
    }
}

const REGISTRY: &str = "CNPJ_CIA;DENOM_SOCIAL;DENOM_COMERC;DT_REG;DT_CONST;DT_CANCEL;MOTIVO_CANCEL;SIT;DT_INI_SIT;CD_CVM;SETOR_ATIV;TP_MERC;CATEG_REG;DT_INI_CATEG;SIT_EMISSOR";

fn registry() -> String {
    [
        REGISTRY,
        "00.000.000/0001-00;CIA FICTICIA UM S.A.;UM;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99901;Máquinas;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL",
        "11.111.111/0001-11;CIA FICTICIA DOIS S.A.;DOIS;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99902;Energia;BOLSA;Categoria A;2010-01-01;EM RECUPERAÇÃO JUDICIAL OU EQUIVALENTE",
    ]
    .join("\r\n")
}

fn fund_monthly(year: i32) -> Vec<u8> {
    let general = [
        "Tipo_Fundo_Classe;CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe;Data_Funcionamento;Publico_Alvo;Codigo_ISIN;Quantidade_Cotas_Emitidas",
        &format!("FII;22.222.222/0001-22;{year}-10-01;1;{year}-11-10;FUNDO FICTICIO FII;2010-01-01;Geral;BRFICTCTF000;10000"),
    ]
    .join("\r\n");
    let complement = [
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Informacao_Numero_Cotistas;Total_Numero_Cotistas;Valor_Ativo;Patrimonio_Liquido;Cotas_Emitidas;Valor_Patrimonial_Cotas",
        &format!("22.222.222/0001-22;{year}-10-01;1;{year}-10-01;10;0;1000000;10000;100"),
    ]
    .join("\r\n");
    zip(&[
        (format!("inf_mensal_fii_complemento_{year}.csv"), complement),
        (format!("inf_mensal_fii_geral_{year}.csv"), general),
    ])
}

fn fund_quarterly(year: i32) -> Vec<u8> {
    let general = [
        "Tipo_Fundo_Classe;CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe",
        &format!("FII;22.222.222/0001-22;{year}-09-30;1;{year}-11-01;FUNDO FICTICIO FII"),
    ]
    .join("\r\n");
    let properties = [
        "CNPJ_Fundo_Classe;Data_Referencia;Versao;Classe;Nome_Imovel;Endereco;Area;Numero_Unidades;Outras_Caracteristicas_Relevantes;Percentual_Vacancia",
        &format!("22.222.222/0001-22;{year}-09-30;1;Imóveis para renda acabados;Galpão A;Rua X;1000;1;;0"),
        &format!("22.222.222/0001-22;{year}-09-30;1;Imóveis para renda acabados;Galpão B;\"Bairro\", Cidade;3000;1;;0.2"),
    ]
    .join("\r\n");
    zip(&[
        (format!("inf_trimestral_fii_geral_{year}.csv"), general),
        (format!("inf_trimestral_fii_imovel_{year}.csv"), properties),
    ])
}

/// Cinco exercícios da companhia 99901 (receita e lucro crescendo 10% ao ano).
fn annual(year: i32, extra: bool) -> Vec<u8> {
    let i = i64::from(year - 2021);
    let revenue = |i: i64| [1000, 1100, 1210, 1331, 1464][usize::try_from(i.clamp(0, 4)).unwrap()];
    let profit = |i: i64| revenue(i) / 10;
    let mut companies = vec![(
        99901,
        1,
        numbers(revenue(i), profit(i)),
        numbers(revenue(i - 1), profit(i - 1)),
    )];
    if extra {
        companies.push((
            99902,
            1,
            Numbers {
                revenue: 500,
                profit: -80,
                equity: -200,
            },
            numbers(400, 10),
        ));
    }
    dfp(year, &companies)
}

// ------------------------------------------------------------ CVM falsa --

/// Uma CVM de mentira: cada arquivo responde com ETag, 304 quando o pedido
/// traz o ETag atual, e conta as respostas 200.
struct FakeCvm {
    server: MockServer,
}

impl FakeCvm {
    async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    async fn serve(&self, route: &str, body: Vec<u8>, etag: &str) {
        Mock::given(method("GET"))
            .and(path(route.to_owned()))
            .and(header("if-none-match", etag))
            .respond_with(ResponseTemplate::new(304))
            .with_priority(1)
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path(route.to_owned()))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", etag)
                    .insert_header("last-modified", "Sun, 22 Nov 2026 10:22:30 GMT")
                    .set_body_bytes(body),
            )
            .with_priority(2)
            .mount(&self.server)
            .await;
    }

    async fn standard(&self, extra: bool) {
        self.serve(
            "/dados/CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv",
            latin1(&registry()),
            "\"cad-1\"",
        )
        .await;
        for year in 2021..=2025 {
            self.serve(
                &format!("/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_{year}.zip"),
                annual(year, extra),
                &format!("\"dfp-{year}\""),
            )
            .await;
        }
        // 2026 e os ITR: sem arquivo (404), como em janeiro.
        for year in [2025, 2026] {
            self.serve(
                &format!("/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_{year}.zip"),
                fund_monthly(year),
                &format!("\"m-{year}\""),
            )
            .await;
            self.serve(
                &format!("/dados/FII/DOC/INF_TRIMESTRAL/DADOS/inf_trimestral_fii_{year}.zip"),
                fund_quarterly(year),
                &format!("\"t-{year}\""),
            )
            .await;
        }
    }

    /// Quantas vezes o arquivo foi baixado (200) e conferido (com ETag).
    async fn hits(&self, route: &str) -> (usize, usize) {
        let requests: Vec<Request> = self.server.received_requests().await.unwrap_or_default();
        let mine: Vec<&Request> = requests.iter().filter(|r| r.url.path() == route).collect();
        let conditional = mine
            .iter()
            .filter(|r| r.headers.contains_key("if-none-match"))
            .count();
        (mine.len() - conditional, conditional)
    }
}

const DFP_2025: &str = "/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_2025.zip";

#[derive(Default)]
struct FakeCodes {
    codes: Mutex<HashMap<String, Option<i32>>>,
    calls: AtomicUsize,
}

impl FakeCodes {
    fn with(pairs: &[(&str, Option<i32>)]) -> Arc<Self> {
        let fake = FakeCodes::default();
        fake.codes
            .lock()
            .unwrap()
            .extend(pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)));
        Arc::new(fake)
    }
}

#[async_trait]
impl IssuerDirectory for FakeCodes {
    async fn issuer(&self, issuer: &str) -> Result<IssuerInfo, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let code = self
            .codes
            .lock()
            .unwrap()
            .get(issuer)
            .copied()
            .ok_or_else(|| "B3 fora do ar".to_owned())?;
        // Mil ações por companhia (fictício).
        Ok(IssuerInfo {
            cvm_code: code,
            total_shares: code.map(|_| dec!(1000)),
        })
    }
}

fn service(store: &Store, cvm: &FakeCvm, codes: Arc<FakeCodes>) -> FundamentalsService {
    FundamentalsService::new(store.clone(), reqwest::Client::new(), codes)
        .with_base(&cvm.server.uri())
        .with_timing(Duration::ZERO, Duration::ZERO)
}

async fn asset(store: &Store, user: Uuid, class: AssetClass, symbol: Option<&str>, listed: bool) {
    store
        .insert_asset(
            user,
            &NewAsset {
                asset_class: class,
                symbol: symbol.map(str::to_owned),
                display_name: symbol.unwrap_or("CDB fictício").to_owned(),
                quantity: dec!(10),
                manual_price: symbol.is_none().then_some(dec!(1)),
                score_override: None,
                listed_on_b3: listed,
                treasury: false,
                max_price: None,
            },
        )
        .await
        .unwrap();
}

/// Uma carteira fictícia: uma ação coberta, um ETF em Ações Nacionais, um FII,
/// uma ação dos EUA, um ETF de cripto na B3 e um CDB.
async fn portfolio(store: &Store, email: &str) -> Uuid {
    let user = store.ensure_user(email).await.unwrap();
    asset(store, user, AssetClass::BrStocks, Some("UMUM3"), true).await;
    asset(store, user, AssetClass::BrStocks, Some("ETFX11"), true).await;
    asset(store, user, AssetClass::BrReits, Some("FICT11"), true).await;
    asset(store, user, AssetClass::IntlStocks, Some("AAPL"), false).await;
    asset(store, user, AssetClass::Crypto, Some("HASH11"), true).await;
    asset(store, user, AssetClass::FixedIncome, None, false).await;
    store
        .upsert_quote(
            &Quote {
                symbol: "FICT11.SA".into(),
                price: dec!(90),
                currency: "BRL".into(),
                name: None,
                traded_at: None,
            },
            NOW,
        )
        .await
        .unwrap();
    user
}

fn company(
    map: &HashMap<String, AssetFundamentals>,
    symbol: &str,
) -> prumo::domain::fundamentals::CompanyMetrics {
    match map.get(symbol) {
        Some(AssetFundamentals::Company(m)) => (**m).clone(),
        other => panic!("{symbol}: {other:?}"),
    }
}

// -------------------------------------------------------------- testes --

#[tokio::test]
async fn a_round_reads_the_portfolio_companies_and_funds() {
    let db = database().await;
    let cvm = FakeCvm::start().await;
    cvm.standard(true).await;
    let user = portfolio(&db.store, "fundamentos@teste.local").await;
    let codes = FakeCodes::with(&[("UMUM", Some(99901)), ("ETFX", None)]);

    // Antes da primeira rodada: tudo pendente, nada inventado.
    let before = db.store.fundamentals_for_user(user).await.unwrap();
    assert_eq!(
        before.get("UMUM3"),
        Some(&AssetFundamentals::Unsupported(Unsupported::Pending))
    );
    assert_eq!(
        before.get("FICT11"),
        Some(&AssetFundamentals::Unsupported(Unsupported::Pending))
    );

    let report = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW)
        .await;
    assert_eq!(report.codes_fetched, 2);
    assert_eq!(report.failed, 0, "{report:?}");
    // Cadastro + 5 DFP + 4 informes lidos; DFP 2026 e os dois ITR não existem.
    assert_eq!(report.updated, 10, "{report:?}");
    assert_eq!(report.unchanged, 3, "{report:?}");

    let map = db.store.fundamentals_for_user(user).await.unwrap();
    assert_eq!(map.len(), 5, "o CDB sem símbolo não entra");
    let m = company(&map, "UMUM3");
    assert_eq!(m.cvm_code, 99901);
    assert_eq!(m.name, "CIA FICTICIA UM S.A.");
    assert_eq!(m.template, Template::Commercial);
    assert_eq!(m.judicial_recovery, Some(false));
    assert_eq!(
        m.years.iter().map(|y| y.year).collect::<Vec<_>>(),
        vec![2021, 2022, 2023, 2024, 2025]
    );
    let last = m.years[4].figures.as_ref().unwrap();
    assert_eq!(last.revenue.value(), Some(dec!(1464000)));
    assert_eq!(last.net_income.value(), Some(dec!(146000)));
    assert_eq!(last.net_debt.value(), Some(dec!(700000)));
    assert_eq!(last.depreciation_amortization.value(), Some(dec!(100000)));
    assert_eq!(last.ebitda.value(), Some(dec!(319000)));
    assert_eq!(last.roe.value(), Some(dec!(0.0292)));
    // LPA e VPA com as mil ações da B3: 146.000 ÷ 1.000 e 5.000.000 ÷ 1.000.
    assert_eq!(m.shares, Some(dec!(1000)));
    assert_eq!(m.earnings_per_share.value(), Some(dec!(146)));
    assert_eq!(m.book_value_per_share.value(), Some(dec!(5000)));
    assert_eq!(m.profitable_years.positive, 5);
    assert!(m.revenue_cagr.value().unwrap() > dec!(0.09));
    assert!(m.synced_at.is_some());
    assert_eq!(
        m.latest_filing.unwrap().reference_date,
        date!(2025 - 12 - 31)
    );
    // Sem ITR: os 12 meses são o exercício.
    assert_eq!(m.trailing.unwrap().period_end, date!(2025 - 12 - 31));

    assert_eq!(
        map.get("ETFX11"),
        Some(&AssetFundamentals::Unsupported(Unsupported::NoCvmCode))
    );
    assert_eq!(
        map.get("AAPL"),
        Some(&AssetFundamentals::Unsupported(Unsupported::NotListedOnB3))
    );
    assert_eq!(
        map.get("HASH11"),
        Some(&AssetFundamentals::Unsupported(
            Unsupported::ExchangeTradedProduct
        ))
    );
    let Some(AssetFundamentals::Fund(fund)) = map.get("FICT11") else {
        panic!("{:?}", map.get("FICT11"));
    };
    assert_eq!(fund.nav_per_share.value(), Some(dec!(100)));
    assert_eq!(fund.price, Some(dec!(90)));
    assert_eq!(fund.price_to_book.value(), Some(dec!(0.9)));
    assert_eq!(fund.vacancy.value(), Some(dec!(0.15)));
    assert_eq!(fund.vacancy_reference_date, Some(date!(2026 - 09 - 30)));
    assert_eq!(fund.reference_date, date!(2026 - 10 - 01));

    // Só a companhia da carteira foi gravada (a 99902 está no arquivo, mas ninguém tem).
    let pool = sqlx::PgPool::connect(&db.url).await.unwrap();
    let (stored,): (i64,) = sqlx::query_as("select count(distinct cvm_code) from cvm_filings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);

    // Uma hora depois: nada venceu, nada vai à rede.
    let again = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(1))
        .await;
    assert_eq!(
        (
            again.updated,
            again.unchanged,
            again.failed,
            again.codes_fetched
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(cvm.hits(DFP_2025).await, (1, 0));

    // No dia seguinte: GET condicional, a CVM diz 304, nada é baixado.
    let next_day = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(21))
        .await;
    assert_eq!(next_day.failed, 0, "{next_day:?}");
    assert_eq!(next_day.updated, 0, "{next_day:?}");
    assert_eq!(cvm.hits(DFP_2025).await, (1, 1));
    // O emissor achado é conferido de novo uma vez por dia (o número de ações
    // muda); o ETF sem código, só depois de uma semana.
    assert_eq!(codes.calls.load(Ordering::SeqCst), 3);

    // A rota mostra a mesma coisa, na ordem da carteira.
    let app = app(
        &db.store,
        "fundamentos@teste.local",
        FakeQuotes::with_market(),
        "static",
    );
    let reply = call(&app.router, Method::GET, "/api/v1/fundamentals", None).await;
    assert_eq!(reply.status, StatusCode::OK);
    let assets = reply.body["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 5);
    assert_eq!(assets[0]["symbol"], "UMUM3");
    assert_eq!(assets[0]["kind"], "company");
    let c = &assets[0]["company"];
    assert_eq!(c["template"], "commercial");
    assert_eq!(c["latest_filing"]["reference_date"], "2025-12-31");
    assert_eq!(c["latest_filing"]["kind"], "annual");
    assert_eq!(c["latest_filing"]["scope"], "consolidated");
    assert_eq!(c["years"][4]["figures"]["revenue"]["value"], 1464000.0);
    assert_eq!(
        c["years"][4]["figures"]["revenue"]["gap"],
        serde_json::Value::Null
    );
    assert_eq!(c["years"][4]["figures"]["period_end"], "2025-12-31");
    assert_eq!(c["profitable_years"]["positive"], 5);
    assert_eq!(c["shares"], 1000.0);
    assert_eq!(c["earnings_per_share"]["value"], 146.0);
    assert_eq!(c["book_value_per_share"]["value"], 5000.0);
    assert_eq!(
        c["years"][4]["figures"]["equity_controlling"]["value"],
        5000000.0
    );
    assert_eq!(assets[1]["kind"], "unsupported");
    assert_eq!(assets[1]["reason"], "no_cvm_code");
    assert_eq!(assets[2]["kind"], "fund");
    assert_eq!(assets[2]["fund"]["price_to_book"]["value"], 0.9);
    assert_eq!(assets[2]["fund"]["vacancy_reference_date"], "2026-09-30");
    assert_eq!(assets[3]["reason"], "not_listed_on_b3");
    assert_eq!(assets[4]["reason"], "exchange_traded_product");
}

#[tokio::test]
async fn a_new_company_reads_the_files_again_for_it_alone() {
    let db = database().await;
    let cvm = FakeCvm::start().await;
    cvm.standard(true).await;
    let user = portfolio(&db.store, "nova@teste.local").await;
    let codes = FakeCodes::with(&[("UMUM", Some(99901)), ("ETFX", None), ("DOIS", Some(99902))]);
    service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW)
        .await;
    assert_eq!(cvm.hits(DFP_2025).await, (1, 0));

    // Outra pessoa compra a companhia 99902 logo depois.
    let other = db.store.ensure_user("outra@teste.local").await.unwrap();
    asset(&db.store, other, AssetClass::BrStocks, Some("DOIS3"), true).await;
    let report = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::minutes(5))
        .await;
    assert_eq!(report.failed, 0, "{report:?}");
    // As cinco DFP lidas de novo (sem condicional: a versão tem gente nova).
    assert_eq!(report.updated, 5, "{report:?}");
    assert_eq!(cvm.hits(DFP_2025).await, (2, 0));

    let map = db.store.fundamentals_for_user(other).await.unwrap();
    let m = company(&map, "DOIS3");
    assert_eq!(m.judicial_recovery, Some(true));
    let y = m.years[4].figures.as_ref().unwrap();
    assert_eq!(y.net_income.value(), Some(dec!(-80000)));
    assert_eq!(y.roe.gap(), Some(Gap::NonPositiveBase));
    assert_eq!(m.earnings_cagr.gap(), Some(Gap::NonPositiveEndpoint));
    // A primeira companhia continua inteira.
    let first = db.store.fundamentals_for_user(user).await.unwrap();
    assert_eq!(company(&first, "UMUM3").profitable_years.known, 5);

    // Tudo lido: a rodada seguinte não baixa nada.
    let quiet = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::minutes(10))
        .await;
    assert_eq!(quiet.updated, 0);
}

#[tokio::test]
async fn a_source_out_of_service_keeps_the_last_good_data() {
    let db = database().await;
    let cvm = FakeCvm::start().await;
    cvm.standard(false).await;
    let user = portfolio(&db.store, "fora@teste.local").await;
    let codes = FakeCodes::with(&[("UMUM", Some(99901)), ("ETFX", None)]);
    service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW)
        .await;

    // A CVM cai: tudo responde 503.
    cvm.server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&cvm.server)
        .await;
    let report = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(21))
        .await;
    assert!(report.failed >= 5, "{report:?}");
    let map = db.store.fundamentals_for_user(user).await.unwrap();
    assert_eq!(company(&map, "UMUM3").profitable_years.known, 5);
    assert!(matches!(
        map.get("FICT11"),
        Some(AssetFundamentals::Fund(_))
    ));

    // A falha espera 6 h antes de tentar de novo.
    let soon = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(22))
        .await;
    assert_eq!((soon.updated, soon.failed), (0, 0));

    // Arquivo que não é zip também não estraga o gravado.
    cvm.server.reset().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(b"<html>manutencao</html>".to_vec()),
        )
        .mount(&cvm.server)
        .await;
    let broken = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(28))
        .await;
    assert!(broken.failed >= 5, "{broken:?}");
    assert_eq!(
        company(
            &db.store.fundamentals_for_user(user).await.unwrap(),
            "UMUM3"
        )
        .profitable_years
        .known,
        5
    );

    // A B3 fora do ar: o emissor fica pendente, sem quebrar o resto.
    let newcomer = db.store.ensure_user("b3fora@teste.local").await.unwrap();
    asset(
        &db.store,
        newcomer,
        AssetClass::BrStocks,
        Some("SEMB3"),
        true,
    )
    .await;
    let b3_down = service(&db.store, &cvm, Arc::clone(&codes))
        .round(NOW + time::Duration::hours(29))
        .await;
    assert_eq!(b3_down.codes_failed, 1);
    let map = db.store.fundamentals_for_user(newcomer).await.unwrap();
    assert_eq!(
        map.get("SEMB3"),
        Some(&AssetFundamentals::Unsupported(Unsupported::Pending))
    );
}

#[tokio::test]
async fn two_replicas_split_the_work() {
    let db = database().await;
    let cvm = FakeCvm::start().await;
    cvm.standard(false).await;
    portfolio(&db.store, "replicas@teste.local").await;
    let codes = FakeCodes::with(&[("UMUM", Some(99901)), ("ETFX", None)]);
    let second = Store::connect(&db.url).await.unwrap();
    let a = service(&db.store, &cvm, Arc::clone(&codes));
    let b = service(&second, &cvm, Arc::clone(&codes));
    let (ra, rb) = tokio::join!(a.round(NOW), b.round(NOW));
    // Cada arquivo baixado uma vez, cada emissor buscado uma vez.
    assert_eq!(ra.updated + rb.updated, 10, "{ra:?} {rb:?}");
    assert_eq!(ra.failed + rb.failed, 0);
    assert_eq!(codes.calls.load(Ordering::SeqCst), 2);
    for year in 2021..=2025 {
        let route = format!("/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_{year}.zip");
        assert_eq!(cvm.hits(&route).await, (1, 0), "{route}");
    }
}

fn filing(version: i32, revenue: Decimal) -> Filing {
    Filing {
        cvm_code: 99909,
        kind: FilingKind::Annual,
        reference_date: date!(2025 - 12 - 31),
        version,
        scope: Scope::Consolidated,
        company_name: "CIA FICTICIA SA".into(),
        lines: vec![
            Line {
                statement: Statement::Income,
                column: Column::Current,
                period_start: Some(date!(2025 - 01 - 01)),
                period_end: date!(2025 - 12 - 31),
                code: "3.01".into(),
                description: "Receita de Venda de Bens e/ou Serviços".into(),
                value: revenue,
            },
            Line {
                statement: Statement::Income,
                column: Column::Current,
                period_start: Some(date!(2025 - 01 - 01)),
                period_end: date!(2025 - 12 - 31),
                code: "3.11".into(),
                description: "Lucro/Prejuízo Consolidado do Período".into(),
                value: dec!(10),
            },
        ],
    }
}

#[tokio::test]
async fn a_restatement_wins_and_an_older_version_never_undoes_it() {
    let db = database().await;
    let user = db
        .store
        .ensure_user("reapresenta@teste.local")
        .await
        .unwrap();
    asset(&db.store, user, AssetClass::BrStocks, Some("NOVE3"), true).await;
    db.store
        .save_issuer_code(
            "NOVE",
            &IssuerInfo {
                cvm_code: Some(99909),
                total_shares: None,
            },
            NOW,
        )
        .await
        .unwrap();
    let scanned: BTreeSet<i32> = [99909].into_iter().collect();
    let v = Validators::default();
    let revenue = |store: &Store| {
        let store = store.clone();
        async move {
            let map = store.fundamentals_for_user(user).await.unwrap();
            company(&map, "NOVE3").years[4]
                .figures
                .as_ref()
                .unwrap()
                .revenue
                .value()
        }
    };
    assert_eq!(
        db.store
            .save_company_filings("annual:2025", &[filing(1, dec!(100))], &v, &scanned, NOW)
            .await
            .unwrap(),
        1
    );
    assert_eq!(revenue(&db.store).await, Some(dec!(100)));
    // Reapresentação (versão 2) troca.
    db.store
        .save_company_filings("annual:2025", &[filing(2, dec!(120))], &v, &scanned, NOW)
        .await
        .unwrap();
    assert_eq!(revenue(&db.store).await, Some(dec!(120)));
    // Um arquivo velho com a versão 1 não desfaz.
    assert_eq!(
        db.store
            .save_company_filings("annual:2025", &[filing(1, dec!(100))], &v, &scanned, NOW)
            .await
            .unwrap(),
        0
    );
    assert_eq!(revenue(&db.store).await, Some(dec!(120)));
    // Sem documento depois de o arquivo ter lido o código: "sem DFP", não pendente.
    let other = db.store.ensure_user("semdfp@teste.local").await.unwrap();
    asset(&db.store, other, AssetClass::BrStocks, Some("ZERO3"), true).await;
    db.store
        .save_issuer_code(
            "ZERO",
            &IssuerInfo {
                cvm_code: Some(99908),
                total_shares: None,
            },
            NOW,
        )
        .await
        .unwrap();
    let pending = db.store.fundamentals_for_user(other).await.unwrap();
    assert_eq!(
        pending.get("ZERO3"),
        Some(&AssetFundamentals::Unsupported(Unsupported::Pending))
    );
    let both: BTreeSet<i32> = [99908, 99909].into_iter().collect();
    db.store
        .save_company_filings("annual:2025", &[], &v, &both, NOW)
        .await
        .unwrap();
    let none = db.store.fundamentals_for_user(other).await.unwrap();
    assert_eq!(
        none.get("ZERO3"),
        Some(&AssetFundamentals::Unsupported(Unsupported::NoFilings))
    );
}

#[tokio::test]
async fn the_route_needs_a_portfolio_and_shows_only_the_owner() {
    let db = database().await;
    let app = app(
        &db.store,
        "vazia@teste.local",
        FakeQuotes::with_market(),
        "static",
    );
    let reply = call(&app.router, Method::GET, "/api/v1/fundamentals", None).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["assets"], serde_json::json!([]));
    // A carteira de outra pessoa não aparece.
    portfolio(&db.store, "dona@teste.local").await;
    let reply = call(&app.router, Method::GET, "/api/v1/fundamentals", None).await;
    assert_eq!(reply.body["assets"], serde_json::json!([]));
}

#[tokio::test]
async fn a_disabled_service_fetches_nothing() {
    let db = database().await;
    portfolio(&db.store, "desligado@teste.local").await;
    let service = FundamentalsService::disabled(db.store.clone());
    assert!(!service.enabled());
    let report = service.round(NOW).await;
    assert_eq!(report, prumo::fundamentals::RoundReport::default());
}

/// Espera `check` dar verdadeiro (ou falha em 15 s).
async fn eventually<F, Fut>(what: &str, check: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !check().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "nao aconteceu: {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_new_asset_wakes_the_task_without_waiting_the_hour() {
    let db = database().await;
    let cvm = FakeCvm::start().await;
    cvm.standard(false).await;
    let codes = FakeCodes::with(&[("WEGE", Some(99901))]);
    let user = db.store.ensure_user("acorda@teste.local").await.unwrap();
    asset(&db.store, user, AssetClass::BrReits, Some("FICT11"), true).await;
    let task = Arc::new(service(&db.store, &cvm, Arc::clone(&codes)));
    let test_app = app(
        &db.store,
        "acorda@teste.local",
        FakeQuotes::with_market(),
        "static",
    );
    let router = prumo::http::public_router(
        test_app
            .state
            .clone()
            .with_fundamentals_service(Arc::clone(&task)),
        "static",
    );
    // A primeira rodada (na hora) lê os informes do FII; a próxima seria em 1 h.
    Arc::clone(&task).spawn(Duration::from_secs(3600));
    let store = db.store.clone();
    eventually("primeira rodada", || {
        let store = store.clone();
        async move {
            store
                .cvm_source_states()
                .await
                .unwrap()
                .get("fund_quarterly:2025")
                .is_some_and(|s| s.checked_at.is_some())
        }
    })
    .await;
    assert_eq!(codes.calls.load(Ordering::SeqCst), 0);

    let created = call(
        &router,
        Method::POST,
        "/api/v1/assets",
        Some(serde_json::json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 3 })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    eventually("busca do ativo novo", || {
        let store = store.clone();
        async move {
            matches!(
                store
                    .fundamentals_for_user(user)
                    .await
                    .unwrap()
                    .get("WEGE3"),
                Some(AssetFundamentals::Company(_))
            )
        }
    })
    .await;
    assert_eq!(codes.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_b3_client_reads_the_code_and_the_shares() {
    let server = MockServer::start().await;
    let query =
        prumo::income_auto::supplement_query("FICT", prumo::domain::income_auto::Venue::Company);
    Mock::given(method("GET"))
        .and(path(format!(
            "/listedCompaniesProxy/CompanyCall/GetListedSupplementCompany/{}",
            prumo::income_auto::encoded(&query)
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "code": "FICT", "codeCVM": "99901", "totalNumberShares": "1.234.567", "cashDividends": [] }
        ])))
        .mount(&server)
        .await;
    let client = B3Issuers::with_base(reqwest::Client::new(), &format!("{}/", server.uri()));
    assert_eq!(
        client.issuer("FICT").await.unwrap(),
        IssuerInfo {
            cvm_code: Some(99901),
            total_shares: Some(dec!(1234567))
        }
    );
    // Emissor fora do padrão: nem vai à B3.
    assert_eq!(client.issuer("fict").await.unwrap(), IssuerInfo::default());
    // Página que não existe: erro, para a rodada tentar de novo.
    assert!(client.issuer("ZZZZ").await.is_err());
    assert_eq!(parse_issuer(&serde_json::json!([])), IssuerInfo::default());
}
