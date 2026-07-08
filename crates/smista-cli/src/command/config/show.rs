//! executor for `smista config show`.

use std::fmt::Write as _;
use std::path::Path;

use crate::args::ConfigScope;

const REDACTED: &str = "[redacted]";

/// Prints the effective configuration or one redacted configuration layer.
///
/// Without a scope, the command prints the effective merged CLI configuration.
/// With a scope, it validates that layer and prints parsed, redacted sections.
///
/// # Errors
///
/// Returns an error when a path cannot be resolved, a selected file cannot be
/// read, or a configuration document cannot be parsed or rendered safely.
pub fn run(path: Option<&Path>, scope: Option<ConfigScope>) -> anyhow::Result<()> {
    let output = match scope {
        Some(scope) => render_config_layer(path, scope)?,
        None => render_effective_config(path)?,
    };
    println!("{output}");

    Ok(())
}

fn render_effective_config(path: Option<&Path>) -> anyhow::Result<String> {
    if let Some(path) = path {
        let config = crate::config::load_and_validate_at(path)?;
        return render_cli_config(&config);
    }

    let cwd = std::env::current_dir()?;
    let config = crate::config::load_effective(&cwd)?;
    render_cli_config(&config)
}

fn render_config_layer(path: Option<&Path>, scope: ConfigScope) -> anyhow::Result<String> {
    let path = super::resolve_config_path_by_scope(path, scope)?;
    tracing::debug!("resolved config path: {path}", path = path.display());

    if matches!(scope, ConfigScope::Router) {
        return render_router_config(&path);
    }

    let config = load_existing_cli_config(&path)?;
    render_cli_config(&config)
}

fn load_existing_cli_config(path: &Path) -> anyhow::Result<crate::config::Config> {
    let contents = std::fs::read_to_string(path).map_err(|source| {
        anyhow::anyhow!(
            "failed to read config file {path}: {source}",
            path = path.display()
        )
    })?;
    let config = toml::from_str::<crate::config::Config>(&contents).map_err(|source| {
        anyhow::anyhow!(
            "invalid config file {path}: {source}",
            path = path.display()
        )
    })?;
    let report = crate::config::validate::validate_layers(&config, &[]);
    if !report.is_ok() {
        anyhow::bail!(
            "CLI configuration validation failed: {report}",
            report = report.to_human()
        );
    }
    Ok(config)
}

fn render_router_config(path: &Path) -> anyhow::Result<String> {
    let config = load_existing_router_config(path)?;
    let validation_report = smista_router::config::validate::validate(&config);
    if !validation_report.is_ok() {
        anyhow::bail!(
            "router configuration validation failed: {report}",
            report = validation_report.to_human()
        );
    }

    let mut output = String::new();
    writeln!(output, "Router configuration")?;
    writeln!(output)?;
    writeln!(output, "HTTP")?;
    push_line(&mut output, 2, "host", &config.host)?;
    push_line(&mut output, 2, "port", config.port)?;
    writeln!(output)?;
    writeln!(output, "Storage")?;
    push_line(
        &mut output,
        2,
        "engine",
        format!("{:?}", config.storage.engine).to_ascii_lowercase(),
    )?;
    push_line(
        &mut output,
        2,
        "mode",
        format!("{:?}", config.storage.mode).to_ascii_lowercase(),
    )?;
    push_option_line(
        &mut output,
        2,
        "path",
        config.storage.path.as_ref().map(|path| path.display()),
    )?;
    push_option_line(&mut output, 2, "url", config.storage.url.as_ref())?;
    push_option_line(
        &mut output,
        2,
        "username",
        config.storage.username.as_deref(),
    )?;
    push_line(
        &mut output,
        2,
        "password",
        redacted_presence(config.storage.password.is_some()),
    )?;
    push_line(&mut output, 2, "namespace", &config.storage.namespace)?;
    push_line(&mut output, 2, "database", &config.storage.database)?;
    writeln!(output)?;
    writeln!(output, "Auth")?;
    push_line(
        &mut output,
        2,
        "token_ttl_seconds",
        config.auth.token_ttl_seconds,
    )?;
    push_line(
        &mut output,
        2,
        "api_key_version",
        &config.auth.api_key_version,
    )?;
    push_line(
        &mut output,
        2,
        "local_bootstrap_enabled",
        config.auth.local_bootstrap_enabled,
    )?;
    writeln!(output)?;
    writeln!(output, "Limits")?;
    push_line(
        &mut output,
        2,
        "max_request_body_bytes",
        config.limits.max_request_body_bytes,
    )?;
    push_line(
        &mut output,
        2,
        "max_context_bytes",
        config.limits.max_context_bytes,
    )?;
    push_line(
        &mut output,
        2,
        "max_concurrent_requests",
        config.limits.max_concurrent_requests,
    )?;
    push_line(
        &mut output,
        2,
        "request_timeout_ms",
        config.limits.request_timeout_ms,
    )?;
    push_line(
        &mut output,
        2,
        "provider_timeout_ms",
        config.limits.provider_timeout_ms,
    )?;
    push_line(
        &mut output,
        2,
        "tool_timeout_ms",
        config.limits.tool_timeout_ms,
    )?;
    writeln!(output)?;
    writeln!(output, "Rate limit")?;
    push_line(&mut output, 2, "enabled", config.rate_limit.enabled)?;
    push_line(&mut output, 2, "period_ms", config.rate_limit.period_ms)?;
    push_line(&mut output, 2, "burst_size", config.rate_limit.burst_size)?;
    push_line(
        &mut output,
        2,
        "trust_proxy_headers",
        config.rate_limit.trust_proxy_headers,
    )?;
    writeln!(output)?;
    writeln!(output, "Logging")?;
    push_line(&mut output, 2, "level", &config.logging.level)?;
    push_line(&mut output, 2, "format", &config.logging.format)?;
    push_line(
        &mut output,
        2,
        "redact_secrets",
        config.logging.redact_secrets,
    )?;
    writeln!(output)?;
    writeln!(output, "OpenTelemetry")?;
    push_line(&mut output, 2, "enabled", config.opentelemetry.enabled)?;
    push_line(&mut output, 2, "endpoint", &config.opentelemetry.endpoint)?;
    push_line(&mut output, 2, "protocol", config.opentelemetry.protocol)?;
    push_line(
        &mut output,
        2,
        "service_name",
        &config.opentelemetry.service_name,
    )?;
    push_line(
        &mut output,
        2,
        "sample_ratio",
        config.opentelemetry.sample_ratio,
    )?;
    writeln!(output)?;
    writeln!(output, "CORS")?;
    push_line(&mut output, 2, "enabled", config.cors.enabled)?;
    push_list(
        &mut output,
        2,
        "allowed_origins",
        &config.cors.allowed_origins,
    )?;
    writeln!(output)?;
    writeln!(output, "Retention")?;
    push_line(
        &mut output,
        2,
        "trace_retention_days",
        config.retention.trace_retention_days,
    )?;
    push_line(
        &mut output,
        2,
        "session_retention_days",
        config.retention.session_retention_days,
    )?;
    push_line(
        &mut output,
        2,
        "archived_session_retention_days",
        config.retention.archived_session_retention_days,
    )?;
    push_line(
        &mut output,
        2,
        "cleanup_interval_seconds",
        config.retention.cleanup_interval_seconds,
    )?;
    writeln!(output)?;
    writeln!(output, "Ollama")?;
    push_line(&mut output, 2, "enabled", config.ollama.enabled)?;
    push_line(&mut output, 2, "base_url", &config.ollama.base_url)?;
    writeln!(output)?;
    render_router_providers(&config, &mut output)?;

    Ok(output)
}

fn load_existing_router_config(path: &Path) -> anyhow::Result<smista_router::config::RouterConfig> {
    let contents = std::fs::read_to_string(path).map_err(|source| {
        anyhow::anyhow!(
            "failed to read router config {path}: {source}",
            path = path.display()
        )
    })?;
    smista_router::config::parse(&contents, &path.display().to_string()).map_err(Into::into)
}

fn render_router_providers(
    config: &smista_router::config::RouterConfig,
    output: &mut String,
) -> std::fmt::Result {
    writeln!(output, "Providers")?;
    if config.providers.is_empty() {
        writeln!(output, "  none")?;
        return Ok(());
    }

    for (provider, provider_config) in &config.providers {
        writeln!(output, "  {provider}")?;
        push_option_line(output, 4, "base_url", provider_config.base_url.as_deref())?;
        push_line(output, 4, "local", provider_config.local)?;
        push_option_line(
            output,
            4,
            "display_name",
            provider_config.display_name.as_deref(),
        )?;
        writeln!(output, "    models:")?;
        if provider_config.models.is_empty() {
            writeln!(output, "      none")?;
            continue;
        }

        for model in &provider_config.models {
            writeln!(output, "      - {name}", name = model.name)?;
            push_option_line(output, 8, "display_name", model.display_name.as_deref())?;
            push_line(output, 8, "auth", display_model_auth(&model.auth))?;
            push_line(
                output,
                8,
                "capabilities",
                display_capabilities(&model.capabilities),
            )?;
            push_line(output, 8, "max_context_tokens", model.max_context_tokens)?;
            push_option_line(
                output,
                8,
                "max_output_tokens",
                model.max_output_tokens.as_ref(),
            )?;
            push_option_line(
                output,
                8,
                "input_cost_per_million_tokens",
                model.input_cost_per_million_tokens.as_ref(),
            )?;
            push_option_line(
                output,
                8,
                "output_cost_per_million_tokens",
                model.output_cost_per_million_tokens.as_ref(),
            )?;
        }
    }

    Ok(())
}

fn render_cli_config(config: &crate::config::Config) -> anyhow::Result<String> {
    let mut output = String::new();

    writeln!(output, "CLI configuration")?;
    writeln!(output)?;
    render_providers(config, &mut output)?;
    writeln!(output)?;
    render_routing(config, &mut output)?;
    writeln!(output)?;
    render_classification(config, &mut output)?;
    writeln!(output)?;
    render_tools("Tools", &config.tools, &mut output)?;
    writeln!(output)?;
    render_privacy(config, &mut output)?;
    writeln!(output)?;
    render_router_client(config, &mut output)?;
    writeln!(output)?;
    render_local_preferences(config, &mut output)?;

    Ok(output)
}

fn render_providers(config: &crate::config::Config, output: &mut String) -> std::fmt::Result {
    writeln!(output, "Providers")?;
    if config.providers.is_empty() {
        writeln!(output, "  none")?;
        return Ok(());
    }

    for (provider, provider_config) in &config.providers {
        writeln!(output, "  {provider}")?;
        push_option_line(output, 4, "type", provider_config.kind.as_ref())?;
        push_line(
            output,
            4,
            "api_key",
            redacted_presence(provider_config.api_key.is_some()),
        )?;
    }
    Ok(())
}

fn render_routing(config: &crate::config::Config, output: &mut String) -> std::fmt::Result {
    writeln!(output, "Routing")?;
    match &config.routing.default {
        Some(default) => {
            writeln!(output, "  default: {model}", model = default.model)?;
            push_model_list(output, 4, "fallbacks", &default.fallbacks)?;
        }
        None => {
            writeln!(output, "  default: unset")?;
        }
    }

    writeln!(output, "  rules:")?;
    if config.routing.rules.is_empty() {
        writeln!(output, "    none")?;
        return Ok(());
    }

    for rule in &config.routing.rules {
        writeln!(output, "    - {name}", name = rule.name)?;
        push_line(output, 6, "priority", rule.priority)?;
        push_line(output, 6, "effort", rule.effort)?;
        push_option_line(output, 6, "intent", rule.intent.as_ref())?;
        push_list(output, 6, "paths", &rule.paths)?;
        push_line(output, 6, "local_only", rule.local_only)?;
        push_option_line(
            output,
            6,
            "requires_capabilities",
            rule.requires_capabilities
                .as_ref()
                .map(display_capabilities),
        )?;
        push_line(output, 6, "model", &rule.model)?;
        push_model_list(output, 6, "fallbacks", &rule.fallbacks)?;
        render_tool_permissions(
            "required_permissions",
            6,
            &rule.required_permissions,
            output,
        )?;
        push_option_line(output, 6, "cost_limit", rule.cost_limit.as_ref())?;
    }

    Ok(())
}

fn render_classification(config: &crate::config::Config, output: &mut String) -> std::fmt::Result {
    writeln!(output, "Classification")?;
    push_line(
        output,
        2,
        "default_intent",
        config.classification.default_intent,
    )?;
    writeln!(output, "  rules:")?;
    if config.classification.rules.is_empty() {
        writeln!(output, "    none")?;
        return Ok(());
    }

    for rule in &config.classification.rules {
        writeln!(output, "    - {intent}", intent = rule.intent)?;
        push_line(output, 6, "priority", rule.priority)?;
        push_list(output, 6, "keywords", &rule.keywords)?;
        push_list(
            output,
            6,
            "requires_any_context",
            &rule.requires_any_context,
        )?;
    }
    Ok(())
}

fn render_tools(
    title: &str,
    tools: &smista_sdk::core::policy::ToolsConfig,
    output: &mut String,
) -> std::fmt::Result {
    render_tool_permissions(title, 0, tools, output)
}

fn render_tool_permissions(
    title: &str,
    indent: usize,
    tools: &smista_sdk::core::policy::ToolsConfig,
    output: &mut String,
) -> std::fmt::Result {
    writeln!(output, "{:indent$}{title}", "")?;
    if tools.permissions.is_empty() {
        writeln!(output, "{:width$}none", "", width = indent + 2)?;
        return Ok(());
    }

    for (tool, mode) in &tools.permissions {
        push_line(output, indent + 2, tool, mode)?;
    }
    Ok(())
}

fn render_privacy(config: &crate::config::Config, output: &mut String) -> std::fmt::Result {
    writeln!(output, "Privacy")?;
    push_list(
        output,
        2,
        "restricted_paths",
        &config.privacy.restricted_paths,
    )?;
    writeln!(output, "  remote")?;
    push_line(output, 4, "mode", config.privacy.remote.mode())?;
    push_list(
        output,
        4,
        "blocked_paths",
        &config.privacy.remote.blocked_paths,
    )?;
    writeln!(output, "  local")?;
    push_line(output, 4, "mode", config.privacy.local.mode())?;
    Ok(())
}

fn render_router_client(config: &crate::config::Config, output: &mut String) -> std::fmt::Result {
    writeln!(output, "Router client")?;
    push_option_line(output, 2, "url", config.router.url.as_deref())?;
    push_line(output, 2, "auto_start", config.router.auto_start)?;
    push_option_line(
        output,
        2,
        "connect_timeout_ms",
        config.router.connect_timeout_ms.as_ref(),
    )?;
    push_option_line(
        output,
        2,
        "request_timeout_ms",
        config.router.request_timeout_ms.as_ref(),
    )?;
    let auth_source = config
        .router
        .auth_source
        .map(|source| format!("{source:?}").to_ascii_lowercase());
    push_option_line(output, 2, "auth_source", auth_source.as_deref())?;
    Ok(())
}

fn render_local_preferences(
    config: &crate::config::Config,
    output: &mut String,
) -> std::fmt::Result {
    writeln!(output, "Local preferences")?;
    push_option_line(output, 2, "auto_apply", config.local.auto_apply.as_ref())?;
    push_option_line(output, 2, "local_only", config.local.local_only.as_ref())?;
    push_option_line(output, 2, "no_network", config.local.no_network.as_ref())?;
    push_option_line(
        output,
        2,
        "encrypt_sessions",
        config.local.encrypt_sessions.as_ref(),
    )?;
    Ok(())
}

fn push_line(
    output: &mut String,
    indent: usize,
    key: &str,
    value: impl std::fmt::Display,
) -> std::fmt::Result {
    writeln!(output, "{:indent$}{key}: {value}", "")
}

fn push_option_line(
    output: &mut String,
    indent: usize,
    key: &str,
    value: Option<impl std::fmt::Display>,
) -> std::fmt::Result {
    match value {
        Some(value) => push_line(output, indent, key, value),
        None => push_line(output, indent, key, "unset"),
    }
}

fn push_list(output: &mut String, indent: usize, key: &str, values: &[String]) -> std::fmt::Result {
    let value = if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    };
    push_line(output, indent, key, value)
}

fn push_model_list(
    output: &mut String,
    indent: usize,
    key: &str,
    values: &[smista_sdk::core::model::ModelReference],
) -> std::fmt::Result {
    let value = if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    push_line(output, indent, key, value)
}

fn display_capabilities(capabilities: &smista_sdk::core::model::ModelCapabilities) -> String {
    let capabilities = capabilities.supported();
    if capabilities.is_empty() {
        return "none".to_string();
    }
    capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_model_auth(auth: &smista_sdk::core::model::ModelAuthRequirement) -> String {
    match auth {
        smista_sdk::core::model::ModelAuthRequirement::None => "none".to_string(),
        smista_sdk::core::model::ModelAuthRequirement::ApiKey => "api_key".to_string(),
        smista_sdk::core::model::ModelAuthRequirement::OptionalApiKey => {
            "optional_api_key".to_string()
        }
        smista_sdk::core::model::ModelAuthRequirement::Custom(name) => {
            format!("custom: {name}")
        }
    }
}

fn redacted_presence(present: bool) -> &'static str {
    if present { REDACTED } else { "unset" }
}

#[cfg(test)]
fn redacted_config_document(contents: &str) -> anyhow::Result<String> {
    let mut document = toml::from_str::<toml::Value>(contents)?;
    redact_value(&mut document);
    Ok(toml::to_string_pretty(&document)?)
}

#[cfg(test)]
fn redact_value(value: &mut toml::Value) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                if is_sensitive_key(key) {
                    *value = toml::Value::String(REDACTED.to_string());
                } else {
                    redact_value(value);
                }
            }
        }
        toml::Value::Array(values) => {
            for value in values {
                redact_value(value);
            }
        }
        toml::Value::String(_)
        | toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => {}
    }
}

#[cfg(test)]
fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "api_key" || key == "password" || key == "secret" || key.ends_with("_secret")
}

#[cfg(test)]
mod tests {
    use smista_sdk::core::model::Provider;

    use super::*;

    #[test]
    fn should_redact_api_keys_from_raw_config_documents() {
        let output = redacted_config_document(
            r#"
                [providers.openai]
                api_key = "sk-live-secret"
            "#,
        )
        .expect("failed to render redacted config document");

        assert!(output.contains("api_key"));
        assert!(output.contains("[redacted]"));
        assert!(!output.contains("sk-live-secret"));
    }

    #[test]
    fn should_redact_api_keys_from_effective_cli_config() {
        let mut config = crate::config::Config::default();
        config
            .providers
            .get_mut(&Provider::Ollama)
            .expect("default Ollama provider should exist")
            .api_key = Some("sk-live-secret".to_string());

        let output = render_cli_config(&config).expect("failed to render CLI config");

        assert!(output.contains("api_key"));
        assert!(output.contains("[redacted]"));
        assert!(!output.contains("sk-live-secret"));
    }

    #[test]
    fn should_render_effective_cli_config_as_sections() {
        let config = crate::config::parse(
            r#"
            [providers.openai]
            type = "openai"
            api_key = "${secret:OPENAI_API_KEY}"

            [routing.default]
            model = "openai/gpt-5.5-mini"
            fallbacks = ["ollama/qwen2.5-coder:7b"]

            [[routing.rules]]
            name = "plan remotely"
            priority = 10
            intent = "plan"
            model = "openai/gpt-5.5-thinking"

            [router]
            auto_start = true
            "#,
            "config.toml",
        )
        .expect("failed to parse sample config");

        let output = render_cli_config(&config).expect("failed to render CLI config");

        assert!(output.contains("CLI configuration"));
        assert!(output.contains("Providers"));
        assert!(output.contains("  openai"));
        assert!(output.contains("    type: openai"));
        assert!(output.contains("    api_key: [redacted]"));
        assert!(output.contains("Routing"));
        assert!(output.contains("  default: openai/gpt-5.5-mini"));
        assert!(output.contains("    fallbacks: ollama/qwen2.5-coder:7b"));
        assert!(output.contains("    - plan remotely"));
        assert!(output.contains("      priority: 10"));
        assert!(output.contains("      intent: plan"));
        assert!(output.contains("Router client"));
        assert!(output.contains("  auto_start: true"));
        assert!(!output.contains("[providers.openai]"));
        assert!(!output.contains("${secret:OPENAI_API_KEY}"));
    }

    #[test]
    fn should_redact_nested_sensitive_keys_case_insensitively() {
        let output = redacted_config_document(
            r#"
                [storage]
                Password = "database-secret"
            "#,
        )
        .expect("failed to render redacted config document");

        assert!(output.contains("[redacted]"));
        assert!(!output.contains("database-secret"));
    }

    #[test]
    fn should_show_effective_config_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run(Some(&path), None).expect("effective config should render");
    }

    #[test]
    fn should_show_redacted_layer_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
            [providers.openai]
            api_key = "${secret:OPENAI_API_KEY}"

            [routing.default]
            model = "openai/gpt-5.5-mini"
            "#,
        )
        .expect("failed to write sample CLI config");

        run(Some(&path), Some(ConfigScope::Project)).expect("layer config should render");
    }

    #[test]
    fn should_show_router_config_as_sections_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.toml");
        std::fs::write(&path, "[router]\nport = 7332\n")
            .expect("failed to write sample router config");

        let output = render_router_config(&path).expect("router config should render");

        assert!(output.contains("Router configuration"));
        assert!(output.contains("HTTP"));
        assert!(output.contains("  port: 7332"));
        assert!(output.contains("Storage"));
        assert!(output.contains("Rate limit"));
        assert!(!output.contains("[router]"));
    }

    #[test]
    fn should_validate_layer_before_rendering() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
            [providers.openai]
            type = "openai"
            api_key = "sk-live-secret"

            [routing.default]
            model = "openai/gpt-5.5-mini"
            "#,
        )
        .expect("failed to write invalid CLI config");

        let error = run(Some(&path), Some(ConfigScope::Project)).unwrap_err();

        assert!(error.to_string().contains("validation failed"));
    }

    #[test]
    fn should_report_missing_layer_file_from_tempfile_path() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("missing.toml");

        let error = run(Some(&path), Some(ConfigScope::Project)).unwrap_err();

        assert!(error.to_string().contains("failed to read config file"));
    }
}
