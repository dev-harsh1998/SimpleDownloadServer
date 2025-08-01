use hdl_sv::templates::TemplateEngine;
use std::collections::HashMap;

/// Test that templates are properly embedded and can render without filesystem access
#[test]
fn test_embedded_templates_functionality() {
    let engine = TemplateEngine::new();

    // Test directory listing template rendering
    let mut variables = HashMap::new();
    variables.insert("PATH".to_string(), "/test/path".to_string());
    variables.insert("ENTRY_COUNT".to_string(), "5".to_string());
    variables.insert(
        "ENTRIES".to_string(),
        "<tr><td>test file</td></tr>".to_string(),
    );

    let result = engine.render("directory_index", &variables);
    assert!(
        result.is_ok(),
        "Directory template should render successfully"
    );

    let html = result.unwrap();
    assert!(
        html.contains("/test/path"),
        "Should contain the path variable"
    );
    assert!(html.contains("test file"), "Should contain the entries");
    assert!(
        html.contains("/_static/directory/styles.css"),
        "Should reference embedded CSS"
    );
    assert!(
        html.contains("/_static/directory/script.js"),
        "Should reference embedded JS"
    );

    // Test error page template rendering
    let mut error_vars = HashMap::new();
    error_vars.insert("STATUS_CODE".to_string(), "404".to_string());
    error_vars.insert("STATUS_TEXT".to_string(), "Not Found".to_string());
    error_vars.insert(
        "DESCRIPTION".to_string(),
        "The requested resource was not found.".to_string(),
    );

    let error_result = engine.render("error_page", &error_vars);
    assert!(
        error_result.is_ok(),
        "Error template should render successfully"
    );

    let error_html = error_result.unwrap();
    assert!(error_html.contains("404"), "Should contain the status code");
    assert!(
        error_html.contains("Not Found"),
        "Should contain the status text"
    );
    assert!(
        error_html.contains("/_static/error/styles.css"),
        "Should reference embedded error CSS"
    );
    assert!(
        error_html.contains("/_static/error/script.js"),
        "Should reference embedded error JS"
    );
}

/// Test static asset retrieval
#[test]
fn test_embedded_static_assets() {
    let engine = TemplateEngine::new();

    // Test directory CSS
    let css = engine.get_static_asset("directory/styles.css");
    assert!(css.is_some(), "Directory CSS should be available");
    let (css_content, css_type) = css.unwrap();
    assert_eq!(css_type, "text/css");
    assert!(css_content.contains("Professional Blackish Grey Design"));
    assert!(css_content.contains("--bg-primary: #0a0a0a"));

    // Test directory JS
    let js = engine.get_static_asset("directory/script.js");
    assert!(js.is_some(), "Directory JS should be available");
    let (js_content, js_type) = js.unwrap();
    assert_eq!(js_type, "application/javascript");
    assert!(js_content.contains("DOMContentLoaded"));
    assert!(js_content.contains("loading animation"));

    // Test error CSS
    let error_css = engine.get_static_asset("error/styles.css");
    assert!(error_css.is_some(), "Error CSS should be available");
    let (error_css_content, error_css_type) = error_css.unwrap();
    assert_eq!(error_css_type, "text/css");
    assert!(error_css_content.contains("Professional Blackish Grey Error Page Design"));

    // Test error JS
    let error_js = engine.get_static_asset("error/script.js");
    assert!(error_js.is_some(), "Error JS should be available");
    let (error_js_content, error_js_type) = error_js.unwrap();
    assert_eq!(error_js_type, "application/javascript");
    assert!(error_js_content.contains("Keyboard shortcuts"));

    // Test non-existent asset
    let nonexistent = engine.get_static_asset("nonexistent/file.css");
    assert!(
        nonexistent.is_none(),
        "Non-existent asset should return None"
    );
}

/// Test directory listing rendering with embedded templates
#[test]
fn test_directory_listing_rendering() {
    let engine = TemplateEngine::new();

    let test_entries = vec![
        (
            "file1.txt".to_string(),
            "1.2 KB".to_string(),
            "2 hours ago".to_string(),
        ),
        (
            "directory/".to_string(),
            "-".to_string(),
            "1 day ago".to_string(),
        ),
        (
            "file2.zip".to_string(),
            "45.8 MB".to_string(),
            "3 days ago".to_string(),
        ),
    ];

    let result = engine.render_directory_listing("/downloads", &test_entries, 3);
    assert!(
        result.is_ok(),
        "Directory listing should render successfully"
    );

    let html = result.unwrap();

    // Should contain all test entries
    assert!(html.contains("file1.txt"), "Should contain file1.txt");
    assert!(html.contains("directory/"), "Should contain directory/");
    assert!(html.contains("file2.zip"), "Should contain file2.zip");
    assert!(html.contains("1.2 KB"), "Should contain file sizes");
    assert!(html.contains("45.8 MB"), "Should contain large file size");

    // Should contain proper HTML structure
    assert!(html.contains("<table>"), "Should contain table structure");
    assert!(html.contains("file-link"), "Should contain styled links");
    assert!(
        html.contains("file-type directory"),
        "Should identify directories"
    );
    assert!(html.contains("file-type file"), "Should identify files");

    // Should reference embedded assets
    assert!(
        html.contains("/_static/directory/styles.css"),
        "Should reference CSS"
    );
    assert!(
        html.contains("/_static/directory/script.js"),
        "Should reference JS"
    );
}

/// Test error page rendering with embedded templates
#[test]
fn test_error_page_rendering() {
    let engine = TemplateEngine::new();

    let result = engine.render_error_page(404, "Not Found", "The requested file was not found");
    assert!(result.is_ok(), "Error page should render successfully");

    let html = result.unwrap();

    // Should contain error information
    assert!(html.contains("404"), "Should contain status code");
    assert!(html.contains("Not Found"), "Should contain status text");
    assert!(
        html.contains("The requested file was not found"),
        "Should contain description"
    );

    // Should contain proper HTML structure
    assert!(
        html.contains("error-container"),
        "Should contain error container"
    );
    assert!(
        html.contains("error-code"),
        "Should contain error code styling"
    );
    assert!(html.contains("back-link"), "Should contain back link");

    // Should reference embedded assets
    assert!(
        html.contains("/_static/error/styles.css"),
        "Should reference error CSS"
    );
    assert!(
        html.contains("/_static/error/script.js"),
        "Should reference error JS"
    );
}
