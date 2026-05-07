//! HTML pre-clean. Strips noise that's pure marketing-mail bloat: scripts,
//! styles, head, comments, 1×1 tracking pixels, and inline `style=""`
//! attributes. Implemented with regex; no DOM parser is needed for this
//! coarse-grained removal pass before `htmd` runs over the remainder.

use std::sync::LazyLock;

use regex::Regex;

static SCRIPT_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").unwrap());
static STYLE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").unwrap());
static HEAD_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<head\b[^>]*>.*?</head\s*>").unwrap());
static COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<!--.*?-->").unwrap());

// Two orderings of width/height so the regex stays simple and fast.
static TINY_IMG_WH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?is)<img\b[^>]*\bwidth\s*=\s*["']?[01]["']?[^>]*\bheight\s*=\s*["']?[01]["']?[^>]*/?\s*>"#,
    )
    .unwrap()
});
static TINY_IMG_HW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?is)<img\b[^>]*\bheight\s*=\s*["']?[01]["']?[^>]*\bwidth\s*=\s*["']?[01]["']?[^>]*/?\s*>"#,
    )
    .unwrap()
});

static STYLE_ATTR_DQ: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\s+style\s*=\s*"[^"]*""#).unwrap());
static STYLE_ATTR_SQ: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\s+style\s*=\s*'[^']*'"#).unwrap());

/// Strip the closed-list of nodes/attributes the spec enumerates from a
/// snippet of HTML, returning a copy. The output is still HTML; the caller
/// runs `htmd` over the result to produce Markdown.
pub fn pre_clean(html: &str) -> String {
    let s = SCRIPT_BLOCK.replace_all(html, "");
    let s = STYLE_BLOCK.replace_all(&s, "");
    let s = HEAD_BLOCK.replace_all(&s, "");
    let s = COMMENT.replace_all(&s, "");
    let s = TINY_IMG_WH.replace_all(&s, "");
    let s = TINY_IMG_HW.replace_all(&s, "");
    let s = STYLE_ATTR_DQ.replace_all(&s, "");
    let s = STYLE_ATTR_SQ.replace_all(&s, "");
    s.into_owned()
}
