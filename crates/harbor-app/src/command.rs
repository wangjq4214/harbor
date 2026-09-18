//! Typed user-invokable commands and startup keybinding resolution.

use std::collections::{HashMap, HashSet};

use harbor_config::RawKeybindings;
use harbor_widget::{
    KeyChord,
    input::event::{Key, Modifiers},
};

use crate::{
    tab_manager::{TabId, TabIndex},
    tab_view::{TabCommand, TabFocusPolicy},
};

/// An application operation that may be invoked by UI controls or keybindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppCommand {
    Tab(TabCommand),
    Copy,
    CopyOrInterrupt,
    Paste,
    PageUp,
    PageDown,
    ScrollToTop,
    ScrollToBottom,
}

/// A typed command plus source-specific focus disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppCommandRequest {
    pub command: AppCommand,
    pub focus: TabFocusPolicy,
}

impl AppCommandRequest {
    pub const fn shortcut(command: AppCommand) -> Self {
        Self {
            command,
            focus: TabFocusPolicy::Terminal,
        }
    }

    pub const fn rail(command: TabCommand) -> Self {
        Self {
            command: AppCommand::Tab(command),
            focus: TabFocusPolicy::PreserveRail,
        }
    }

    pub const fn close_rail(id: TabId) -> Self {
        Self::rail(TabCommand::Close(id))
    }
}

/// Discoverable metadata for one configurable command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub command: AppCommand,
    pub default_chords: Vec<KeyChord>,
    pub palette_visible: bool,
}

/// A complete, conflict-free keybinding table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedKeybindings {
    bindings: Vec<(KeyChord, AppCommand)>,
}

impl ResolvedKeybindings {
    pub fn bindings(&self) -> impl Iterator<Item = (KeyChord, AppCommand)> + '_ {
        self.bindings.iter().copied()
    }
}

/// Result of resolving raw startup settings against the command registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeybindingResolution {
    pub keybindings: ResolvedKeybindings,
    pub diagnostics: Vec<String>,
    pub used_defaults: bool,
}

fn modifiers(ctrl: bool, shift: bool) -> Modifiers {
    Modifiers {
        ctrl,
        shift,
        ..Modifiers::default()
    }
}

fn chord(key: Key, ctrl: bool, shift: bool) -> KeyChord {
    KeyChord::new(key, modifiers(ctrl, shift))
}

/// Returns the fixed initial registry in stable presentation order.
pub fn command_registry() -> Vec<CommandDescriptor> {
    let mut entries = vec![
        descriptor(
            "app.new-tab",
            "New tab",
            AppCommand::Tab(TabCommand::New),
            chord(Key::Character('t'), true, false),
            true,
        ),
        descriptor(
            "app.close-active-tab",
            "Close active tab",
            AppCommand::Tab(TabCommand::CloseActive),
            chord(Key::Character('w'), true, false),
            true,
        ),
        descriptor(
            "app.next-tab",
            "Next tab",
            AppCommand::Tab(TabCommand::Next),
            chord(Key::Tab, true, false),
            true,
        ),
        descriptor(
            "app.previous-tab",
            "Previous tab",
            AppCommand::Tab(TabCommand::Previous),
            chord(Key::Tab, true, true),
            true,
        ),
    ];
    for index in 1..=9_u8 {
        let id = match index {
            1 => "app.select-tab-1",
            2 => "app.select-tab-2",
            3 => "app.select-tab-3",
            4 => "app.select-tab-4",
            5 => "app.select-tab-5",
            6 => "app.select-tab-6",
            7 => "app.select-tab-7",
            8 => "app.select-tab-8",
            _ => "app.select-tab-9",
        };
        let label = match index {
            1 => "Select tab 1",
            2 => "Select tab 2",
            3 => "Select tab 3",
            4 => "Select tab 4",
            5 => "Select tab 5",
            6 => "Select tab 6",
            7 => "Select tab 7",
            8 => "Select tab 8",
            _ => "Select tab 9",
        };
        entries.push(descriptor(
            id,
            label,
            AppCommand::Tab(TabCommand::Numeric(TabIndex::from_valid_u8(index))),
            chord(
                Key::Character(char::from_digit(u32::from(index), 10).expect("valid digit")),
                true,
                false,
            ),
            true,
        ));
    }
    entries.extend([
        descriptor(
            "terminal.copy",
            "Copy",
            AppCommand::Copy,
            chord(Key::Character('c'), true, true),
            true,
        ),
        descriptor(
            "terminal.copy-or-interrupt",
            "Copy or interrupt",
            AppCommand::CopyOrInterrupt,
            chord(Key::Character('c'), true, false),
            false,
        ),
        descriptor(
            "terminal.paste",
            "Paste",
            AppCommand::Paste,
            chord(Key::Character('v'), true, false),
            true,
        ),
        descriptor(
            "terminal.page-up",
            "Page up",
            AppCommand::PageUp,
            chord(Key::PageUp, false, false),
            true,
        ),
        descriptor(
            "terminal.page-down",
            "Page down",
            AppCommand::PageDown,
            chord(Key::PageDown, false, false),
            true,
        ),
        descriptor(
            "terminal.scroll-to-top",
            "Scroll to top",
            AppCommand::ScrollToTop,
            chord(Key::Home, false, false),
            true,
        ),
        descriptor(
            "terminal.scroll-to-bottom",
            "Scroll to bottom",
            AppCommand::ScrollToBottom,
            chord(Key::End, false, false),
            true,
        ),
    ]);
    entries
}

fn descriptor(
    id: &'static str,
    label: &'static str,
    command: AppCommand,
    default_chord: KeyChord,
    palette_visible: bool,
) -> CommandDescriptor {
    CommandDescriptor {
        id,
        label,
        command,
        default_chords: vec![default_chord],
        palette_visible,
    }
}

fn default_bindings(registry: &[CommandDescriptor]) -> ResolvedKeybindings {
    ResolvedKeybindings {
        bindings: registry
            .iter()
            .flat_map(|entry| {
                entry
                    .default_chords
                    .iter()
                    .copied()
                    .map(|chord| (chord, entry.command))
            })
            .collect(),
    }
}

/// Resolves all overrides atomically. Any semantic error restores all defaults.
pub fn resolve_keybindings(raw: &RawKeybindings) -> KeybindingResolution {
    let registry = command_registry();
    let defaults = default_bindings(&registry);
    let mut diagnostics = Vec::new();
    if !raw.valid {
        diagnostics.push("keybindings: invalid value shape; using all defaults".to_owned());
        return KeybindingResolution {
            keybindings: defaults,
            diagnostics,
            used_defaults: true,
        };
    }

    let by_id: HashMap<_, _> = registry.iter().map(|entry| (entry.id, entry)).collect();
    for id in raw.entries.keys() {
        if !by_id.contains_key(id.as_str()) {
            diagnostics.push(format!("keybindings: unknown command `{id}`"));
        }
    }

    let mut bindings = Vec::new();
    let mut owners: HashMap<KeyChord, &str> = HashMap::new();
    for entry in &registry {
        let configured = raw.entries.get(entry.id);
        let mut local = HashSet::new();
        if let Some(chords) = configured {
            for text in chords {
                match parse_key_chord(text) {
                    Ok(chord) => {
                        if !local.insert(chord) {
                            diagnostics.push(format!("keybindings.{} repeats `{text}`", entry.id));
                            continue;
                        }
                        if let Some(owner) = owners.insert(chord, entry.id) {
                            diagnostics.push(format!(
                                "keybindings conflict: `{text}` is owned by `{owner}` and `{}`",
                                entry.id
                            ));
                        }
                        bindings.push((chord, entry.command));
                    }
                    Err(reason) => {
                        diagnostics.push(format!("keybindings.{} `{text}` {reason}", entry.id))
                    }
                }
            }
        } else {
            for chord in &entry.default_chords {
                if let Some(owner) = owners.insert(*chord, entry.id) {
                    diagnostics.push(format!(
                        "default keybinding conflict between `{owner}` and `{}`",
                        entry.id
                    ));
                }
                bindings.push((*chord, entry.command));
            }
        }
    }

    if diagnostics.is_empty() {
        KeybindingResolution {
            keybindings: ResolvedKeybindings { bindings },
            diagnostics,
            used_defaults: false,
        }
    } else {
        KeybindingResolution {
            keybindings: defaults,
            diagnostics,
            used_defaults: true,
        }
    }
}

/// Parses one documented single-stroke chord with an exact modifier set.
pub fn parse_key_chord(text: &str) -> Result<KeyChord, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("must not be empty");
    }
    let mut modifiers = Modifiers::default();
    let mut key = None;
    let mut seen = HashSet::new();
    for raw_token in text.split('+') {
        let token = raw_token.trim().to_ascii_lowercase();
        if token.is_empty() || !seen.insert(token.clone()) {
            return Err("contains an empty or repeated token");
        }
        match token.as_str() {
            "ctrl" | "control" if !modifiers.ctrl => modifiers.ctrl = true,
            "shift" if !modifiers.shift => modifiers.shift = true,
            "alt" if !modifiers.alt => modifiers.alt = true,
            "meta" | "super" | "win" if !modifiers.meta => modifiers.meta = true,
            "ctrl" | "control" | "shift" | "alt" | "meta" | "super" | "win" => {
                return Err("contains a repeated modifier");
            }
            _ => {
                if key.is_some() {
                    return Err("must contain exactly one key");
                }
                key = Some(parse_key(&token)?);
            }
        }
    }
    key.map(|key| KeyChord::new(key, modifiers))
        .ok_or("must contain a key")
}

fn parse_key(token: &str) -> Result<Key, &'static str> {
    let named = match token {
        "tab" => Some(Key::Tab),
        "enter" => Some(Key::Enter),
        "space" => Some(Key::Space),
        "escape" | "esc" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "insert" => Some(Key::Insert),
        "delete" => Some(Key::Delete),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" | "page-up" => Some(Key::PageUp),
        "pagedown" | "page-down" => Some(Key::PageDown),
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        _ => None,
    };
    if let Some(key) = named {
        return Ok(key);
    }
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(character), None) if !character.is_control() => Ok(Key::Character(character)),
        _ => Err("contains an unsupported key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn raw(entries: &[(&str, &[&str])]) -> RawKeybindings {
        RawKeybindings {
            entries: entries
                .iter()
                .map(|(id, chords)| {
                    (
                        (*id).to_owned(),
                        chords.iter().map(|chord| (*chord).to_owned()).collect(),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            valid: true,
        }
    }

    #[test]
    fn registry_ids_and_defaults_are_unique_and_complete() {
        let registry = command_registry();
        assert_eq!(registry.len(), 20);
        assert_eq!(
            registry
                .iter()
                .map(|entry| entry.id)
                .collect::<HashSet<_>>()
                .len(),
            registry.len()
        );
        let defaults: Vec<_> = registry
            .iter()
            .flat_map(|entry| entry.default_chords.iter())
            .collect();
        assert_eq!(
            defaults.iter().copied().collect::<HashSet<_>>().len(),
            defaults.len()
        );
        assert!(
            registry
                .iter()
                .find(|entry| entry.id == "terminal.copy")
                .unwrap()
                .palette_visible
        );
        assert!(
            !registry
                .iter()
                .find(|entry| entry.id == "terminal.copy-or-interrupt")
                .unwrap()
                .palette_visible
        );
    }

    #[test]
    fn partial_replacement_multiple_bindings_and_empty_unbind_are_supported() {
        let resolved = resolve_keybindings(&raw(&[
            ("app.new-tab", &["ctrl+n"]),
            ("terminal.paste", &["ctrl+v", "shift+insert"]),
            ("terminal.page-up", &[]),
        ]));
        assert!(!resolved.used_defaults);
        let bindings: Vec<_> = resolved.keybindings.bindings().collect();
        assert!(bindings.contains(&(
            parse_key_chord("ctrl+n").unwrap(),
            AppCommand::Tab(TabCommand::New)
        )));
        assert!(
            !bindings
                .iter()
                .any(|(_, command)| *command == AppCommand::PageUp)
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|(_, command)| *command == AppCommand::Paste)
                .count(),
            2
        );
    }

    #[test]
    fn semantic_errors_restore_the_complete_default_table() {
        for overrides in [
            raw(&[("unknown", &["ctrl+x"])]),
            raw(&[("app.new-tab", &["ctrl++n"])]),
            raw(&[("app.new-tab", &["ctrl+n", "ctrl+n"])]),
            raw(&[("app.new-tab", &["ctrl+w"])]),
        ] {
            let resolved = resolve_keybindings(&overrides);
            assert!(resolved.used_defaults);
            assert_eq!(resolved.keybindings, default_bindings(&command_registry()));
            assert!(!resolved.diagnostics.is_empty());
        }
    }
}
