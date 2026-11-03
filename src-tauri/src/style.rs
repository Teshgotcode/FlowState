//! Tone/style handling for dictation post-processing.
//!
//! A "style" is a post-processing prompt preset (Nøytral, Formell, Kort,
//! E-post, Notat, Kode-kommentar) living entirely in local settings — no
//! network. The active style is resolved per dictation: a per-app rule for
//! the focused app wins; once any rule is configured, apps without a rule
//! fall back to the Neutral preset. With no rules at all, the globally
//! selected prompt is used, as before.

use crate::settings::{get_settings, write_settings, AppSettings, LLMPrompt};
use serde::Serialize;
use specta::Type;

/// Id of the built-in Neutral style preset — the fallback whenever per-app
/// rules are configured but the focused app has no rule.
pub const STYLE_NEUTRAL_ID: &str = "style_neutral";

/// The app that currently has keyboard focus, as the system reports it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrontmostAppInfo {
    pub bundle_id: Option<String>,
    pub name: Option<String>,
}

/// Everything the overlay needs to show and switch the active style.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActiveStyleInfo {
    pub style_id: Option<String>,
    pub style_name: Option<String>,
    /// The per-app rule key (bundle id or app name) that matched, if any.
    pub matched_rule_key: Option<String>,
    pub frontmost: Option<FrontmostAppInfo>,
    /// Every configured prompt, so quick menus can list all styles.
    pub styles: Vec<LLMPrompt>,
    /// Whether post-processing will run at all — the overlay only shows the
    /// style chip when the style is actually going to be applied.
    pub post_process_enabled: bool,
}

/// The frontmost app. The dictation overlay never takes focus, so during a
/// dictation this is the app the text is destined for.
#[cfg(target_os = "macos")]
pub fn frontmost_app() -> Option<FrontmostAppInfo> {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    Some(FrontmostAppInfo {
        bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
        name: app.localizedName().map(|s| s.to_string()),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_app() -> Option<FrontmostAppInfo> {
    None
}

/// The style that should clean up the dictation currently being typed.
pub fn resolve_style_id(settings: &AppSettings) -> Option<String> {
    resolve_style_id_for(settings, frontmost_app().as_ref()).0
}

/// Pure half of [`resolve_style_id`], so resolution can be tested without a
/// window server. Returns the style id and the rule key that matched (if any).
pub fn resolve_style_id_for(
    settings: &AppSettings,
    frontmost: Option<&FrontmostAppInfo>,
) -> (Option<String>, Option<String>) {
    if !settings.post_process_app_rules.is_empty() {
        if let Some(app) = frontmost {
            let bundle = app.bundle_id.as_deref().map(|s| s.trim().to_lowercase());
            let name = app.name.as_deref().map(|s| s.trim().to_lowercase());
            // Sorted so resolution is deterministic — HashMap iteration order
            // must never decide which rule wins when several keys match.
            let mut rules: Vec<(String, &String)> = settings
                .post_process_app_rules
                .iter()
                .map(|(k, v)| (k.trim().to_lowercase(), v))
                .filter(|(k, _)| !k.is_empty())
                .collect();
            rules.sort();
            // Bundle-id rules are checked before app-name rules so an exact
            // bundle match always beats a more generic name match.
            for app_key in [bundle.as_deref(), name.as_deref()] {
                for (key, style_id) in &rules {
                    if app_key == Some(key.as_str()) {
                        return (Some(style_id.to_string()), Some(key.clone()));
                    }
                }
            }
        }
        // Unknown (or undetectable) app falls back to the Neutral style for
        // as long as that preset exists; otherwise the global pick stands.
        if settings
            .post_process_prompts
            .iter()
            .any(|p| p.id == STYLE_NEUTRAL_ID)
        {
            return (Some(STYLE_NEUTRAL_ID.to_string()), None);
        }
    }
    (settings.post_process_selected_prompt_id.clone(), None)
}

#[tauri::command]
#[specta::specta]
pub fn get_frontmost_app() -> Option<FrontmostAppInfo> {
    frontmost_app()
}

#[tauri::command]
#[specta::specta]
pub fn get_active_style(app: tauri::AppHandle) -> ActiveStyleInfo {
    let settings = get_settings(&app);
    let frontmost = frontmost_app();
    let (style_id, matched_rule_key) = resolve_style_id_for(&settings, frontmost.as_ref());
    let style_name = style_id
        .as_ref()
        .and_then(|id| settings.post_process_prompts.iter().find(|p| p.id == *id))
        .map(|p| p.name.clone());
    ActiveStyleInfo {
        style_id,
        style_name,
        matched_rule_key,
        frontmost,
        styles: settings.post_process_prompts,
        post_process_enabled: settings.post_process_enabled,
    }
}

#[tauri::command]
#[specta::specta]
pub fn set_app_style_rule(
    app: tauri::AppHandle,
    app_key: String,
    style_id: Option<String>,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let app_key = app_key.trim().to_string();

    match style_id {
        Some(style_id) => {
            if app_key.is_empty() {
                return Err("App cannot be empty".to_string());
            }
            if !settings
                .post_process_prompts
                .iter()
                .any(|p| p.id == style_id)
            {
                return Err(format!("Prompt with id '{}' not found", style_id));
            }
            settings.post_process_app_rules.insert(app_key, style_id);
        }
        None => {
            settings.post_process_app_rules.remove(&app_key);
        }
    }

    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn frontmost(bundle: &str, name: &str) -> FrontmostAppInfo {
        FrontmostAppInfo {
            bundle_id: Some(bundle.to_string()),
            name: Some(name.to_string()),
        }
    }

    #[test]
    fn three_styles_render_three_distinct_prompts_for_fixed_input() {
        let base = AppSettings::default();
        let transcript = "øh kan du eh sende den rapporten til Kari i morgen klokka ti";

        let mut rendered: Vec<String> = Vec::new();
        for style_id in ["style_formal", "style_short", "style_email"] {
            let mut settings = base.clone();
            settings.post_process_selected_prompt_id = Some(style_id.to_string());

            let (resolved, _) = resolve_style_id_for(&settings, None);
            assert_eq!(resolved.as_deref(), Some(style_id));

            let template = settings
                .post_process_prompts
                .iter()
                .find(|p| p.id == style_id)
                .expect("style preset exists")
                .prompt
                .clone();
            let final_prompt = template.replace("${output}", transcript);
            assert!(final_prompt.contains(transcript));
            rendered.push(final_prompt);
        }

        assert_eq!(rendered.len(), 3);
        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                assert_ne!(
                    rendered[i], rendered[j],
                    "styles {i} and {j} must clean the same input differently"
                );
            }
        }
    }

    #[test]
    fn app_rule_wins_for_focused_app_and_unknown_apps_fall_back_to_neutral() {
        let mut settings = AppSettings::default();
        settings.post_process_selected_prompt_id = Some("style_formal".to_string());
        let mut rules = HashMap::new();
        rules.insert(
            "com.tinyspeck.slackmacgap".to_string(),
            "style_short".to_string(),
        );
        // A rule can also be keyed on the app's localized name.
        rules.insert("Slack".to_string(), "style_note".to_string());
        settings.post_process_app_rules = rules;

        // Focused app with a rule → the rule's style (bundle id match).
        let slack = frontmost("com.tinyspeck.slackmacgap", "Slack");
        let (id, key) = resolve_style_id_for(&settings, Some(&slack));
        assert_eq!(id.as_deref(), Some("style_short"));
        assert_eq!(key.as_deref(), Some("com.tinyspeck.slackmacgap"));

        // Rule keyed on the localized app name instead of the bundle id
        // (a different style, so exactly one rule can match).
        let by_name = frontmost("com.example.other", "Slack");
        let (id, key) = resolve_style_id_for(&settings, Some(&by_name));
        assert_eq!(id.as_deref(), Some("style_note"));
        assert_eq!(key.as_deref(), Some("slack"));

        // Unknown app → Neutral, not the globally selected Formal.
        let safari = frontmost("com.apple.Safari", "Safari");
        let (id, key) = resolve_style_id_for(&settings, Some(&safari));
        assert_eq!(id.as_deref(), Some("style_neutral"));
        assert!(key.is_none());

        // Undetectable app → Neutral too.
        let (id, _) = resolve_style_id_for(&settings, None);
        assert_eq!(id.as_deref(), Some("style_neutral"));
    }

    #[test]
    fn without_rules_the_global_selection_is_used() {
        let mut settings = AppSettings::default();
        settings.post_process_selected_prompt_id = Some("style_email".to_string());

        let (id, key) =
            resolve_style_id_for(&settings, Some(&frontmost("com.apple.Safari", "Safari")));
        assert_eq!(id.as_deref(), Some("style_email"));
        assert!(key.is_none());
    }
}
