//! A importação do extrato da B3: leitura do `.xlsx` e a rota de prévia.
//!
//! As planilhas são FICTÍCIAS, montadas aqui no formato real (cabeçalhos e
//! tipos de coluna iguais aos que a Área do Investidor exporta): tickers
//! públicos, quantidades e valores inventados.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use common::*;
use http_body_util::BodyExt;
use prumo::auth::{Authenticator, Identity};
use prumo::b3_xlsx::{self, ReadError};
use prumo::domain::b3_import::{read_file, FileKind, FormatProblem};
use prumo::error::ApiError;
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
use serde_json::{json, Value};
use tower::ServiceExt;

// --- planilhas fictícias -----------------------------------------------------

#[derive(Clone)]
enum Cell {
    Text(&'static str),
    Num(f64),
    Empty,
}

use Cell::{Empty, Num, Text};

fn workbook(sheets: &[(&str, Vec<Vec<Cell>>)]) -> Vec<u8> {
    let mut book = Workbook::new();
    for (name, rows) in sheets {
        let sheet = book.add_worksheet();
        sheet.set_name(*name).unwrap();
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let (r, c) = (u32::try_from(r).unwrap(), u16::try_from(c).unwrap());
                match cell {
                    Text(text) => {
                        sheet.write_string(r, c, *text).unwrap();
                    }
                    Num(number) => {
                        sheet.write_number(r, c, *number).unwrap();
                    }
                    Empty => {}
                }
            }
        }
    }
    book.save_to_buffer().unwrap()
}

fn header(names: &[&'static str]) -> Vec<Cell> {
    names.iter().map(|n| Text(n)).collect()
}

const BROKER: &str = "CORRETORA EXEMPLO S/A";

fn trades_sheet() -> Vec<u8> {
    workbook(&[(
        "Negociação",
        vec![
            header(&[
                "Data do Negócio",
                "Tipo de Movimentação",
                "Mercado",
                "Prazo/Vencimento",
                "Instituição",
                "Código de Negociação",
                "Quantidade",
                "Preço",
                "Valor",
            ]),
            // Do mais novo ao mais velho, como a B3 manda.
            vec![
                Text("20/02/2026"),
                Text("Venda"),
                Text("Mercado à Vista"),
                Text("-"),
                Text(BROKER),
                Text("PETR4"),
                Num(50.0),
                Num(34.0),
                Num(1700.0),
            ],
            vec![
                Text("05/01/2026"),
                Text("Compra"),
                Text("Mercado Fracionário"),
                Text("-"),
                Text(BROKER),
                Text("PETR4F"),
                Num(5.0),
                Num(0.1 + 0.2),
                Num(1.5),
            ],
            vec![
                Text("05/01/2026"),
                Text("Compra"),
                Text("Mercado à Vista"),
                Text("-"),
                Text(BROKER),
                Text("PETR4"),
                Num(100.0),
                Num(30.0),
                Num(3000.0),
            ],
            vec![
                Text("12/01/2026"),
                Text("Compra"),
                Text("Mercado à Vista"),
                Text("-"),
                Text(BROKER),
                Text("HGLG11"),
                Num(10.0),
                Num(160.0),
                Num(1600.0),
            ],
            vec![
                Text("05/03/2026"),
                Text("Compra"),
                Text("Opção de Compra sobre Ações"),
                Text("20/03/2026"),
                Text(BROKER),
                Text("BOVAC130"),
                Num(100.0),
                Num(1.2),
                Num(120.0),
            ],
        ],
    )])
}

fn movements_sheet() -> Vec<u8> {
    workbook(&[(
        "Movimentação",
        vec![
            header(&[
                "Entrada/Saída",
                "Data",
                "Movimentação",
                "Produto",
                "Instituição",
                "Quantidade",
                "Preço unitário",
                "Valor da Operação",
            ]),
            vec![
                Text("Credito"),
                Text("20/05/2026"),
                Text("Dividendo"),
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text(BROKER),
                Num(150.0),
                Num(1.0),
                Num(150.0),
            ],
            vec![
                Text("Credito"),
                Text("18/12/2025"),
                Text("Juros Sobre Capital Próprio"),
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text(BROKER),
                Num(150.0),
                Num(0.5),
                Num(63.75),
            ],
            vec![
                Text("Credito"),
                Text("13/03/2026"),
                Text("Rendimento"),
                Text("HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII"),
                Text(BROKER),
                Num(10.0),
                Num(1.1),
                Num(11.0),
            ],
            vec![
                Text("Credito"),
                Text("05/01/2026"),
                Text("Transferência - Liquidação"),
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text(BROKER),
                Num(100.0),
                Num(30.0),
                Num(3000.0),
            ],
            vec![
                Text("Credito"),
                Text("20/03/2026"),
                Text("Bonificação em Ativos"),
                Text("ITUB4 - ITAU UNIBANCO HOLDING S.A."),
                Text(BROKER),
                Num(10.0),
                Text("-"),
                Text("-"),
            ],
            vec![
                Text("Debito"),
                Text("02/04/2026"),
                Text("COMPRA / VENDA"),
                Text("Tesouro Selic 2029"),
                Text(BROKER),
                Num(1.0),
                Num(15000.0),
                Num(15000.0),
            ],
            vec![
                Text("Credito"),
                Text("02/06/2026"),
                Text("Movimento Novo da B3"),
                Text("ITUB4 - ITAU UNIBANCO HOLDING S.A."),
                Text(BROKER),
                Num(1.0),
                Text("-"),
                Text("-"),
            ],
        ],
    )])
}

fn received_sheet() -> Vec<u8> {
    workbook(&[(
        "Proventos Recebidos",
        vec![
            header(&[
                "Produto",
                "Pagamento",
                "Tipo de Evento",
                "Instituição",
                "Quantidade",
                "Preço unitário",
                "Valor líquido",
            ]),
            // O mesmo dividendo da Movimentação, com os valores em texto pt-BR.
            vec![
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text("20/05/2026"),
                Text("Dividendo"),
                Text(BROKER),
                Text("150"),
                Text("1,00"),
                Text("150,00"),
            ],
            vec![
                Text("VALE3 - VALE S.A."),
                Text("10/09/2025"),
                Text("Juros Sobre Capital Próprio"),
                Text(BROKER),
                Num(80.0),
                Num(1.0),
                Text("R$ 1.068,00"),
            ],
            vec![Empty, Empty, Empty, Empty, Empty, Empty, Text("Total")],
        ],
    )])
}

fn provisioned_sheet() -> Vec<u8> {
    workbook(&[(
        "Proventos a Receber",
        vec![
            header(&[
                "Produto",
                "Tipo",
                "Tipo de Evento",
                "Previsão de pagamento",
                "Instituição",
                "Conta",
                "Quantidade",
                "Preço unitário",
                "Valor líquido",
            ]),
            vec![
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text("PN"),
                Text("DIVIDENDO"),
                Text("20/11/2026"),
                Text(BROKER),
                Num(1_000_001.0),
                Text("150"),
                Num(1.1),
                Num(165.0),
            ],
            vec![
                Text("PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS"),
                Text("PN"),
                Text("JUROS SOBRE CAPITAL PRÓPRIO"),
                Text("-"),
                Text(BROKER),
                Num(1_000_001.0),
                Text("150"),
                Num(0.4),
                Num(51.0),
            ],
            vec![
                Empty,
                Empty,
                Empty,
                Empty,
                Empty,
                Empty,
                Empty,
                Text("Total líquido"),
                Num(216.0),
            ],
        ],
    )])
}

fn stranger_sheet() -> Vec<u8> {
    workbook(&[(
        "Plan1",
        vec![header(&["Nome", "Idade"]), vec![Text("Fulana"), Num(30.0)]],
    )])
}

// --- leitura do arquivo ---------------------------------------------------------

fn read(bytes: &[u8]) -> prumo::domain::b3_import::FileReading {
    read_file(&b3_xlsx::read_workbook(bytes).unwrap()).unwrap()
}

#[test]
fn reads_each_type_of_sheet() {
    let trades = read(&trades_sheet());
    assert_eq!(trades.kinds, vec![FileKind::Trades]);
    assert_eq!(trades.trades.len(), 4);
    assert_eq!(trades.unrecognized.len(), 1);
    assert_eq!(trades.unrecognized[0].reason_code, "unsupported_market");
    assert_eq!(trades.unrecognized[0].line, 6);
    let fractional = trades
        .trades
        .iter()
        .find(|t| t.quantity == 5.into())
        .unwrap();
    // O ponto flutuante do Excel (0.30000000000000004) volta a 0,30.
    assert_eq!(fractional.unit_price.to_string(), "0.3");
    assert_eq!(fractional.code, "PETR4");

    let movements = read(&movements_sheet());
    assert_eq!(movements.kinds, vec![FileKind::Movements]);
    assert_eq!(movements.income.len(), 3);
    assert_eq!(movements.events.len(), 2);
    assert_eq!(movements.ignored.len(), 1);
    assert_eq!(movements.unrecognized.len(), 1);
    assert!(movements.events.iter().any(|e| e.treasury));

    let received = read(&received_sheet());
    assert_eq!(received.kinds, vec![FileKind::IncomeReceived]);
    assert_eq!(received.income.len(), 2);
    assert_eq!(received.income[1].net_amount.to_string(), "1068.00");

    let provisioned = read(&provisioned_sheet());
    assert_eq!(provisioned.kinds, vec![FileKind::IncomeProvisioned]);
    assert_eq!(provisioned.income.len(), 2);
    assert!(provisioned.income[1].date.is_none());
}

#[test]
fn the_excel_date_type_is_read_as_a_date() {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    let format = Format::new().set_num_format("dd/mm/yyyy");
    for (c, name) in [
        "Entrada/Saída",
        "Data",
        "Movimentação",
        "Produto",
        "Instituição",
        "Quantidade",
        "Preço unitário",
        "Valor da Operação",
    ]
    .iter()
    .enumerate()
    {
        sheet
            .write_string(0, u16::try_from(c).unwrap(), *name)
            .unwrap();
    }
    sheet.write_string(1, 0, "Credito").unwrap();
    sheet
        .write_datetime_with_format(1, 1, ExcelDateTime::from_ymd(2026, 3, 13).unwrap(), &format)
        .unwrap();
    sheet.write_string(1, 2, "Dividendo").unwrap();
    sheet.write_string(1, 3, "VALE3 - VALE S.A.").unwrap();
    sheet.write_number(1, 5, 80.0).unwrap();
    sheet.write_number(1, 7, 144.0).unwrap();
    let reading = read(&book.save_to_buffer().unwrap());
    assert_eq!(reading.income.len(), 1);
    assert_eq!(reading.income[0].date.unwrap().to_string(), "2026-03-13");
}

#[test]
fn an_unknown_workbook_says_what_it_expected() {
    let sheets = b3_xlsx::read_workbook(&stranger_sheet()).unwrap();
    let problem = read_file(&sheets).unwrap_err();
    assert_eq!(problem, FormatProblem::Unknown);
    assert!(problem.message().contains("Negociacao"));
}

#[test]
fn a_known_workbook_with_a_changed_layout_lists_the_missing_columns() {
    let book = workbook(&[(
        "Negociação",
        vec![header(&[
            "Data do Negócio",
            "Código de Negociação",
            "Quantidade",
        ])],
    )]);
    let problem = read_file(&b3_xlsx::read_workbook(&book).unwrap()).unwrap_err();
    let FormatProblem::MissingColumns(kind, missing) = problem else {
        panic!("esperava colunas faltando");
    };
    assert_eq!(kind, FileKind::Trades);
    assert!(missing.contains(&"Mercado") && missing.contains(&"Preco"));
}

// --- arquivo corrompido e tetos de tamanho -----------------------------------------

fn zip_with(entries: &[(&str, usize)]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, size) in entries {
            writer.start_file(*name, options).unwrap();
            let chunk = vec![b'a'; 1024 * 1024];
            let mut left = *size;
            while left > 0 {
                let step = left.min(chunk.len());
                writer.write_all(&chunk[..step]).unwrap();
                left -= step;
            }
        }
        writer.finish().unwrap();
    }
    buffer.into_inner()
}

#[test]
fn files_that_are_not_a_readable_xlsx_are_refused_with_a_clear_reason() {
    let good = trades_sheet();
    let cases: Vec<(&str, Vec<u8>, ReadError)> = vec![
        ("vazio", Vec::new(), ReadError::NotXlsx),
        (
            "texto",
            b"Data;Valor\n01/01/2026;10\n".to_vec(),
            ReadError::NotXlsx,
        ),
        (
            "xls antigo",
            [&[0xD0, 0xCF, 0x11, 0xE0][..], &[0u8; 64][..]].concat(),
            ReadError::LegacyXls,
        ),
        (
            "cortado",
            good[..good.len() / 2].to_vec(),
            ReadError::Corrupt,
        ),
        (
            "so a assinatura",
            b"PK\x03\x04".to_vec(),
            ReadError::Corrupt,
        ),
        (
            "zip que nao e xlsx",
            zip_with(&[("leia-me.txt", 10)]),
            ReadError::Corrupt,
        ),
        (
            "lixo no meio",
            {
                let mut bytes = good.clone();
                let middle = bytes.len() / 2;
                for byte in &mut bytes[middle..middle + 64] {
                    *byte = 0xFF;
                }
                bytes
            },
            ReadError::Corrupt,
        ),
    ];
    for (name, bytes, expected) in cases {
        let error = b3_xlsx::read_workbook(&bytes).err();
        // O "lixo no meio" pode cair em dado comprimido que ainda descompacta;
        // nesse caso o que importa é nao dar panico.
        if name == "lixo no meio" {
            continue;
        }
        assert_eq!(error, Some(expected), "{name}");
        assert!(!expected.message().is_empty());
    }
}

#[test]
fn a_decompression_bomb_is_refused_before_it_is_opened() {
    let bomb = zip_with(&[("xl/worksheets/sheet1.xml", 70 * 1024 * 1024)]);
    assert!(
        bomb.len() < 1024 * 1024,
        "a bomba compacta: {} bytes",
        bomb.len()
    );
    assert_eq!(
        b3_xlsx::read_workbook(&bomb).err(),
        Some(ReadError::TooLarge)
    );

    let many: Vec<(String, usize)> = (0..=b3_xlsx::MAX_ENTRIES)
        .map(|n| (format!("f{n}.txt"), 1))
        .collect();
    let many: Vec<(&str, usize)> = many.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    assert_eq!(
        b3_xlsx::read_workbook(&zip_with(&many)).err(),
        Some(ReadError::TooLarge)
    );
}

#[test]
fn a_lone_cell_far_away_does_not_blow_the_memory() {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.write_string(0, 0, "Data do Negócio").unwrap();
    // A célula mais distante da planilha: a grade densa custaria dezenas de GB.
    sheet.write_string(1_048_575, 16_383, "x").unwrap();
    let sheets = b3_xlsx::read_workbook(&book.save_to_buffer().unwrap()).unwrap();
    assert_eq!(sheets[0].rows.len(), 1, "a coluna fora do teto e ignorada");

    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.write_string(b3_xlsx::MAX_ROWS + 10, 0, "x").unwrap();
    assert_eq!(
        b3_xlsx::read_workbook(&book.save_to_buffer().unwrap()).err(),
        Some(ReadError::TooLarge)
    );
}

// --- a rota ------------------------------------------------------------------------

const BOUNDARY: &str = "----prumo-teste-b3";

fn multipart(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, bytes) in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    text: String,
}

async fn send(router: &axum::Router, content_type: &str, body: Vec<u8>) -> Answer {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/imports/b3/preview")
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Answer {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn upload(router: &axum::Router, files: &[(&str, &[u8])]) -> Answer {
    send(
        router,
        &format!("multipart/form-data; boundary={BOUNDARY}"),
        multipart(files),
    )
    .await
}

/// Recusa toda credencial.
struct Refuse;

#[async_trait]
impl Authenticator for Refuse {
    async fn authenticate(&self, _headers: &HeaderMap) -> Result<Identity, ApiError> {
        Err(ApiError::unauthenticated("sem credencial"))
    }
}

/// Junta tudo que o `tracing` emitiu (qualquer nível) para conferir que a
/// planilha não vazou.
#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn preview_route_contract() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "importa@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );

    unauthenticated_gets_401(&db, &dir).await;
    full_preview_across_four_sheets(&app).await;
    bad_files_are_reported_per_file(&app).await;
    request_shape_is_validated(&app).await;
    body_limit_is_raised_only_for_this_route(&app).await;
    nothing_from_the_sheets_reaches_the_log(&app).await;
    nothing_is_written(&app).await;
}

async fn unauthenticated_gets_401(db: &Database, dir: &tempfile_dir::Dir) {
    let refused = app_with_auth(
        &db.store,
        Arc::new(Refuse),
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let reply = upload(&refused.router, &[("negociacao.xlsx", &trades_sheet())]).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.body["code"], "unauthenticated");
    assert!(!reply.text.contains("PETR4"));
}

async fn full_preview_across_four_sheets(app: &TestApp) {
    let (trades, movements, received, provisioned) = (
        trades_sheet(),
        movements_sheet(),
        received_sheet(),
        provisioned_sheet(),
    );
    let reply = upload(
        &app.router,
        &[
            ("movimentacao-2026.xlsx", &movements),
            ("negociacao-2026.xlsx", &trades),
            ("proventos-recebidos.xlsx", &received),
            ("proventos-a-receber.xlsx", &provisioned),
            // O mesmo arquivo duas vezes: nada entra em dobro.
            ("negociacao-2026 (1).xlsx", &trades),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    assert_eq!(
        reply.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let body = &reply.body;

    let files = body["files"].as_array().unwrap();
    assert_eq!(files.len(), 5);
    assert!(files.iter().all(|f| f["status"] == "read"));
    assert_eq!(files[0]["kinds"], json!(["movements"]));
    assert_eq!(files[1]["kinds"], json!(["trades"]));
    assert_eq!(files[2]["kinds"], json!(["income_received"]));
    assert_eq!(files[3]["kinds"], json!(["income_provisioned"]));
    assert_eq!(files[4]["accepted"], 0);

    let keys: Vec<&str> = body["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["HGLG11", "ITUB4", "PETR4", "TESOURO SELIC 2029", "VALE3"]
    );
    let petr = find(&body["assets"], "key", "PETR4");
    assert_eq!(petr["suggested_class"], "br_stocks");
    assert_eq!(petr["class_basis"], "name");
    assert_eq!(petr["listed_on_b3"], true);
    assert_eq!(petr["totals"]["bought_quantity"], 105.0);
    assert_eq!(petr["totals"]["sold_quantity"], 50.0);
    assert_eq!(petr["totals"]["net_quantity"], 55.0);
    // 150 de dividendo (uma vez, apesar de estar na Movimentação e nos Proventos) + 63,75 de JCP.
    assert_eq!(petr["totals"]["income_received"], 213.75);
    assert_eq!(petr["totals"]["income_provisioned"], 216.0);
    let fractional = find(&petr["trades"], "market", "fractional");
    assert_eq!(fractional["code"], "PETR4");
    assert_eq!(fractional["unit_price"], 0.3);
    assert!(fractional["external_ref"]
        .as_str()
        .unwrap()
        .starts_with("b3:v1:trade:"));

    assert_eq!(
        find(&body["assets"], "key", "HGLG11")["suggested_class"],
        "br_reits"
    );
    let bond = find(&body["assets"], "key", "TESOURO SELIC 2029");
    assert_eq!(bond["treasury"], true);
    assert_eq!(bond["suggested_class"], "fixed_income");
    assert_eq!(bond["listed_on_b3"], false);
    assert_eq!(bond["events"][0]["kind"], "treasury_movement");

    let unrecognized: Vec<&str> = body["unrecognized"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["reason_code"].as_str().unwrap())
        .collect();
    assert_eq!(unrecognized, ["unknown_movement", "unsupported_market"]);
    let ignored = find(&body["ignored"], "reason_code", "settlement");
    assert_eq!(ignored["count"], 1);
    assert!(
        find(&body["ignored"], "reason_code", "duplicate_in_request")["count"]
            .as_u64()
            .unwrap()
            >= 5
    );
    assert_eq!(body["summary"]["unrecognized"], 2);
    assert_eq!(body["summary"]["assets"], 5);
    assert_eq!(body["summary"]["first_date"], "2025-09-10");
    assert_eq!(body["summary"]["last_date"], "2026-11-20");

    // Idempotência: a mesma planilha em outra requisição dá os mesmos `external_ref`.
    let again = upload(&app.router, &[("negociacao.xlsx", &trades)]).await;
    let refs = |body: &Value| -> Vec<String> {
        let mut refs: Vec<String> = find(&body["assets"], "key", "PETR4")["trades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["external_ref"].as_str().unwrap().to_owned())
            .collect();
        refs.sort();
        refs
    };
    assert_eq!(refs(body), refs(&again.body));
}

async fn bad_files_are_reported_per_file(app: &TestApp) {
    let good = trades_sheet();
    let broken = good[..good.len() / 3].to_vec();
    let changed_layout = workbook(&[(
        "Negociação",
        vec![header(&[
            "Data do Negócio",
            "Código de Negociação",
            "Quantidade",
        ])],
    )]);
    let reply = upload(
        &app.router,
        &[
            ("estranha.xlsx", &stranger_sheet()),
            ("quebrada.xlsx", &broken),
            (
                "lista.csv",
                b"Produto;Valor
PETR4;100
",
            ),
            ("negociacao.xlsx", &good),
            ("layout-novo.xlsx", &changed_layout),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    let files = reply.body["files"].as_array().unwrap();
    assert_eq!(files[0]["status"], "unknown_format");
    assert_eq!(files[0]["problem"], "unknown_format");
    assert!(files[0]["message"]
        .as_str()
        .unwrap()
        .contains("Formato nao reconhecido"));
    assert_eq!(files[1]["status"], "unreadable");
    assert_eq!(files[1]["problem"], "corrupt");
    assert!(files[1]["message"].as_str().unwrap().contains("corrompida"));
    assert_eq!(files[2]["status"], "unreadable");
    assert_eq!(files[2]["problem"], "not_xlsx");
    assert_eq!(files[3]["status"], "read");
    assert_eq!(files[3]["problem"], Value::Null);
    assert_eq!(files[4]["status"], "unknown_format");
    assert_eq!(files[4]["problem"], "missing_columns");
    assert!(files[4]["missing_columns"]
        .as_array()
        .unwrap()
        .contains(&json!("Mercado")));
    assert_eq!(reply.body["summary"]["trades"], 4);
    // A mensagem de erro nunca ecoa o conteúdo.
    for file in &files[..3] {
        let message = file["message"].as_str().unwrap();
        assert!(!message.contains("PETR4") && !message.contains("Fulana"));
    }
}

async fn request_shape_is_validated(app: &TestApp) {
    // Sem arquivo nenhum.
    let none = upload(&app.router, &[]).await;
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(none.body["code"], "validation_failed");
    // Não é multipart.
    let json_body = send(&app.router, "application/json", b"{}".to_vec()).await;
    assert_eq!(json_body.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Multipart torto.
    let crooked = send(
        &app.router,
        &format!("multipart/form-data; boundary={BOUNDARY}"),
        b"isto nao e multipart".to_vec(),
    )
    .await;
    assert_eq!(crooked.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Arquivos demais.
    let sheet = trades_sheet();
    let many: Vec<(&str, &[u8])> = (0..11).map(|_| ("n.xlsx", sheet.as_slice())).collect();
    let too_many = upload(&app.router, &many).await;
    assert_eq!(too_many.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(too_many.body["detail"].as_str().unwrap().contains("10"));
    // GET não existe.
    let get = get(&app.router, "/api/v1/imports/b3/preview").await;
    assert_eq!(get.status, StatusCode::METHOD_NOT_ALLOWED);
}

async fn body_limit_is_raised_only_for_this_route(app: &TestApp) {
    // 200 KiB passa de 64 KiB e é aceito aqui (e vira "arquivo ilegivel", nao erro do envio).
    let medium = vec![b'x'; 200 * 1024];
    let reply = upload(&app.router, &[("grande.xlsx", &medium)]).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["files"][0]["status"], "unreadable");

    // Perto do teto: 9 MiB ainda passa.
    let near = vec![b'x'; 9 * 1024 * 1024];
    let reply = upload(&app.router, &[("quase.xlsx", &near)]).await;
    assert_eq!(reply.status, StatusCode::OK);

    // Acima de 10 MiB: 413 em problem+json, com Content-Length.
    let over = vec![b'x'; 10 * 1024 * 1024 + 1024];
    let reply = upload(&app.router, &[("enorme.xlsx", &over)]).await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(reply.body["code"], "payload_too_large");
    assert!(reply
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("problem+json"));

    // O mesmo acima do teto, sem Content-Length (chunked): o corte vem da leitura.
    let body = Body::from_stream(futures_util::stream::iter(
        multipart(&[("enorme.xlsx", &over)])
            .chunks(64 * 1024)
            .map(|chunk| Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>(),
    ));
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/imports/b3/preview")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(body)
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // O resto da API continua com 64 KiB.
    let big_json = json!({ "amount": 1, "padding": "x".repeat(70 * 1024) }).to_string();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/plans")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, big_json.len())
        .body(Body::from(big_json))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

async fn nothing_from_the_sheets_reaches_the_log(app: &TestApp) {
    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let broken = trades_sheet();
    let reply = upload(
        &app.router,
        &[
            ("movimentacao-secreta.xlsx", &movements_sheet()),
            ("negociacao-secreta.xlsx", &trades_sheet()),
            ("quebrada-secreta.xlsx", &broken[..broken.len() / 2]),
        ],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    let logged = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    for secret in [
        "PETR4",
        "HGLG11",
        "PETROLEO",
        "CORRETORA",
        "secreta",
        "15000",
        "3000",
        "Tesouro",
    ] {
        assert!(!logged.contains(secret), "o log tem {secret}: {logged}");
    }
}

async fn nothing_is_written(app: &TestApp) {
    // A prévia não cria ativo nem lançamento: a carteira desta pessoa segue vazia.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(portfolio.body["assets"], json!([]));
    let contributions = get(&app.router, "/api/v1/contributions").await;
    assert_eq!(contributions.body, json!([]));
}
