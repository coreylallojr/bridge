//! Port of tests/unit/meta.test.ts.
use bridge_core::meta_parse::{decode_entities, icon_ext_for, parse_html_meta, should_fetch_meta};

// ---- parseHtmlMeta

#[test]
fn reads_the_title_decodes_entities_and_collapses_whitespace() {
    let html = "<html><head><title>\n  Tom &amp; Jerry&#39;s   &quot;Page&quot; &#x2014; &lt;home&gt;\n</title></head></html>";
    assert_eq!(parse_html_meta(html, "https://example.com/a").title.as_deref(), Some("Tom & Jerry's \"Page\" \u{2014} <home>"));
}

#[test]
fn returns_no_title_when_missing_or_blank() {
    assert_eq!(parse_html_meta("<html></html>", "https://example.com/").title, None);
    assert_eq!(parse_html_meta("<title>   </title>", "https://example.com/").title, None);
}

#[test]
fn resolves_a_relative_icon_href_against_the_page_url() {
    let html = "<link rel=\"stylesheet\" href=\"/a.css\"><link rel=\"shortcut icon\" href=\"/static/fav.png\">";
    assert_eq!(
        parse_html_meta(html, "https://example.com/docs/page").icon_href.as_deref(),
        Some("https://example.com/static/fav.png")
    );
}

#[test]
fn handles_single_quotes_unquoted_attrs_attribute_order_and_case() {
    assert_eq!(
        parse_html_meta("<LINK HREF='img/i.svg' REL='Icon'>", "https://example.com/x/").icon_href.as_deref(),
        Some("https://example.com/x/img/i.svg")
    );
    assert_eq!(
        parse_html_meta("<link rel=icon href=//cdn.example.com/i.ico>", "https://example.com/").icon_href.as_deref(),
        Some("https://cdn.example.com/i.ico")
    );
}

#[test]
fn falls_back_to_apple_touch_icon_and_ignores_data_and_non_http_icons() {
    assert_eq!(
        parse_html_meta("<link rel=\"apple-touch-icon\" href=\"/t.png\">", "https://example.com/").icon_href.as_deref(),
        Some("https://example.com/t.png")
    );
    assert_eq!(
        parse_html_meta("<link rel=\"icon\" href=\"data:image/png;base64,AAAA\">", "https://example.com/").icon_href,
        None
    );
    assert_eq!(parse_html_meta("<link rel=\"icon\" href=\"javascript:x\">", "https://example.com/").icon_href, None);
}

#[test]
fn does_not_treat_rel_iconic_as_an_icon() {
    assert_eq!(parse_html_meta("<link rel=\"iconic\" href=\"/x.png\">", "https://example.com/").icon_href, None);
}

// ---- shouldFetchMeta

#[test]
fn should_fetch_meta_only_allows_http_https_and_never_domo_com() {
    assert!(should_fetch_meta("https://example.com/"));
    assert!(should_fetch_meta("http://example.com/"));
    assert!(!should_fetch_meta("https://acme.domo.com/page/1"));
    assert!(!should_fetch_meta("https://ACME.DOMO.COM/"));
    assert!(!should_fetch_meta("https://domo.com/"));
    assert!(should_fetch_meta("https://notdomo.com/"));
    assert!(!should_fetch_meta("ftp://example.com/"));
    assert!(!should_fetch_meta("nonsense"));
}

// ---- iconExtFor

#[test]
fn icon_ext_for_maps_image_content_types_to_file_extensions() {
    assert_eq!(icon_ext_for(Some("image/png")), Some("png"));
    assert_eq!(icon_ext_for(Some("image/x-icon")), Some("ico"));
    assert_eq!(icon_ext_for(Some("image/vnd.microsoft.icon")), Some("ico"));
    assert_eq!(icon_ext_for(Some("image/svg+xml; charset=utf-8")), Some("svg"));
    assert_eq!(icon_ext_for(Some("text/html")), None);
    assert_eq!(icon_ext_for(None), None);
}

// ---- extra (Rust port)

#[test]
fn og_title_fallback_and_entity_edge_cases() {
    let html = "<meta name=\"description\" content=\"x\"><meta property='OG:Title' content=' Hello&nbsp;there '>";
    assert_eq!(parse_html_meta(html, "https://example.com/").title.as_deref(), Some("Hello there"));
    assert_eq!(decode_entities("&bogus; &#0; &#x110000; &AMP; &#65;"), "&bogus; &#0; &#x110000; & A");
}
