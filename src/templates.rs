//! Template loading and rendering system for modular HTML

use crate::error::AppError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Template loader and renderer for modular HTML templates
pub struct TemplateEngine {
    templates: HashMap<String, String>,
}

impl TemplateEngine {
    /// Create a new template engine
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Load template from file system
    pub fn load_template(&mut self, name: &str, path: &str) -> Result<(), AppError> {
        let content = fs::read_to_string(path).map_err(|e| {
            AppError::InternalServerError(format!("Failed to load template {}: {}", name, e))
        })?;
        self.templates.insert(name.to_string(), content);
        Ok(())
    }

    /// Load all templates from the templates directory
    pub fn load_all_templates(&mut self) -> Result<(), AppError> {
        // Load directory templates
        if Path::new("templates/directory").exists() {
            self.load_template("directory_index", "templates/directory/index.html")?;
        }

        // Load error templates
        if Path::new("templates/error").exists() {
            self.load_template("error_page", "templates/error/page.html")?;
        }

        Ok(())
    }

    /// Render a template with variables
    pub fn render(&self, template_name: &str, variables: &HashMap<String, String>) -> Result<String, AppError> {
        let template = self.templates.get(template_name)
            .ok_or_else(|| AppError::InternalServerError(format!("Template '{}' not found", template_name)))?;

        let mut rendered = template.clone();
        
        // Replace variables in the format {{VARIABLE_NAME}}
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            rendered = rendered.replace(&placeholder, value);
        }

        Ok(rendered)
    }

    /// Generate directory listing HTML using template
    pub fn render_directory_listing(
        &self,
        path: &str,
        entries: &[(String, String, String)], // (name, size, date)
        entry_count: usize,
    ) -> Result<String, AppError> {
        let mut variables = HashMap::new();
        variables.insert("PATH".to_string(), path.to_string());
        variables.insert("ENTRY_COUNT".to_string(), entry_count.to_string());

        // Generate entries HTML
        let mut entries_html = String::new();
        
        // Add parent directory link if not at root
        if path != "/" && !path.is_empty() {
            entries_html.push_str(
                r#"<tr>
                    <td>
                        <a href="../" class="file-link">
                            <span class="file-type directory"></span>
                            <span class="name">..</span>
                        </a>
                    </td>
                    <td class="size">-</td>
                    <td class="date">-</td>
                </tr>"#
            );
        }

        // Add file/directory entries
        for (name, size, date) in entries {
            let is_directory = name.ends_with('/');
            let type_class = if is_directory { "directory" } else { "file" };
            let display_name = if is_directory {
                name.trim_end_matches('/')
            } else {
                name
            };

            entries_html.push_str(&format!(
                r#"<tr>
                    <td>
                        <a href="{}" class="file-link">
                            <span class="file-type {}"></span>
                            <span class="name">{}</span>
                        </a>
                    </td>
                    <td class="size">{}</td>
                    <td class="date">{}</td>
                </tr>"#,
                percent_encode(name),
                type_class,
                html_escape(display_name),
                size,
                date
            ));
        }

        variables.insert("ENTRIES".to_string(), entries_html);
        
        self.render("directory_index", &variables)
    }

    /// Generate error page HTML using template
    pub fn render_error_page(
        &self,
        status_code: u16,
        status_text: &str,
        description: &str,
    ) -> Result<String, AppError> {
        let mut variables = HashMap::new();
        variables.insert("STATUS_CODE".to_string(), status_code.to_string());
        variables.insert("STATUS_TEXT".to_string(), status_text.to_string());
        variables.insert("DESCRIPTION".to_string(), description.to_string());
        
        self.render("error_page", &variables)
    }
}

/// Simple percent encoding for URLs
fn percent_encode(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '"' => "%22".to_string(),
            '#' => "%23".to_string(),
            '%' => "%25".to_string(),
            '<' => "%3C".to_string(),
            '>' => "%3E".to_string(),
            '?' => "%3F".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

/// Simple HTML entity escaping
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Get human-friendly error descriptions
pub fn get_error_description(status_code: u16) -> &'static str {
    match status_code {
        400 => "The request could not be understood due to malformed syntax.",
        401 => "Authentication is required to access this resource.",
        403 => "Access to this resource is forbidden.",
        404 => "The requested file or directory could not be found.",
        405 => "The request method is not allowed for this resource.",
        500 => "An internal server error occurred while processing your request.",
        _ => "An unexpected error occurred while processing your request."
    }
}