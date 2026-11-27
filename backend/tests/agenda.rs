//! Agenda contra uma CVM e um Fundos.NET falsos (wiremock, arquivos no formato
//! real) e um Postgres de verdade: a rodada que lê só o que é da carteira, o
//! GET condicional, a fonte fora do ar, a rota com os proventos, o fora do
//! padrão, os documentos e os dias sem pregão, e o selo da Renda.
//!
//! Companhias, fundos, códigos CVM, CNPJs e valores são FICTÍCIOS.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::http::StatusCode;
use common::*;
use prumo::agenda::fundos_net::FundosNet;
use prumo::agenda::AgendaService;
use prumo::domain::fundamentals::FundReport;
use prumo::domain::income::today_in_brazil;
use prumo::fundamentals::b3::IssuerInfo;
use prumo::fundamentals::download::Validators;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COMPANY: i32 = 99_901;
const FUND_CNPJ: &str = "11.111.111/0001-11";

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

fn days(n: i64) -> Date {
    today().saturating_add(Duration::days(n))
}

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

/// O IPE do ano de hoje: um fato relevante da companhia da carteira, uma ata
/// que a Agenda não guarda e um fato de outra companhia.
fn ipe(year: i32) -> Vec<u8> {
    let header = "CNPJ_Companhia;Nome_Companhia;Codigo_CVM;Data_Referencia;Categoria;Tipo;Especie;Assunto;Data_Entrega;Tipo_Apresentacao;Protocolo_Entrega;Versao;Link_Download";
    let link = |seq: u32| {
        format!("https://www.rad.cvm.gov.br/ENET/frmDownloadDocumento.aspx?Tela=ext&descTipo=IPE&CodigoInstituicao=1&numProtocolo=8{seq}&numSequencia={seq}&numVersao=1")
    };
    let day = today();
    let rows = [
        format!("00.000.000/0001-00;CIA FICTICIA SA;{COMPANY};{day};Fato Relevante;;;Aquisição fictícia;{day};AP - Apresentação;X;1;{}", link(501)),
        format!("00.000.000/0001-00;CIA FICTICIA SA;{COMPANY};{day};Reunião da Administração;;Ata;Ata fictícia;{day};AP - Apresentação;X;1;{}", link(502)),
        format!("00.000.000/0002-00;OUTRA FICTICIA SA;99902;{day};Fato Relevante;;;De outra;{day};AP - Apresentação;X;1;{}", link(503)),
    ];
    let csv = std::iter::once(header.to_owned())
        .chain(rows)
        .collect::<Vec<_>>()
        .join("\n");
    zip(&[(format!("ipe_cia_aberta_{year}.csv"), csv)])
}

/// O ITR do ano de hoje: o índice (a entrega) e uma demonstração que a Agenda
/// não lê.
fn itr(year: i32) -> Vec<u8> {
    let index = [
        "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;CATEG_DOC;ID_DOC;DT_RECEB;LINK_DOC".to_owned(),
        format!("00.000.000/0001-00;{};1;CIA FICTICIA SA;0{COMPANY};ITR;7;{};http://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?CodigoInstituicao=1&NumeroSequencialDocumento=7", days(-40), days(-1)),
    ]
    .join("\n");
    zip(&[
        (format!("itr_cia_aberta_{year}.csv"), index),
        (
            format!("itr_cia_aberta_DRE_con_{year}.csv"),
            "x".repeat(4096),
        ),
    ])
}

/// A consulta do Fundos.NET de um fundo fictício.
fn listing() -> String {
    let day = today();
    let br = format!(
        "{:02}/{:02}/{}",
        day.day(),
        u8::from(day.month()),
        day.year()
    );
    json!({
        "draw": 1, "recordsFiltered": 2, "recordsTotal": 2,
        "data": [
            {"id": 7_000_001, "categoriaDocumento": "Fato Relevante", "tipoDocumento": "",
             "especieDocumento": "", "dataEntrega": format!("{br} 18:00"), "dataReferencia": br,
             "situacaoDocumento": "A", "versao": 1},
            {"id": 7_000_002, "categoriaDocumento": "Aviso aos Cotistas - Estruturado",
             "tipoDocumento": "Rendimentos e Amortizações", "especieDocumento": "",
             "dataEntrega": format!("{br} 18:00"), "dataReferencia": br,
             "situacaoDocumento": "A", "versao": 1}
        ]
    })
    .to_string()
}

struct Sources {
    server: MockServer,
}

impl Sources {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let year = today().year();
        Mock::given(method("GET"))
            .and(path(format!(
                "/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_{year}.zip"
            )))
            .and(header("if-none-match", "\"ipe\""))
            .respond_with(ResponseTemplate::new(304))
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_{year}.zip"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"ipe\"")
                    .set_body_bytes(ipe(year)),
            )
            .with_priority(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_{year}.zip"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"itr\"")
                    .set_body_bytes(itr(year)),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/fnet/publico/pesquisarGerenciadorDocumentosDados"))
            .and(query_param("cnpjFundo", "11111111000111"))
            .respond_with(ResponseTemplate::new(200).set_body_string(listing()))
            .mount(&server)
            .await;
        // O resto (o ano anterior, a DFP do ano) ainda não existe.
        Sources { server }
    }

    fn service(&self, store: &prumo::store::Store) -> AgendaService {
        let http = reqwest::Client::new();
        AgendaService::new(
            store.clone(),
            http.clone(),
            Arc::new(FundosNet::with_base(http, &self.server.uri())),
        )
        .with_base(&self.server.uri())
        .with_pause(StdDuration::ZERO)
    }

    async fn hits(&self, route: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path() == route)
            .count()
    }
}

struct Person {
    app: TestApp,
    user: Uuid,
    _dir: tempfile_dir::Dir,
}

async fn person(db: &Database, email: &str) -> Person {
    let dir = tempfile_dir::Dir::new();
    let quotes = FakeQuotes::with_market();
    {
        let mut prices = quotes.prices.lock().unwrap();
        prices.insert("FICT3.SA".into(), (dec!(20), "BRL", "Cia Ficticia ON"));
        prices.insert("FUNR11.SA".into(), (dec!(90), "BRL", "Fundo Ficticio FII"));
    }
    let app = app(&db.store, email, quotes, &dir.path());
    let user = db.store.ensure_user(email).await.unwrap();
    Person {
        app,
        user,
        _dir: dir,
    }
}

async fn add_asset(p: &Person, class: &str, symbol: &str, quantity: u32) -> String {
    let reply = post(
        &p.app.router,
        "/api/v1/assets",
        json!({ "asset_class": class, "symbol": symbol, "quantity": quantity }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

/// O que a tarefa dos fundamentos já deixa no banco: o código CVM da
/// companhia e o CNPJ do fundo.
async fn reference_data(db: &Database) {
    db.store
        .save_issuer_code(
            "FICT",
            &IssuerInfo {
                cvm_code: Some(COMPANY),
                total_shares: Some(dec!(1000000)),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    db.store
        .save_fund_reports(
            "fund_monthly:2026",
            &[FundReport {
                cnpj: FUND_CNPJ.to_owned(),
                issuer: "FUNR".to_owned(),
                isin: "BRFUNRCTF000".to_owned(),
                name: "FUNDO FICTICIO FII".to_owned(),
                reference_date: days(-30),
                version: 1,
                net_assets: Some(dec!(1000)),
                shares: Some(dec!(10)),
                total_assets: None,
                liquid_assets: None,
            }],
            &Validators::default(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
}

/// Doze rendimentos mensais de 0,80 do FII fictício, um anunciado de 0,60
/// (corte de 25%) e um ainda sem data de pagamento.
async fn fund_announcements(db: &Database) {
    let pool = PgPoolOptions::new().connect(&db.url).await.unwrap();
    let insert = |record: Date, rate: Decimal, pay: Option<Date>, approved: Option<Date>| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "insert into b3_cash_announcements
                    (issuer, isin, label, record_date, rate, pay_date, approved_on)
                 values ('FUNR', 'BRFUNRCTF000', 'RENDIMENTO', $1, $2, $3, $4)",
            )
            .bind(record)
            .bind(rate)
            .bind(pay)
            .bind(approved)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    for k in 1..=12 {
        let record = days(-30 * k);
        insert(record, dec!(0.80), Some(record + Duration::days(15)), None).await;
    }
    insert(days(3), dec!(0.60), Some(days(20)), Some(days(-2))).await;
    insert(days(33), dec!(0.80), None, None).await;
    // Um resíduo avulso dez dias depois de um rendimento: abaixo de todos, mas
    // fora do lugar do pagamento do mês. Não é corte.
    insert(days(-20), dec!(0.004), Some(days(-5)), None).await;
    sqlx::query("insert into b3_issuer_fetches (issuer, fetched_at) values ('FUNR', now())")
        .execute(&pool)
        .await
        .unwrap();
}

/// A companhia fictícia: JCP trimestral de 0,15 e dividendo semestral de 0,20
/// nos 12 meses, e uma data com que declara 3 × 0,4128 de dividendo, pago em
/// três parcelas anuais (um aumento extraordinário).
async fn company_announcements(db: &Database) {
    let pool = PgPoolOptions::new().connect(&db.url).await.unwrap();
    let insert =
        |label: &'static str, record: Date, rate: Decimal, pay: Date, approved: Option<Date>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "insert into b3_cash_announcements
                    (issuer, isin, label, record_date, rate, pay_date, approved_on)
                 values ('FICT', 'BRFICTACNOR0', $1, $2, $3, $4, $5)",
                )
                .bind(label)
                .bind(record)
                .bind(rate)
                .bind(pay)
                .bind(approved)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
    for back in [20, 111, 202, 293] {
        let record = days(-back);
        insert(
            "JRS CAP PROPRIO",
            record,
            dec!(0.15),
            record + Duration::days(10),
            None,
        )
        .await;
    }
    for back in [60, 240] {
        let record = days(-back);
        insert(
            "DIVIDENDO",
            record,
            dec!(0.20),
            record + Duration::days(10),
            None,
        )
        .await;
    }
    for pay in [days(30), days(395), days(760)] {
        insert("DIVIDENDO", days(5), dec!(0.4128), pay, Some(days(-1))).await;
    }
    sqlx::query("insert into b3_issuer_fetches (issuer, fetched_at) values ('FICT', now())")
        .execute(&pool)
        .await
        .unwrap();
}

fn of_type<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == kind).collect()
}

fn symbols(event: &Value) -> Vec<String> {
    event["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["symbol"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn the_agenda_reads_only_the_portfolio_and_shows_it_by_day() {
    let db = database().await;
    let sources = Sources::start().await;
    reference_data(&db).await;
    let ana = person(&db, "agenda-ana@prumo.test").await;
    add_asset(&ana, "br_stocks", "FICT3", 50).await;
    let fund = add_asset(&ana, "br_reits", "FUNR11", 100).await;
    let bia = person(&db, "agenda-bia@prumo.test").await;
    add_asset(&bia, "br_reits", "MXRF11", 10).await;

    // A rodada: o IPE e o ITR do ano, o fundo no Fundos.NET; o ano anterior e
    // a DFP do ano ainda não existem (404).
    let service = sources.service(&db.store);
    let now = OffsetDateTime::now_utc();
    let report = service.round(now).await;
    assert_eq!(report.updated, 2, "{report:?}");
    assert_eq!(report.unchanged, 4, "{report:?}");
    assert_eq!(report.failed, 0, "{report:?}");
    assert_eq!(report.funds_fetched, 1, "{report:?}");
    // Logo depois, nada venceu: nenhum pedido novo.
    let again = service.round(now + Duration::minutes(5)).await;
    assert_eq!(again, prumo::agenda::RoundReport::default());
    let year = today().year();
    let ipe_route = format!("/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_{year}.zip");
    assert_eq!(sources.hits(&ipe_route).await, 1);
    // No dia seguinte, o IPE não mudou (304) e o gravado fica.
    let later = service.round(now + Duration::hours(21)).await;
    assert_eq!(later.failed, 0, "{later:?}");
    assert_eq!(sources.hits(&ipe_route).await, 2);

    fund_announcements(&db).await;
    company_announcements(&db).await;
    db.store.regenerate_income_auto(ana.user).await.unwrap();

    let from = days(-60);
    let to = days(60);
    let reply = get(
        &ana.app.router,
        &format!("/api/v1/agenda?from={from}&to={to}"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    assert_eq!(reply.body["today"], today().to_string());
    assert!(reply.body["calendar_until"].as_i64().unwrap() >= 2027);
    assert!(reply.body["updated"]["cvm_documents"].is_string());
    assert!(reply.body["updated"]["fund_documents"].is_string());
    let events = reply.body["events"].as_array().unwrap().clone();

    // O fato relevante da companhia (a ata e a outra companhia ficam de fora)
    // e o do fundo; o aviso estruturado de rendimento do fundo também fica.
    let facts = of_type(&events, "material_fact");
    assert_eq!(facts.len(), 2, "{facts:?}");
    let company_fact = facts
        .iter()
        .find(|e| symbols(e) == ["FICT3"])
        .expect("fato da companhia");
    assert_eq!(company_fact["document"]["source"], "cvm_ipe");
    assert_eq!(company_fact["document"]["subject"], "Aquisição fictícia");
    assert!(company_fact["document"]["url"]
        .as_str()
        .unwrap()
        .starts_with("https://www.rad.cvm.gov.br/"));
    let fund_fact = facts
        .iter()
        .find(|e| symbols(e) == ["FUNR11"])
        .expect("fato do fundo");
    assert_eq!(fund_fact["document"]["source"], "fundos_net");
    assert!(fund_fact["document"]["url"]
        .as_str()
        .unwrap()
        .starts_with("https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=7000001"));
    assert!(of_type(&events, "notice").is_empty());

    // O ITR entregue ontem.
    let results = of_type(&events, "results");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["date"], days(-1).to_string());
    assert_eq!(results[0]["document"]["filing"], "quarterly");
    assert_eq!(
        results[0]["document"]["reference_date"],
        days(-40).to_string()
    );
    assert!(results[0]["document"]["url"]
        .as_str()
        .unwrap()
        .starts_with("https://"));

    // O rendimento de 0,60 é corte (−25%) contra os 0,80 dos 12 meses: um
    // evento no dia do anúncio e a marca na data com e no pagamento.
    let unusual = of_type(&events, "unusual_income");
    assert_eq!(unusual.len(), 2, "{unusual:?}");
    let fund_cut = unusual
        .iter()
        .find(|e| symbols(e) == ["FUNR11"])
        .expect("corte do fundo");
    assert_eq!(fund_cut["date"], days(-2).to_string());
    assert_eq!(fund_cut["unusual"]["direction"], "cut");
    assert_eq!(fund_cut["unusual"]["basis"], "similar");
    assert_eq!(fund_cut["unusual"]["kind"], "reit_income");
    assert_eq!(fund_cut["unusual"]["recent"]["change"], -0.25);
    assert_eq!(fund_cut["unusual"]["recent"]["usual"], 0.8);
    assert!(fund_cut["unusual"]["long"].is_null());
    // O resíduo avulso não vira corte.
    assert!(unusual
        .iter()
        .all(|e| e["unusual"]["record_date"] != days(-20).to_string()));
    // O dividendo extraordinário em três parcelas: um evento, um aumento contra o
    // maior pagamento dos 12 meses.
    let raise = unusual
        .iter()
        .find(|e| symbols(e) == ["FICT3"])
        .expect("aumento da companhia");
    assert_eq!(raise["date"], days(-1).to_string());
    assert_eq!(raise["unusual"]["direction"], "increase");
    assert_eq!(raise["unusual"]["basis"], "largest");
    assert!(raise["unusual"]["kind"].is_null());
    assert_eq!(raise["unusual"]["installments"], 3);
    assert_eq!(raise["unusual"]["per_unit"], 1.2384);
    assert_eq!(raise["unusual"]["recent"]["usual"], 0.2);
    assert_eq!(raise["unusual"]["pay_dates"].as_array().unwrap().len(), 3);
    let payments = of_type(&events, "income_payment");
    let cut_payment = payments
        .iter()
        .find(|e| e["date"] == days(20).to_string())
        .expect("pagamento do corte");
    assert_eq!(cut_payment["income"]["unusual"]["direction"], "cut");
    assert_eq!(cut_payment["income"]["net"], 60.0);
    assert_eq!(cut_payment["income"]["state"], "upcoming");
    let records = of_type(&events, "income_record");
    assert!(records
        .iter()
        .any(|e| e["date"] == days(3).to_string() && e["income"]["unusual"]["direction"] == "cut"));
    // O anúncio sem data de pagamento: só a data com, estimada pelas 100 cotas.
    let undated = records
        .iter()
        .find(|e| e["income"]["state"] == "undated")
        .expect("a definir");
    assert_eq!(undated["date"], days(33).to_string());
    assert_eq!(undated["income"]["net"], 80.0);
    assert!(undated["income"]["pay_date"].is_null());
    // Um mês normal do fundo não é marcado; a parcela do dividendo extraordinário é.
    assert!(payments
        .iter()
        .filter(|e| symbols(e) == ["FUNR11"] && e["date"] != days(20).to_string())
        .all(|e| e["income"]["unusual"].is_null()));
    assert!(payments
        .iter()
        .filter(|e| symbols(e) == ["FICT3"] && e["date"] == days(30).to_string())
        .all(|e| e["income"]["unusual"]["direction"] == "increase"));

    // Os dias sem pregão do período estão lá, sem ativo.
    for closed in of_type(&events, "market_closed") {
        assert!(closed["assets"].as_array().unwrap().is_empty());
        assert!(closed["closure"].is_string());
    }

    // O selo da Renda: o provento a receber fora do padrão.
    let badge = get(&ana.app.router, "/api/v1/agenda/unusual-income").await;
    assert_eq!(badge.status, StatusCode::OK, "{}", badge.text);
    let items = badge.body["items"].as_array().unwrap();
    // Uma linha por data com: as três parcelas do dividendo são um item só.
    assert_eq!(items.len(), 2, "{items:?}");
    let cut = items.iter().find(|i| i["asset_id"] == fund).expect("corte");
    assert_eq!(cut["direction"], "cut");
    assert_eq!(cut["per_unit"], 0.6);
    assert_eq!(cut["installments"], 1);
    assert_eq!(cut["income_ids"].as_array().unwrap().len(), 1);
    let raise = items
        .iter()
        .find(|i| i["asset_id"] != fund)
        .expect("aumento");
    assert_eq!(raise["direction"], "increase");
    assert_eq!(raise["basis"], "largest");
    assert_eq!(raise["installments"], 3);
    assert_eq!(raise["income_ids"].as_array().unwrap().len(), 3);
    let upcoming = get(&ana.app.router, "/api/v1/income?status=upcoming").await;
    let ids: Vec<&Value> = upcoming
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["id"])
        .collect();
    for item in items {
        for id in item["income_ids"].as_array().unwrap() {
            assert!(ids.contains(&id), "{id}");
        }
    }

    // Quem não tem os ativos não vê os documentos deles.
    let other = get(
        &bia.app.router,
        &format!("/api/v1/agenda?from={from}&to={to}"),
    )
    .await;
    assert_eq!(other.status, StatusCode::OK, "{}", other.text);
    let theirs = other.body["events"].as_array().unwrap();
    assert!(
        theirs.iter().all(|e| e["type"] == "market_closed"),
        "{theirs:?}"
    );
    let none = get(&bia.app.router, "/api/v1/agenda/unusual-income").await;
    assert!(none.body["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_range_is_validated_and_a_source_out_of_service_keeps_the_last_good_data() {
    let db = database().await;
    reference_data(&db).await;
    let ana = person(&db, "agenda-limites@prumo.test").await;
    add_asset(&ana, "br_stocks", "FICT3", 10).await;

    let inverted = get(
        &ana.app.router,
        &format!("/api/v1/agenda?from={}&to={}", days(5), days(1)),
    )
    .await;
    assert_eq!(inverted.status, StatusCode::UNPROCESSABLE_ENTITY);
    let long = get(
        &ana.app.router,
        &format!("/api/v1/agenda?from={}&to={}", days(-300), days(300)),
    )
    .await;
    assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);
    let unknown = get(&ana.app.router, "/api/v1/agenda?ate=2026-01-01").await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
    let default = get(&ana.app.router, "/api/v1/agenda").await;
    assert_eq!(default.status, StatusCode::OK, "{}", default.text);
    assert_eq!(default.body["from"], days(-30).to_string());
    assert_eq!(default.body["to"], days(90).to_string());

    // A CVM fora do ar: a falha fica registrada e nada quebra.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let service = AgendaService::new(
        db.store.clone(),
        http.clone(),
        Arc::new(FundosNet::with_base(http, &server.uri())),
    )
    .with_base(&server.uri())
    .with_pause(StdDuration::ZERO);
    let report = service.round(OffsetDateTime::now_utc()).await;
    assert_eq!(report.failed, 6, "{report:?}");
    assert_eq!(report.updated, 0);
    // Seis horas sem tentar de novo.
    let soon = service
        .round(OffsetDateTime::now_utc() + Duration::hours(1))
        .await;
    assert_eq!(soon.failed, 0, "{soon:?}");
    let still = get(&ana.app.router, "/api/v1/agenda").await;
    assert_eq!(still.status, StatusCode::OK);

    // Desligada, a tarefa não busca nada.
    let off = AgendaService::disabled(db.store.clone());
    assert!(!off.enabled());
    assert_eq!(
        off.round(OffsetDateTime::now_utc()).await,
        prumo::agenda::RoundReport::default()
    );
}
