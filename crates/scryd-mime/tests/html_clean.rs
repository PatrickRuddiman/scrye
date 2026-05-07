use scryd_mime::html_clean::pre_clean;

#[test]
fn drops_script_blocks() {
    let html = r#"<p>before</p><script>alert(1)</script><p>after</p>"#;
    let out = pre_clean(html);
    assert!(out.contains("before"));
    assert!(out.contains("after"));
    assert!(!out.contains("alert"));
    assert!(!out.contains("<script"));
}

#[test]
fn drops_style_blocks() {
    let html = r#"<style>body { color: red; }</style><p>visible</p>"#;
    let out = pre_clean(html);
    assert!(out.contains("visible"));
    assert!(!out.contains("color: red"));
    assert!(!out.contains("<style"));
}

#[test]
fn drops_head_subtree() {
    let html = r#"<html><head><title>t</title><meta></head><body><p>visible</p></body></html>"#;
    let out = pre_clean(html);
    assert!(out.contains("visible"));
    assert!(!out.contains("<title"));
    assert!(!out.contains("<meta"));
}

#[test]
fn drops_html_comments() {
    let html = "<p>before</p><!-- secret --><p>after</p>";
    let out = pre_clean(html);
    assert!(out.contains("before"));
    assert!(out.contains("after"));
    assert!(!out.contains("secret"));
    assert!(!out.contains("<!--"));
}

#[test]
fn drops_one_pixel_image_in_either_attr_order() {
    for img in [
        r#"<img width="1" height="1" src="t.gif"/>"#,
        r#"<img height="1" width="1" src="t.gif"/>"#,
        r#"<img width=1 height=1 src=t.gif>"#,
        r#"<img width="0" height="0" src="t.gif"/>"#,
    ] {
        let html = format!("<p>before</p>{img}<p>after</p>");
        let out = pre_clean(&html);
        assert!(out.contains("before"));
        assert!(out.contains("after"));
        assert!(
            !out.contains("t.gif"),
            "1x1 img leaked through pre_clean: {out}"
        );
    }
}

#[test]
fn keeps_normal_images() {
    let html = r#"<p>before</p><img src="cat.png" width="640" height="480"/><p>after</p>"#;
    let out = pre_clean(html);
    assert!(out.contains("cat.png"), "normal-sized image incorrectly dropped");
}

#[test]
fn drops_inline_style_attributes_double_and_single_quoted() {
    let html = r#"<p style="color: red">a</p><p style='font-size:12px'>b</p>"#;
    let out = pre_clean(html);
    assert!(out.contains("<p>a</p>"));
    assert!(out.contains("<p>b</p>"));
    assert!(!out.contains("color: red"));
    assert!(!out.contains("font-size"));
}

#[test]
fn safe_html_passes_through_unchanged() {
    let html = "<p>hello</p><a href=\"/x\">link</a>";
    let out = pre_clean(html);
    assert_eq!(out, html);
}
