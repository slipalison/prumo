//! O logo de um ativo: o que o Prumo aceita guardar e servir do próprio
//! domínio.
//!
//! O logo vem de um terceiro (a brapi) e é servido pelo domínio do Prumo, onde
//! um SVG com `<script>` ou `onload=` seria código de terceiro rodando na
//! origem do app. Por isso o SVG nunca é repassado como chegou: este módulo o
//! **lê e escreve de novo** com uma lista do que é permitido (formas,
//! gradientes e atributos de desenho). O que não está na lista não passa: um
//! elemento fora dela, um `on*=`, um `javascript:`, um `<foreignObject>`, uma
//! referência a fora do arquivo, um `DOCTYPE`. Nesses casos o logo inteiro é
//! recusado (e o ativo fica com as iniciais), em vez de tentar consertá-lo.
//!
//! A resposta HTTP ainda leva uma CSP que não deixa nada rodar nem carregar
//! (`docs/design.md`): este saneamento é a primeira camada, a CSP a segunda.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// Maior SVG aceito de fora, em bytes. Os logos da brapi têm de 0,3 a 3 kB.
pub const MAX_SVG_BYTES: usize = 128 * 1024;
/// Profundidade máxima de elementos aninhados.
const MAX_DEPTH: usize = 32;
/// Máximo de elementos escritos: um logo é um punhado de formas.
const MAX_ELEMENTS: usize = 2_000;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Elementos que o logo pode ter. Nada que carregue, rode ou repita outro
/// elemento (`use`, `image`, `script`, `style`, `a`, `animate`, `foreignObject`,
/// `text`...).
const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
];

/// Elementos sem efeito no desenho, descartados com o que há dentro deles.
const IGNORED_ELEMENTS: &[&str] = &["title", "desc", "metadata"];

/// Atributos de desenho que passam. Os demais atributos sem perigo são
/// descartados em silêncio; os perigosos recusam o logo.
const ALLOWED_ATTRIBUTES: &[&str] = &[
    "id",
    "d",
    "points",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "fx",
    "fy",
    "width",
    "height",
    "viewBox",
    "preserveAspectRatio",
    "transform",
    "gradientTransform",
    "gradientUnits",
    "clipPathUnits",
    "spreadMethod",
    "offset",
    "fill",
    "fill-rule",
    "fill-opacity",
    "clip-rule",
    "clip-path",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stop-color",
    "stop-opacity",
    "opacity",
    "style",
    "href",
    "xlink:href",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgError {
    #[error("svg grande demais ({0} bytes)")]
    TooLarge(usize),
    #[error("svg que nao e texto utf-8")]
    NotUtf8,
    #[error("xml mal formado: {0}")]
    Malformed(String),
    #[error("o arquivo nao e um svg")]
    NotSvg,
    #[error("elemento nao permitido: {0}")]
    ForbiddenElement(String),
    #[error("atributo nao permitido: {0}")]
    ForbiddenAttribute(String),
    #[error("valor nao permitido em {0}")]
    ForbiddenValue(String),
    #[error("svg complexo demais")]
    TooComplex,
}

/// Lê o SVG e devolve outro, só com o permitido. Qualquer coisa fora da lista
/// que possa rodar código ou buscar algo de fora recusa o arquivo todo.
pub fn sanitize_svg(input: &[u8]) -> Result<Vec<u8>, SvgError> {
    if input.len() > MAX_SVG_BYTES {
        return Err(SvgError::TooLarge(input.len()));
    }
    let text = std::str::from_utf8(input).map_err(|_| SvgError::NotUtf8)?;
    let text = text.trim_start_matches('\u{feff}');

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut out = String::with_capacity(text.len());
    // Pilha de elementos escritos; `skipping` conta a profundidade dentro de um
    // elemento ignorado.
    let mut open: Vec<String> = Vec::new();
    let mut skipping = 0usize;
    let mut elements = 0usize;
    let mut root_closed = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| SvgError::Malformed(e.to_string()))?;
        if skipping > 0 {
            match event {
                Event::Start(_) => skipping += 1,
                Event::End(_) => skipping -= 1,
                Event::Eof => return Err(SvgError::Malformed("fim inesperado".to_owned())),
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(e) | Event::Empty(e) if root_closed => {
                return Err(SvgError::Malformed(format!(
                    "elemento depois da raiz: {}",
                    name_of(&e)
                )));
            }
            Event::Start(e) => {
                let name = name_of(&e);
                if IGNORED_ELEMENTS.contains(&name.as_str()) {
                    skipping = 1;
                    continue;
                }
                write_start(&mut out, &e, &name, &open, false, &mut elements)?;
                open.push(name);
                if open.len() > MAX_DEPTH {
                    return Err(SvgError::TooComplex);
                }
            }
            Event::Empty(e) => {
                let name = name_of(&e);
                if IGNORED_ELEMENTS.contains(&name.as_str()) {
                    continue;
                }
                write_start(&mut out, &e, &name, &open, true, &mut elements)?;
                if open.is_empty() {
                    root_closed = true;
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref().as_bytes()).into_owned();
                if open.pop().as_deref() != Some(name.as_str()) {
                    return Err(SvgError::Malformed(
                        "tag de fechamento fora de lugar".into(),
                    ));
                }
                out.push_str("</");
                out.push_str(&name);
                out.push('>');
                if open.is_empty() {
                    root_closed = true;
                }
            }
            // Um logo não tem texto: só espaço entre as tags, que o leitor já
            // aparou. Texto, CDATA e entidade soltos recusam.
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                return Err(SvgError::ForbiddenElement("texto".to_owned()));
            }
            // `<!DOCTYPE>` carrega entidades externas e internas (XXE, bomba de
            // expansão): nunca.
            Event::DocType(_) => return Err(SvgError::ForbiddenElement("DOCTYPE".to_owned())),
            // Declaração `<?xml?>`, comentário e instrução de processamento não
            // são reescritos.
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
    }
    if !open.is_empty() {
        return Err(SvgError::Malformed("elemento sem fechar".to_owned()));
    }
    if !out.starts_with("<svg") {
        return Err(SvgError::NotSvg);
    }
    Ok(out.into_bytes())
}

fn name_of(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref().as_bytes()).into_owned()
}

fn write_start(
    out: &mut String,
    e: &BytesStart<'_>,
    name: &str,
    open: &[String],
    empty: bool,
    elements: &mut usize,
) -> Result<(), SvgError> {
    if open.is_empty() != (name == "svg") {
        // A raiz tem de ser `svg`, e `svg` aninhado fica de fora.
        return Err(if open.is_empty() {
            SvgError::NotSvg
        } else {
            SvgError::ForbiddenElement(name.to_owned())
        });
    }
    if !ALLOWED_ELEMENTS.contains(&name) {
        return Err(SvgError::ForbiddenElement(name.to_owned()));
    }
    *elements += 1;
    if *elements > MAX_ELEMENTS {
        return Err(SvgError::TooComplex);
    }
    out.push('<');
    out.push_str(name);
    if name == "svg" {
        out.push_str(" xmlns=\"");
        out.push_str(SVG_NS);
        out.push('"');
    }
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|e| SvgError::Malformed(e.to_string()))?;
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| SvgError::Malformed(e.to_string()))?;
        check_attribute(&key, &value)?;
        if key.starts_with("xmlns") || !ALLOWED_ATTRIBUTES.contains(&key.as_str()) {
            continue;
        }
        out.push(' ');
        out.push_str(&key);
        out.push_str("=\"");
        out.push_str(&escape(&value));
        out.push('"');
    }
    out.push_str(if empty { "/>" } else { ">" });
    Ok(())
}

/// Recusa o que é perigoso, esteja o atributo na lista ou não: um valor que
/// aponta para fora, código, ou um atributo de evento.
fn check_attribute(key: &str, value: &str) -> Result<(), SvgError> {
    let forbidden = || SvgError::ForbiddenAttribute(key.to_owned());
    let bad_value = || SvgError::ForbiddenValue(key.to_owned());
    let lower_key = key.to_ascii_lowercase();
    if lower_key.starts_with("on") {
        return Err(forbidden());
    }
    // Declarações de namespace: o `xmlns` da raiz e o do `xlink` têm valor
    // certo; as outras (editores como o Inkscape deixam várias) são só
    // descartadas, e elemento com esse prefixo não está na lista de qualquer jeito.
    if key == "xmlns" || key == "xmlns:xlink" {
        let expected = if key == "xmlns" { SVG_NS } else { XLINK_NS };
        return if value == expected {
            Ok(())
        } else {
            Err(bad_value())
        };
    }
    if key.starts_with("xmlns:") {
        return Ok(());
    }
    if key.len() > 40 || value.len() > MAX_SVG_BYTES {
        return Err(forbidden());
    }
    if value.contains(['<', '>', '"', '\'', '\\', '&', '\0'])
        || value.chars().any(|c| c.is_control() && !c.is_whitespace())
    {
        return Err(bad_value());
    }
    let lower = value.to_ascii_lowercase();
    for scheme in [
        "javascript:",
        "vbscript:",
        "data:",
        "file:",
        "http:",
        "https:",
        "//",
        "expression(",
        "@import",
        "-moz-binding",
    ] {
        if lower.contains(scheme) {
            return Err(bad_value());
        }
    }
    // `url(...)` só aponta para dentro do arquivo (`url(#gradiente)`).
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("url(") {
        rest = rest[at + 4..].trim_start();
        if !rest.starts_with('#') {
            return Err(bad_value());
        }
    }
    // `href` (herança de gradiente) só para âncora do próprio arquivo.
    if (key == "href" || key == "xlink:href") && !value.starts_with('#') {
        return Err(bad_value());
    }
    // Dados de caminho e de transformação têm alfabeto próprio.
    let charset = match key {
        "d" | "points" => Some("MmLlHhVvCcSsQqTtAaZzEe0123456789 \t\r\n,.+-"),
        "transform" | "gradientTransform" => {
            Some("matrixtranslatescalerotateskewXY0123456789 \t\r\n,.+-()eE")
        }
        _ => None,
    };
    if let Some(allowed) = charset {
        if !value.chars().all(|c| allowed.contains(c)) {
            return Err(bad_value());
        }
    }
    if key == "id"
        && !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
    {
        return Err(bad_value());
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// O símbolo como o cache de logos o guarda: maiúsculas e dígitos, até 12.
pub fn icon_symbol(raw: &str) -> Option<String> {
    let upper = raw.trim().to_ascii_uppercase();
    let valid =
        !upper.is_empty() && upper.len() <= 12 && upper.chars().all(|c| c.is_ascii_alphanumeric());
    valid.then_some(upper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GOOD: &str = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="56" height="56" viewBox="0 0 56 56">
  <title>Empresa Exemplo</title>
  <defs>
    <linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#00488E"/><stop offset="1" stop-color="#fff"/></linearGradient>
  </defs>
  <path fill="url(#a)" d="M0 0h56v56H0z"/>
  <g transform="translate(2 2)"><path fill="#fff" fill-rule="evenodd" d="M14.022 32.084 14 31.256v-6.629"/></g>
</svg>"##;

    fn clean(svg: &str) -> Result<String, SvgError> {
        sanitize_svg(svg.as_bytes()).map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn a_plain_logo_passes_and_is_rewritten() {
        let out = clean(GOOD).unwrap();
        assert!(out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(out.contains("<linearGradient id=\"a\""));
        assert!(out.contains("fill=\"url(#a)\""));
        assert!(out.contains("fill-rule=\"evenodd\""));
        // Título e declaração XML ficam de fora.
        assert!(!out.contains("Empresa"));
        assert!(!out.contains("<?xml"));
        assert!(!out.contains("xlink"));
    }

    #[test]
    fn sanitizing_is_idempotent() {
        let once = clean(GOOD).unwrap();
        assert_eq!(clean(&once).unwrap(), once);
    }

    #[test]
    fn the_attacks_are_refused() {
        let wrap = |inner: &str| {
            format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\">{inner}</svg>")
        };
        let cases = [
            // script, em todas as grafias
            wrap("<script>alert(1)</script>"),
            wrap("<SCRIPT>alert(1)</SCRIPT>"),
            wrap("<svg:script xmlns:svg=\"http://www.w3.org/2000/svg\">alert(1)</svg:script>"),
            wrap("<script><![CDATA[alert(1)]]></script>"),
            // evento
            "<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"alert(1)\"/>".to_owned(),
            wrap("<path d=\"M0 0\" onclick=\"alert(1)\"/>"),
            wrap("<path d=\"M0 0\" ONMOUSEOVER=\"alert(1)\"/>"),
            // foreignObject com HTML dentro
            wrap("<foreignObject><iframe src=\"https://exemplo.test\"/></foreignObject>"),
            wrap("<foreignObject><body xmlns=\"http://www.w3.org/1999/xhtml\"/></foreignObject>"),
            // links e imagens
            wrap("<a href=\"javascript:alert(1)\"><path d=\"M0 0\"/></a>"),
            wrap("<image href=\"https://exemplo.test/x.png\"/>"),
            wrap("<use href=\"https://exemplo.test/x.svg#a\"/>"),
            wrap("<use xlink:href=\"data:image/svg+xml;base64,AAAA\"/>"),
            wrap("<path d=\"M0 0\" href=\"javascript:alert(1)\"/>"),
            wrap("<linearGradient id=\"a\" xlink:href=\"https://exemplo.test/g.svg#a\"/>"),
            // referência externa em valor de desenho
            wrap("<path d=\"M0 0\" fill=\"url(https://exemplo.test/g.svg#a)\"/>"),
            wrap("<path d=\"M0 0\" style=\"fill:url('https://exemplo.test/x')\"/>"),
            wrap("<path d=\"M0 0\" style=\"background:url(//exemplo.test/x)\"/>"),
            wrap("<path d=\"M0 0\" style=\"@import 'https://exemplo.test/x.css'\"/>"),
            wrap("<path d=\"M0 0\" style=\"width:expression(alert(1))\"/>"),
            wrap("<path d=\"M0 0\" fill=\"javascript:alert(1)\"/>"),
            wrap("<path d=\"M0 0\" fill=\"  JaVaScRiPt:alert(1)\"/>"),
            // animação que escreve atributo
            wrap("<path d=\"M0 0\"><set attributeName=\"onclick\" to=\"alert(1)\"/></path>"),
            wrap("<path d=\"M0 0\"><animate attributeName=\"href\" values=\"javascript:alert(1)\"/></path>"),
            // folha de estilo
            wrap("<style>@import url(https://exemplo.test/x.css);</style>"),
            // dados de caminho com algo que não é caminho
            wrap("<path d=\"M0 0 <script>\"/>"),
            // entidades e DOCTYPE (XXE, bomba de expansão)
            "<!DOCTYPE svg [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]><svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_owned(),
            "<!DOCTYPE svg SYSTEM \"file:///etc/passwd\"><svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_owned(),
            wrap("&lt;script&gt;"),
            // texto solto
            wrap("alert(1)"),
            // namespace trocado
            "<svg xmlns=\"http://www.w3.org/1999/xhtml\"/>".to_owned(),
            // não é svg
            "<html><body>oi</body></html>".to_owned(),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><svg><path d=\"M0 0\"/></svg></svg>".to_owned(),
        ];
        for case in cases {
            assert!(clean(&case).is_err(), "passou: {case}");
        }
    }

    #[test]
    fn each_attack_is_refused_for_the_right_reason() {
        let open = "<svg xmlns=\"http://www.w3.org/2000/svg\"";
        assert_eq!(
            clean(&format!("{open}><script>alert(1)</script></svg>")),
            Err(SvgError::ForbiddenElement("script".to_owned()))
        );
        assert_eq!(
            clean(&format!("{open}><foreignObject/></svg>")),
            Err(SvgError::ForbiddenElement("foreignObject".to_owned()))
        );
        assert_eq!(
            clean(&format!("{open} onload=\"alert(1)\"/>")),
            Err(SvgError::ForbiddenAttribute("onload".to_owned()))
        );
        assert_eq!(
            clean(&format!(
                "{open}><path d=\"M0 0\" fill=\"javascript:alert(1)\"/></svg>"
            )),
            Err(SvgError::ForbiddenValue("fill".to_owned()))
        );
        assert_eq!(
            clean(&format!(
                "{open}><path d=\"M0 0\" fill=\"url(https://exemplo.test/x)\"/></svg>"
            )),
            Err(SvgError::ForbiddenValue("fill".to_owned()))
        );
        assert_eq!(
            clean(&format!("<!DOCTYPE svg>{open}/>")),
            Err(SvgError::ForbiddenElement("DOCTYPE".to_owned()))
        );
        assert_eq!(
            clean("<svg xmlns=\"http://exemplo.test/outro\"/>"),
            Err(SvgError::ForbiddenValue("xmlns".to_owned()))
        );
    }

    #[test]
    fn the_output_of_a_good_logo_has_no_active_content() {
        let out = clean(GOOD).unwrap().to_ascii_lowercase();
        for forbidden in ["<script", "javascript", "href", "<style", "<image", "<use"] {
            assert!(!out.contains(forbidden), "{forbidden}");
        }
        // A única URL é o namespace do próprio SVG.
        assert_eq!(out.matches("http").count(), 1);
    }

    #[test]
    fn unknown_harmless_attributes_are_dropped_not_refused() {
        let out = clean(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"http://exemplo.test/i\" inkscape:version=\"1\" class=\"x\" data-a=\"b\"><path d=\"M0 0\" class=\"y\" fill=\"#000\"/></svg>",
        )
        .unwrap();
        assert_eq!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0\" fill=\"#000\"/></svg>"
        );
    }

    #[test]
    fn size_depth_and_count_are_capped() {
        let big = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0 {}\"/></svg>",
            "L1 1 ".repeat(MAX_SVG_BYTES)
        );
        assert!(matches!(clean(&big), Err(SvgError::TooLarge(_))));

        let deep = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">{}{}</svg>",
            "<g>".repeat(MAX_DEPTH + 1),
            "</g>".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(clean(&deep), Err(SvgError::TooComplex));

        let many = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">{}</svg>",
            "<path d=\"M0 0\"/>".repeat(MAX_ELEMENTS + 1)
        );
        assert_eq!(clean(&many), Err(SvgError::TooComplex));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for input in [
            &b""[..],
            b"\xff\xfe\x00",
            b"<svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\">",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"></g>",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/><svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            b"plain text",
        ] {
            assert!(sanitize_svg(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn symbols_are_upper_case_and_short() {
        assert_eq!(icon_symbol(" itsa4 ").as_deref(), Some("ITSA4"));
        assert_eq!(icon_symbol("KNRI11").as_deref(), Some("KNRI11"));
        for bad in [
            "",
            "TD-IPCA-20350515",
            "A/B",
            "../x",
            "ABCDEFGHIJKLM",
            "ação",
        ] {
            assert_eq!(icon_symbol(bad), None, "{bad}");
        }
    }

    proptest! {
        #[test]
        fn any_accepted_output_is_clean_and_stable(body in "[ -~\n]{0,300}") {
            let input = format!("<svg xmlns=\"http://www.w3.org/2000/svg\">{body}</svg>");
            if let Ok(out) = sanitize_svg(input.as_bytes()) {
                let text = String::from_utf8(out.clone()).unwrap().to_ascii_lowercase();
                prop_assert!(!text.contains("<script"));
                prop_assert!(!text.contains("javascript:"));
                prop_assert!(!text.contains("<!doctype"));
                prop_assert!(!text.contains("foreignobject"));
                prop_assert_eq!(sanitize_svg(&out).unwrap(), out);
            }
        }

        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
            let _ = sanitize_svg(&bytes);
        }
    }
}
