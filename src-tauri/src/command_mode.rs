//! Command Mode: speak an instruction that edits the selected text.
//!
//! The user selects text in any app, holds the command hotkey, and says e.g.
//! "make this shorter". We read the selection through the macOS Accessibility
//! API (no clipboard involved), send `{ selected_text, instruction, app_name }`
//! plus a fixed system prompt to the configured LLM (the same provider
//! plumbing as post-processing — Ollama by default, never a cloud endpoint
//! unless the user configured one), and paste the result back so it replaces
//! the selection.
//!
//! When the focused app has no selection we fall back to plain dictation with a
//! discreet hint, so the hotkey is never a dead end.

use log::{debug, warn};
use std::sync::Mutex;

use crate::settings::AppSettings;

/// Binding id of the command-mode hotkey. It behaves like the transcribe
/// bindings (same activation modes and cancel handling), so it is routed
/// through the transcription coordinator.
pub const TRANSCRIBE_BINDING_ID: &str = "command";

/// The selected text captured when the command hotkey fired, together with the
/// name of the app it came from. Held between `start` and `stop` of the
/// recording that captures the spoken instruction.
#[derive(Debug, Clone)]
pub struct CommandSelection {
    pub text: String,
    pub app_name: String,
}

/// Selection captured at hotkey-press time. `None` means "no selection —
/// fall back to normal dictation". A `Mutex` so the shortcut thread can store
/// it and the async transcription task can take it.
static PENDING_SELECTION: Mutex<Option<CommandSelection>> = Mutex::new(None);

pub fn store_pending(selection: Option<CommandSelection>) {
    match PENDING_SELECTION.lock() {
        Ok(mut guard) => *guard = selection,
        Err(poisoned) => {
            warn!("Command selection lock poisoned; clearing selection");
            *poisoned.into_inner() = None;
        }
    }
}

pub fn take_pending() -> Option<CommandSelection> {
    match PENDING_SELECTION.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    }
}

pub fn clear_pending() {
    store_pending(None);
}

/// Fixed system prompt for command mode. The selected text is provided in the
/// user message so providers without structured output still see it clearly
/// separated from the instruction.
const COMMAND_SYSTEM_PROMPT: &str = "You are a text editing assistant inside a \
dictation app. The user has selected text in an application and spoken an \
instruction. Apply the instruction to the selected text and return ONLY the \
edited text as plain text. Preserve the input language and formatting style \
unless the instruction says otherwise. Never add explanations, preambles, \
markdown fences, or quotation marks. If the instruction cannot be applied, \
return the selected text unchanged.";

/// Applies the spoken instruction to the selected text via the configured
/// LLM provider. Returns `None` on any failure — the caller then keeps the
/// selection untouched and aborts the command.
pub async fn apply_instruction(
    settings: &AppSettings,
    selection: &CommandSelection,
    instruction: &str,
) -> Option<String> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        debug!("Command instruction is empty; nothing to apply");
        return None;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            warn!("Command Mode has no LLM provider configured");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() {
        warn!(
            "Command Mode skipped: provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let user_content = serde_json::json!({
        "selected_text": selection.text,
        "instruction": instruction,
        "app_name": selection.app_name,
    })
    .to_string();

    debug!(
        "Command Mode: applying instruction ({} chars) to a selection of {} chars in '{}'",
        instruction.len(),
        selection.text.len(),
        selection.app_name
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");

    match crate::llm_client::send_chat_completion_with_schema(
        &provider,
        api_key,
        &model,
        user_content,
        Some(COMMAND_SYSTEM_PROMPT.to_string()),
        None,
        disable_reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content =
                crate::actions::strip_invisible_chars(crate::actions::strip_think_block(&content))
                    .trim()
                    .to_string();
            if content.is_empty() {
                warn!("Command Mode: LLM returned empty content");
                return None;
            }
            Some(content)
        }
        Ok(None) => {
            warn!("Command Mode: LLM response has no content");
            None
        }
        Err(e) => {
            warn!("Command Mode: LLM call failed: {e}");
            None
        }
    }
}

#[cfg(target_os = "macos")]
mod ax {
    //! Minimal bindings for reading the selection of the focused application
    //! through the macOS Accessibility (AX) API.
    //!
    //! The AX entry points live in HIServices, a *sub*-framework of
    //! ApplicationServices that the static linker cannot resolve
    //! (`-framework HIServices` fails; HIServices only exists nested inside
    //! ApplicationServices on disk). The symbols are therefore loaded
    //! dynamically at first use. CoreFoundation helpers, by contrast, are
    //! always linked directly.

    use std::ffi::c_void;
    use std::os::raw::c_char;

    type AXUIElementRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFIndex = std::os::raw::c_int;
    type CFDictionaryRef = *const c_void;

    const K_AX_ERROR_SUCCESS: i32 = 0;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: CFIndex,
            encoding: u32,
        ) -> u8;
    }

    /// The AX symbols, resolved lazily from the HIServices sub-framework.
    struct AxApi {
        copy_system_wide: unsafe extern "C" fn() -> AXUIElementRef,
        copy_attribute_value:
            unsafe extern "C" fn(AXUIElementRef, CFStringRef, *mut CFTypeRef) -> i32,
        is_process_trusted: unsafe extern "C" fn(CFDictionaryRef) -> u8,
        /// Addresses of the exported `CFStringRef` globals; each is a pointer
        /// to the key string, so the value is read with one dereference.
        focused_application_key: CFStringRef,
        focused_ui_element_key: CFStringRef,
        selected_text_key: CFStringRef,
        title_key: CFStringRef,
    }

    /// Where HIServices can be found. The nested path is the canonical one;
    /// the plain name lets `dlopen` fall back to the standard search paths.
    const HSERVICES_PATHS: &[&str] = &[
        "/System/Library/Frameworks/ApplicationServices.framework/Frameworks/HIServices.framework/HIServices",
        "/System/Library/Frameworks/HIServices.framework/HIServices",
    ];

    unsafe fn load_ax_api() -> Option<AxApi> {
        for path in HSERVICES_PATHS {
            let Ok(cpath) = std::ffi::CString::new(*path) else {
                continue;
            };
            let lib = libc::dlopen(cpath.as_ptr(), libc::RTLD_LAZY);
            if lib.is_null() {
                continue;
            }

            macro_rules! sym {
                ($name:literal) => {{
                    let ptr = libc::dlsym(lib, concat!($name, "\0").as_ptr() as *const c_char);
                    if ptr.is_null() {
                        None
                    } else {
                        Some(ptr)
                    }
                }};
            }

            // Every symbol must resolve; a partial image is useless.
            let (Some(copy_system_wide), Some(copy_attribute_value), Some(is_process_trusted)) = (
                sym!("AXUIElementCopySystemWide"),
                sym!("AXUIElementCopyAttributeValue"),
                sym!("AXIsProcessTrustedWithOptions"),
            ) else {
                continue;
            };
            let (Some(focused_application), Some(focused_ui_element), Some(selected_text), Some(title)) = (
                sym!("kAXFocusedApplicationAttributeKey"),
                sym!("kAXFocusedUIElementAttributeKey"),
                sym!("kAXSelectedTextAttributeKey"),
                sym!("kAXTitleAttributeKey"),
            ) else {
                continue;
            };

            // `dlsym` returns `*mut c_void`; the AX entry points are plain C
            // functions, so the pointer cast is sound.
            unsafe fn as_fn0<R>(
                ptr: *mut c_void,
            ) -> unsafe extern "C" fn() -> R {
                std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> R>(ptr)
            }
            unsafe fn as_fn3<A, B, C, R>(
                ptr: *mut c_void,
            ) -> unsafe extern "C" fn(A, B, C) -> R {
                std::mem::transmute::<*mut c_void, unsafe extern "C" fn(A, B, C) -> R>(ptr)
            }
            unsafe fn as_fn1<A, R>(
                ptr: *mut c_void,
            ) -> unsafe extern "C" fn(A) -> R {
                std::mem::transmute::<*mut c_void, unsafe extern "C" fn(A) -> R>(ptr)
            }

            return Some(AxApi {
                copy_system_wide: unsafe { as_fn0(copy_system_wide) },
                copy_attribute_value: unsafe { as_fn3(copy_attribute_value) },
                is_process_trusted: unsafe { as_fn1(is_process_trusted) },
                // The globals hold the CFString themselves.
                focused_application_key: *(focused_application as *const CFStringRef),
                focused_ui_element_key: *(focused_ui_element as *const CFStringRef),
                selected_text_key: *(selected_text as *const CFStringRef),
                title_key: *(title as *const CFStringRef),
            });
        }
        None
    }

    /// `true` when the app has Accessibility permission. Never prompts.
    fn is_trusted(api: &AxApi) -> bool {
        unsafe { (api.is_process_trusted)(std::ptr::null()) != 0 }
    }

    fn copy_attribute(
        api: &AxApi,
        element: AXUIElementRef,
        attribute: CFStringRef,
    ) -> Option<CFTypeRef> {
        if element.is_null() {
            return None;
        }
        let mut value: CFTypeRef = std::ptr::null();
        let status = unsafe { (api.copy_attribute_value)(element, attribute, &mut value) };
        if status == K_AX_ERROR_SUCCESS && !value.is_null() {
            Some(value)
        } else {
            None
        }
    }

    /// Converts a CFString to a Rust `String` (UTF-8).
    fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
        if cf.is_null() {
            return None;
        }
        unsafe {
            // UTF-8 worst case: 4 bytes per UTF-16 code unit + NUL.
            let length = CFStringGetLength(cf);
            let capacity = length.saturating_mul(4).saturating_add(1);
            let mut buffer = vec![0u8; capacity as usize];
            if CFStringGetCString(
                cf,
                buffer.as_mut_ptr() as *mut c_char,
                capacity,
                K_CF_STRING_ENCODING_UTF8,
            ) == 0
            {
                return None;
            }
            let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            buffer.truncate(end);
            String::from_utf8(buffer).ok()
        }
    }

    fn element_attribute_string(
        api: &AxApi,
        element: AXUIElementRef,
        attribute: CFStringRef,
    ) -> Option<String> {
        let value = copy_attribute(api, element, attribute)?;
        let s = cfstring_to_string(value as CFStringRef);
        unsafe { CFRelease(value) };
        s
    }

    /// Reads the selected text of the focused application, plus the app's
    /// (AX) title. Returns `None` for the text when there is no selection or
    /// the AX permission is missing.
    pub fn read_focused_selection() -> (Option<String>, String) {
        let api = match unsafe { load_ax_api() } {
            Some(api) => api,
            None => {
                log::warn!("Command Mode: could not load the Accessibility API");
                return (None, String::new());
            }
        };
        if !is_trusted(&api) {
            log::debug!("Command Mode: missing Accessibility permission");
            return (None, String::new());
        }

        unsafe {
            let system_wide = (api.copy_system_wide)();
            if system_wide.is_null() {
                return (None, String::new());
            }

            let Some(app) = copy_attribute(
                &api,
                system_wide,
                api.focused_application_key,
            ) else {
                CFRelease(system_wide);
                return (None, String::new());
            };

            let app_name =
                element_attribute_string(&api, app, api.title_key).unwrap_or_default();

            // The selected text lives on the focused UI element; fall back to
            // the application element itself (some apps expose it there).
            let focused = copy_attribute(&api, app, api.focused_ui_element_key);
            let selected = match focused {
                Some(element) => {
                    let text =
                        element_attribute_string(&api, element, api.selected_text_key);
                    CFRelease(element);
                    text
                }
                None => element_attribute_string(&api, app, api.selected_text_key),
            };

            CFRelease(app);
            CFRelease(system_wide);
            (selected, app_name)
        }
    }
}

#[cfg(target_os = "macos")]
pub fn capture_selection() -> Option<CommandSelection> {
    let (selected, app_name) = ax::read_focused_selection();
    let text = selected.unwrap_or_default();
    if text.trim().is_empty() {
        debug!("Command Mode: no text selected in '{}'", app_name);
        return None;
    }
    Some(CommandSelection {
        text,
        app_name: if app_name.is_empty() {
            "unknown".to_string()
        } else {
            app_name
        },
    })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_selection() -> Option<CommandSelection> {
    // AX selection reading is macOS-only today; other platforms fall back to
    // plain dictation until an equivalent is wired up.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_selection_round_trip() {
        store_pending(Some(CommandSelection {
            text: "hello".to_string(),
            app_name: "TextEdit".to_string(),
        }));
        let taken = take_pending();
        assert_eq!(taken.as_ref().map(|s| s.text.as_str()), Some("hello"));
        assert!(take_pending().is_none(), "take must clear the slot");
        clear_pending();
    }

    #[test]
    fn empty_selection_is_none() {
        // `capture_selection` needs a GUI, so this only pins the helper's
        // contract on non-macOS: no AX plumbing, no selection.
        #[cfg(not(target_os = "macos"))]
        assert!(capture_selection().is_none());
    }

    #[test]
    fn system_prompt_is_fixed_and_focused() {
        assert!(COMMAND_SYSTEM_PROMPT.contains("ONLY the edited text"));
    }
}
