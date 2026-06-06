//! Collect and validate symbol bindings for instantiation.

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::{SymbolType, TemplateManifest, TemplateSymbol};

pub type SymbolValues = BTreeMap<String, String>;

#[derive(Debug, Clone, Default)]
pub struct SymbolCollectOptions {
    pub interactive: bool,
    pub no_interactive: bool,
    pub primary_name: Option<String>,
    pub bindings: BTreeMap<String, String>,
}

pub fn collect_symbol_values(
    manifest: &TemplateManifest,
    options: &SymbolCollectOptions,
) -> TemplateResult<SymbolValues> {
    let mut values = BTreeMap::new();
    let primary_id = manifest.primary_name_symbol_id().to_string();

    if let Some(name) = &options.primary_name {
        values.insert(primary_id.clone(), name.clone());
    }

    for (id, value) in &options.bindings {
        values.insert(id.clone(), value.clone());
    }

    for (id, symbol) in &manifest.symbols {
        if values.contains_key(id) {
            validate_symbol_value(id, symbol, values.get(id).unwrap())?;
            continue;
        }

        if let Some(default) = &symbol.default_value {
            values.insert(id.clone(), default.clone());
            validate_symbol_value(id, symbol, default)?;
            continue;
        }

        let needs_prompt = symbol.is_required
            || (manifest.prefer_interactive && options.interactive && !options.no_interactive);

        if needs_prompt {
            if options.no_interactive || !options.interactive {
                return Err(TemplateError::RequiredSymbol {
                    symbol_id: id.clone(),
                });
            }
            let value = prompt_symbol(id, symbol)?;
            validate_symbol_value(id, symbol, &value)?;
            values.insert(id.clone(), value);
        }
    }

    Ok(values)
}

fn validate_symbol_value(id: &str, symbol: &TemplateSymbol, value: &str) -> TemplateResult<()> {
    match symbol.symbol_type {
        SymbolType::Choice => {
            if let Some(choices) = &symbol.choices
                && !choices.iter().any(|c| c == value)
            {
                return Err(TemplateError::InvalidManifest(format!(
                    "symbol `{id}` value `{value}` is not in choices"
                )));
            }
        }
        SymbolType::Bool => {
            if value != "true" && value != "false" {
                return Err(TemplateError::InvalidManifest(format!(
                    "symbol `{id}` must be true or false"
                )));
            }
        }
        SymbolType::Integer => {
            if value.parse::<i64>().is_err() {
                return Err(TemplateError::InvalidManifest(format!(
                    "symbol `{id}` must be an integer"
                )));
            }
        }
        SymbolType::String => {}
    }
    Ok(())
}

fn prompt_symbol(id: &str, symbol: &TemplateSymbol) -> TemplateResult<String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let prompt = symbol.description.as_deref().unwrap_or(id);
    match symbol.symbol_type {
        SymbolType::Choice => {
            let choices = symbol.choices.as_deref().unwrap_or(&[]);
            writeln!(stdout, "{prompt} [{id}]")?;
            for (i, choice) in choices.iter().enumerate() {
                writeln!(stdout, "  {}. {}", i + 1, choice)?;
            }
            write!(stdout, "> ")?;
            stdout.flush()?;
            let mut line = String::new();
            stdin.read_line(&mut line)?;
            let line = line.trim();
            if let Ok(idx) = line.parse::<usize>()
                && idx >= 1
                && idx <= choices.len()
            {
                return Ok(choices[idx - 1].clone());
            }
            if choices.iter().any(|c| c == line) {
                return Ok(line.to_string());
            }
            Err(TemplateError::InvalidManifest(format!(
                "invalid choice for symbol `{id}`"
            )))
        }
        SymbolType::Bool => {
            write!(stdout, "{prompt} [{id}] (true/false): ")?;
            stdout.flush()?;
            let mut line = String::new();
            stdin.read_line(&mut line)?;
            let line = line.trim();
            if line == "true" || line == "false" {
                Ok(line.to_string())
            } else {
                Err(TemplateError::InvalidManifest(format!(
                    "symbol `{id}` requires true or false"
                )))
            }
        }
        _ => {
            write!(stdout, "{prompt} [{id}]: ")?;
            stdout.flush()?;
            let mut line = String::new();
            stdin.read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() && symbol.is_required {
                return Err(TemplateError::RequiredSymbol {
                    symbol_id: id.to_string(),
                });
            }
            Ok(line.to_string())
        }
    }
}

pub fn parse_symbol_flag(flag: &str) -> TemplateResult<(String, String)> {
    let (id, value) = flag
        .split_once('=')
        .ok_or_else(|| TemplateError::InvalidManifest(format!("invalid --symbol `{flag}`")))?;
    if id.is_empty() {
        return Err(TemplateError::InvalidManifest(format!(
            "invalid --symbol `{flag}`"
        )));
    }
    Ok((id.to_string(), value.to_string()))
}

pub fn stdin_is_interactive() -> bool {
    io::stdin().is_terminal()
}
