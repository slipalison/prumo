//! Peças comuns dos testes de integração: um Postgres 17 de verdade (o mesmo
//! major do CloudNativePG do cluster) e um provedor de cotações falso.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use prumo::auth::DevAuthenticator;
use prumo::domain::asset_class::AssetClass;
use prumo::http::{AppState, BuildInfo};
use prumo::quotes::{Quote, QuoteError, QuoteProvider, SymbolMatch};
use prumo::service::QuoteService;
use prumo::store::Store;
use rust_decimal::Decimal;
use serde_json::Value;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

pub struct Database {
    pub store: Store,
    _container: ContainerAsync<Postgres>,
}

pub async fn database() -> Database {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("porta do postgres");
    let host = container.get_host().await.expect("host do postgres");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let store = Store::connect(&url).await.expect("conexao");
    store.migrate().await.expect("migracoes");
    Database {
        store,
        _container: container,
    }
}

/// Provedor de cotações em memória. `fail` faz toda chamada falhar.
#[derive(Default)]
pub struct FakeQuotes {
    pub prices: Mutex<HashMap<String, (Decimal, &'static str, &'static str)>>,
    pub fail: Mutex<bool>,
    pub calls: Mutex<usize>,
}

impl FakeQuotes {
    pub fn with_market() -> Arc<Self> {
        let fake = FakeQuotes::default();
        {
            let mut prices = fake.prices.lock().unwrap();
            for (symbol, price, currency, name) in [
                ("KNRI11.SA", "142.00", "BRL", "Kinea Renda Imobiliaria"),
                ("HGLG11.SA", "140.80", "BRL", "Patria Log"),
                ("BTLG11.SA", "94.55", "BRL", "BTG Pactual Logistica"),
                ("TRXF11.SA", "53.81", "BRL", "TRX Real Estate"),
                ("MXRF11.SA", "8.37", "BRL", "Maxi Renda"),
                ("WEGE3.SA", "50.00", "BRL", "WEG"),
                ("QBTC11.SA", "52.00", "BRL", "QR Bitcoin"),
                ("HASH11.SA", "60.00", "BRL", "Hashdex Nasdaq Crypto Index"),
                ("IMAB11.SA", "100.00", "BRL", "It Now IMA-B"),
                ("BTC-USD", "60000", "USD", "Bitcoin USD"),
                ("AAPL", "200", "USD", "Apple Inc."),
                ("USDBRL=X", "5.50", "BRL", "USD/BRL"),
            ] {
                prices.insert(symbol.to_owned(), (price.parse().unwrap(), currency, name));
            }
        }
        Arc::new(fake)
    }
}

#[async_trait]
impl QuoteProvider for FakeQuotes {
    async fn quote(&self, provider_symbol: &str) -> Result<Quote, QuoteError> {
        *self.calls.lock().unwrap() += 1;
        if *self.fail.lock().unwrap() {
            return Err(QuoteError::Unavailable("falso fora do ar".into()));
        }
        let prices = self.prices.lock().unwrap();
        let (price, currency, name) = prices.get(provider_symbol).ok_or(QuoteError::NotFound)?;
        Ok(Quote {
            symbol: provider_symbol.to_owned(),
            price: *price,
            currency: (*currency).to_owned(),
            name: Some((*name).to_owned()),
        })
    }

    async fn search(&self, query: &str) -> Result<Vec<SymbolMatch>, QuoteError> {
        if *self.fail.lock().unwrap() {
            return Err(QuoteError::Unavailable("falso fora do ar".into()));
        }
        Ok(vec![SymbolMatch {
            symbol: query.to_uppercase(),
            display_name: format!("Resultado para {query}"),
            exchange: "SAO".into(),
            suggested_class: AssetClass::BrReits,
            listed_on_b3: true,
        }])
    }
}

pub struct TestApp {
    pub router: Router,
    pub ops: Router,
    pub state: AppState,
}

pub fn app(store: &Store, email: &str, quotes: Arc<FakeQuotes>, static_dir: &str) -> TestApp {
    let service = Arc::new(QuoteService::new(store.clone(), quotes));
    let state = AppState::new(
        store.clone(),
        Arc::new(DevAuthenticator::new(email)),
        service,
        BuildInfo {
            version: "9.9.9".into(),
            commit: "abc1234".into(),
        },
        None,
    );
    TestApp {
        router: prumo::http::public_router(state.clone(), static_dir),
        ops: prumo::http::operational_router(state.clone()),
        state,
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Value,
    pub text: String,
}

pub async fn call(router: &Router, method: Method, uri: &str, body: Option<Value>) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        headers,
        body,
        text,
    }
}

pub async fn get(router: &Router, uri: &str) -> Reply {
    call(router, Method::GET, uri, None).await
}

pub async fn post(router: &Router, uri: &str, body: Value) -> Reply {
    call(router, Method::POST, uri, Some(body)).await
}

pub async fn put(router: &Router, uri: &str, body: Value) -> Reply {
    call(router, Method::PUT, uri, Some(body)).await
}

pub async fn patch(router: &Router, uri: &str, body: Value) -> Reply {
    call(router, Method::PATCH, uri, Some(body)).await
}

pub async fn delete(router: &Router, uri: &str) -> Reply {
    call(router, Method::DELETE, uri, None).await
}
