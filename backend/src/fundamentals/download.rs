//! Download em fluxo dos arquivos da CVM: o GET condicional (ETag e
//! Last-Modified, que a CVM respeita com 304) e a leitura do corpo enquanto
//! ele chega, sem guardar o arquivo em memória nem em disco.
//!
//! O corpo passa por um canal pequeno para uma thread de bloqueio, que lê o
//! `.zip` entrada por entrada (`zip::read::read_zipfile_from_stream`) e o CSV
//! linha por linha. O que fica em memória é só o que o leitor escolhe guardar
//! (as companhias da carteira). O pod tem 192 MiB; o ITR de um ano tem 30 MB
//! compactado e 780 MB aberto.

use std::io::Read;
use std::time::Duration;

use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::StatusCode;
use tokio::sync::mpsc;

/// O maior arquivo aceito, compactado. O maior da CVM hoje (ITR) tem ~31 MB.
pub const MAX_DOWNLOAD: usize = 256 * 1024 * 1024;
/// Prazo do download inteiro: 30 MB numa rede lenta ainda cabe.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// Pedaços em trânsito entre a rede e o leitor.
const IN_FLIGHT: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("fonte indisponivel: {0}")]
    Unavailable(String),
    /// 404: o arquivo do ano ainda não existe (janeiro, antes da primeira DFP).
    #[error("arquivo ainda nao publicado")]
    NotFound,
    #[error("formato inesperado: {0}")]
    Format(String),
}

/// O que identifica uma versão do arquivo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Validators {
    pub fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }

    /// A mesma versão: o ETag, se os dois têm; senão o Last-Modified.
    pub fn same_version(&self, other: &Validators) -> bool {
        match (&self.etag, &other.etag) {
            (Some(a), Some(b)) => a == b,
            _ => match (&self.last_modified, &other.last_modified) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            },
        }
    }
}

/// O resultado de abrir o arquivo.
pub enum Opened {
    /// 304: a versão que o Prumo já leu continua valendo.
    NotModified,
    Body(Download),
}

/// Um corpo pronto para ler, com a versão que o servidor informou.
pub struct Download {
    response: reqwest::Response,
    pub validators: Validators,
}

/// Abre `url`. Com `known`, o pedido é condicional.
pub async fn open(
    http: &reqwest::Client,
    url: &str,
    known: Option<&Validators>,
) -> Result<Opened, FetchError> {
    let mut request = http.get(url).timeout(DOWNLOAD_TIMEOUT);
    if let Some(known) = known {
        if let Some(etag) = &known.etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(at) = &known.last_modified {
            request = request.header(IF_MODIFIED_SINCE, at);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|e| FetchError::Unavailable(e.without_url().to_string()))?;
    match response.status() {
        StatusCode::NOT_MODIFIED => return Ok(Opened::NotModified),
        StatusCode::NOT_FOUND => return Err(FetchError::NotFound),
        status if !status.is_success() => {
            return Err(FetchError::Unavailable(format!("status {status}")));
        }
        _ => {}
    }
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.chars().take(200).collect::<String>())
    };
    let validators = Validators {
        etag: header(ETAG),
        last_modified: header(LAST_MODIFIED),
    };
    Ok(Opened::Body(Download {
        response,
        validators,
    }))
}

/// O corpo como `Read`, alimentado pelo canal.
pub struct ChannelReader {
    rx: mpsc::Receiver<Result<Vec<u8>, String>>,
    chunk: Vec<u8>,
    at: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.at >= self.chunk.len() {
            match self.rx.blocking_recv() {
                None => return Ok(0),
                Some(Err(error)) => return Err(std::io::Error::other(error)),
                Some(Ok(chunk)) => {
                    self.chunk = chunk;
                    self.at = 0;
                }
            }
        }
        let n = buf.len().min(self.chunk.len() - self.at);
        buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

impl Download {
    /// Lê o corpo com `parse` numa thread de bloqueio, enquanto ele chega.
    /// Se o leitor termina antes do fim (a parte final do `.zip` é o
    /// diretório central, que a leitura em fluxo não usa), o resto não é baixado.
    pub async fn parse<T, F>(self, parse: F) -> Result<T, FetchError>
    where
        T: Send + 'static,
        F: FnOnce(ChannelReader) -> Result<T, FetchError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(IN_FLIGHT);
        let reader = ChannelReader {
            rx,
            chunk: Vec::new(),
            at: 0,
        };
        let worker = tokio::task::spawn_blocking(move || parse(reader));
        let mut response = self.response;
        let mut total = 0usize;
        let mut network: Option<FetchError> = None;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    total += chunk.len();
                    if total > MAX_DOWNLOAD {
                        network = Some(FetchError::Format("arquivo grande demais".to_owned()));
                        let _ = tx.send(Err("arquivo grande demais".to_owned())).await;
                        break;
                    }
                    if tx.send(Ok(chunk.to_vec())).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let message = error.without_url().to_string();
                    network = Some(FetchError::Unavailable(message.clone()));
                    let _ = tx.send(Err(message)).await;
                    break;
                }
            }
        }
        drop(tx);
        let parsed = worker
            .await
            .map_err(|_| FetchError::Format("leitura interrompida".to_owned()))?;
        match (parsed, network) {
            (Ok(value), _) => Ok(value),
            // A rede caiu no meio: o erro do leitor é consequência.
            (Err(_), Some(error)) => Err(error),
            (Err(error), None) => Err(error),
        }
    }
}

/// Um leitor que recusa passar de `limit` bytes (CSV aberto de tamanho absurdo
/// é arquivo errado ou bomba de descompressão, nunca dado a cortar em silêncio).
pub struct Limited<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Limited<R> {
    pub fn new(inner: R, limit: u64) -> Self {
        Self { inner, left: limit }
    }
}

impl<R: Read> Read for Limited<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        let n64 = u64::try_from(n).unwrap_or(u64::MAX);
        if n64 > self.left {
            return Err(std::io::Error::other("arquivo aberto grande demais"));
        }
        self.left -= n64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validators_compare_by_etag_then_date() {
        let a = Validators {
            etag: Some("\"1\"".into()),
            last_modified: Some("x".into()),
        };
        let b = Validators {
            etag: Some("\"2\"".into()),
            last_modified: Some("x".into()),
        };
        assert!(!a.same_version(&b));
        assert!(a.same_version(&a.clone()));
        let only_date = Validators {
            etag: None,
            last_modified: Some("x".into()),
        };
        assert!(a.same_version(&only_date));
        assert!(!Validators::default().same_version(&Validators::default()));
        assert!(Validators::default().is_empty());
    }

    #[test]
    fn limited_refuses_past_the_limit() {
        let mut ok = Limited::new(&b"abc"[..], 3);
        let mut out = Vec::new();
        assert!(ok.read_to_end(&mut out).is_ok());
        let mut too_big = Limited::new(&b"abcd"[..], 3);
        assert!(too_big.read_to_end(&mut Vec::new()).is_err());
    }
}
