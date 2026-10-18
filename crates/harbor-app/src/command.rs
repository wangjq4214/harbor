//! Typed user-invokable commands and startup keybinding resolution.

use std::collections::{HashMap, HashSet};

use harbor_config::{RawKeybinding, RawKeybindings};
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
    OpenCommandPalette,
    /// Internal close intent tied to one editor lifetime.
    CloseCommandPalette(u64),
    Tab(TabCommand),
    Copy,
    CopyOrInterrupt,
    Paste,
    PageUp,
    PageDown,
    ScrollToTop,
    ScrollToBottom,
    SearchOpen,
    SearchClose,
    SearchNext,
    SearchPrevious,
    /// Internal coalesced query edit, absent from the configurable registry.
    SearchChanged,
    ReloadConfiguration,
    /// Internal presentation intent, deliberately absent from the command registry.
    DismissNotification(u64),
}

/// Invocation origin is independent of shortcut consumption and focus disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSource {
    Direct,
    Palette {
        generation: u64,
        target: Option<TabId>,
    },
}
/// A typed command plus source-specific focus disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppCommandRequest {
    pub command: AppCommand,
    pub focus: TabFocusPolicy,
    pub source: CommandSource,
}

impl AppCommandRequest {
    pub const fn shortcut(command: AppCommand) -> Self {
        Self {
            command,
            focus: TabFocusPolicy::Terminal,
            source: CommandSource::Direct,
        }
    }

    pub const fn rail(command: TabCommand) -> Self {
        Self {
            command: AppCommand::Tab(command),
            focus: TabFocusPolicy::PreserveRail,
            source: CommandSource::Direct,
        }
    }

    pub const fn close_rail(id: TabId) -> Self {
        Self::rail(TabCommand::Close(id))
    }
}

/// Execution evidence, never inferred from FIFO insertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandCompletion {
    Applied,
    Unchanged,
    PendingConfirmation,
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
            "app.command-palette",
            "Open command palette",
            AppCommand::OpenCommandPalette,
            chord(Key::Character('p'), true, true),
            false,
        ),
        descriptor(
            "app.reload-configuration",
            "Reload configuration",
            AppCommand::ReloadConfiguration,
            chord(Key::Character('r'), true, true),
            true,
        ),
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
    entries.extend([
        descriptor(
            "terminal.search",
            "Search current session",
            AppCommand::SearchOpen,
            chord(Key::Character('f'), true, true),
            true,
        ),
        descriptor(
            "terminal.search-next",
            "Next search match",
            AppCommand::SearchNext,
            chord(Key::F3, false, false),
            true,
        ),
        descriptor(
            "terminal.search-previous",
            "Previous search match",
            AppCommand::SearchPrevious,
            chord(Key::F3, false, true),
            true,
        ),
        CommandDescriptor {
            id: "terminal.search-close",
            label: "Close search",
            command: AppCommand::SearchClose,
            default_chords: Vec::new(),
            palette_visible: true,
        },
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

/// Resolves a live candidate without exposing startup fallback as acceptance.
pub fn validate_keybindings(raw: &RawKeybindings) -> Result<ResolvedKeybindings, Vec<String>> {
    let resolution = resolve_keybindings(raw);
    if resolution.used_defaults {
        Err(resolution.diagnostics)
    } else {
        Ok(resolution.keybindings)
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
        if let Some(raw_bindings) = configured {
            for (index, raw_binding) in raw_bindings.iter().enumerate() {
                match parse_keybinding(raw_binding) {
                    Ok(chord) => {
                        if !local.insert(chord) {
                            diagnostics.push(format!(
                                "keybindings.{}.bindings[{index}] repeats a binding",
                                entry.id
                            ));
                            continue;
                        }
                        if let Some(owner) = owners.insert(chord, entry.id) {
                            diagnostics.push(format!(
                                "keybindings.{}.bindings[{index}] conflicts with `{owner}`",
                                entry.id
                            ));
                        }
                        bindings.push((chord, entry.command));
                    }
                    Err(reason) => diagnostics.push(format!(
                        "keybindings.{}.bindings[{index}] {reason}",
                        entry.id
                    )),
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

/// Parses one documented structured single-stroke binding with an exact modifier set.
fn parse_keybinding(raw: &RawKeybinding) -> Result<KeyChord, &'static str> {
    let mut modifiers = Modifiers::default();
    for raw_modifier in &raw.modifiers {
        let modifier = raw_modifier.trim().to_ascii_lowercase();
        match modifier.as_str() {
            "ctrl" | "control" if !modifiers.ctrl => modifiers.ctrl = true,
            "shift" if !modifiers.shift => modifiers.shift = true,
            "alt" if !modifiers.alt => modifiers.alt = true,
            "meta" | "super" | "win" if !modifiers.meta => modifiers.meta = true,
            "ctrl" | "control" | "shift" | "alt" | "meta" | "super" | "win" => {
                return Err("contains a repeated modifier");
            }
            _ => return Err("contains an unsupported modifier"),
        }
    }

    let key = raw.key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("key must not be empty");
    }
    Ok(KeyChord::new(parse_key(&key)?, modifiers))
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

/// Whether a command is discoverable; internal and hidden intents are never admitted.
pub fn palette_command_supported(command: AppCommand) -> bool {
    command_registry()
        .iter()
        .any(|entry| entry.palette_visible && entry.command == command)
}

/// Current execution facts, separate from source-specific shortcut consumption.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandContext {
    pub alternate_screen: bool,
    pub search_open: bool,
}

/// Shared availability facts used by palette presentation and authoritative Host revalidation.
/// Shortcut routing remains source-specific (including terminal pass-through).
pub fn command_availability(
    command: AppCommand,
    snapshots: &[crate::tab_manager::TabSnapshot],
    context: CommandContext,
) -> Result<(), &'static str> {
    let active = snapshots.iter().find(|tab| tab.active);
    match command {
        AppCommand::ReloadConfiguration
        | AppCommand::OpenCommandPalette
        | AppCommand::Tab(TabCommand::New) => Ok(()),
        AppCommand::Tab(TabCommand::Numeric(index)) => {
            let tab = snapshots
                .get(index.to_zero_based())
                .ok_or("Tab does not exist")?;
            if tab.active {
                Err("Tab is already active")
            } else {
                Ok(())
            }
        }
        AppCommand::Tab(TabCommand::Activate(id) | TabCommand::Close(id)) => snapshots
            .iter()
            .any(|tab| tab.id == id)
            .then_some(())
            .ok_or("Tab no longer exists"),
        AppCommand::Tab(TabCommand::Next | TabCommand::Previous) if snapshots.len() < 2 => {
            Err("No other tab")
        }
        AppCommand::Tab(_) => active.map(|_| ()).ok_or("No active terminal"),
        AppCommand::PageUp
        | AppCommand::PageDown
        | AppCommand::ScrollToTop
        | AppCommand::ScrollToBottom => {
            active.ok_or("No active terminal")?;
            if context.alternate_screen {
                Err("Scrolling is unavailable in the alternate screen")
            } else {
                Ok(())
            }
        }
        AppCommand::SearchClose | AppCommand::SearchNext | AppCommand::SearchPrevious => {
            active.ok_or("No active terminal")?;
            context.search_open.then_some(()).ok_or("Search is closed")
        }
        AppCommand::Copy | AppCommand::Paste | AppCommand::SearchOpen => {
            active.map(|_| ()).ok_or("No active terminal")
        }
        _ => Err("Command is not available in the palette"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn binding(modifiers: &[&str], key: &str) -> RawKeybinding {
        RawKeybinding {
            modifiers: modifiers
                .iter()
                .map(|modifier| (*modifier).to_owned())
                .collect(),
            key: key.to_owned(),
        }
    }

    fn raw(entries: Vec<(&str, Vec<RawKeybinding>)>) -> RawKeybindings {
        RawKeybindings {
            entries: entries
                .into_iter()
                .map(|(id, bindings)| (id.to_owned(), bindings))
                .collect::<BTreeMap<_, _>>(),
            valid: true,
        }
    }

    #[test]
    fn palette_opener_uses_atomic_override_multiple_unbind_and_conflict_policy() {
        let command = AppCommand::OpenCommandPalette;
        let default = chord(Key::Character('p'), true, true);
        let defaults = resolve_keybindings(&RawKeybindings::default()).keybindings;
        assert!(
            defaults
                .bindings()
                .any(|binding| binding == (default, command))
        );
        let replaced = validate_keybindings(&raw(vec![(
            "app.command-palette",
            vec![binding(&["ctrl"], "p"), binding(&["alt"], "p")],
        )]))
        .unwrap();
        assert_eq!(
            replaced
                .bindings()
                .filter(|(_, bound)| *bound == command)
                .count(),
            2
        );
        assert!(
            !replaced
                .bindings()
                .any(|binding| binding == (default, command))
        );
        let unbound = validate_keybindings(&raw(vec![("app.command-palette", vec![])])).unwrap();
        assert!(!unbound.bindings().any(|(_, bound)| bound == command));
        let conflict = raw(vec![("app.command-palette", vec![binding(&["ctrl"], "t")])]);
        assert!(validate_keybindings(&conflict).is_err());
        let fallback = resolve_keybindings(&conflict);
        assert!(fallback.used_defaults);
        assert_eq!(fallback.keybindings, defaults);
    }

    #[test]
    fn reload_command_is_configurable_and_live_semantics_never_return_fallback() {
        let defaults = validate_keybindings(&RawKeybindings::default()).unwrap();
        assert!(
            defaults
                .bindings()
                .any(|(key, command)| command == AppCommand::ReloadConfiguration
                    && key == chord(Key::Character('r'), true, true))
        );
        let unbound =
            validate_keybindings(&raw(vec![("app.reload-configuration", vec![])])).unwrap();
        assert!(
            !unbound
                .bindings()
                .any(|(_, command)| command == AppCommand::ReloadConfiguration)
        );
        for entry in [
            ("unknown.command", vec![]),
            ("app.reload-configuration", vec![binding(&["ctrl"], "t")]),
            (
                "app.reload-configuration",
                vec![binding(&["ctrl", "ctrl"], "r")],
            ),
            (
                "app.reload-configuration",
                vec![binding(&[], "unsupported")],
            ),
            (
                "app.reload-configuration",
                vec![binding(&[], "r"), binding(&[], "r")],
            ),
        ] {
            assert!(validate_keybindings(&raw(vec![entry])).is_err());
        }
    }
    #[test]
    fn registry_ids_and_defaults_are_unique_and_complete() {
        let registry = command_registry();
        assert_eq!(registry.len(), 26);
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
        let resolved = resolve_keybindings(&raw(vec![
            ("app.new-tab", vec![binding(&["ctrl"], "n")]),
            (
                "terminal.paste",
                vec![binding(&["ctrl"], "v"), binding(&["shift"], "insert")],
            ),
            ("terminal.page-up", vec![]),
        ]));
        assert!(!resolved.used_defaults);
        let bindings: Vec<_> = resolved.keybindings.bindings().collect();
        assert!(bindings.contains(&(
            chord(Key::Character('n'), true, false),
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
    fn structured_fields_resolve_independently_to_an_exact_chord() {
        let chord = parse_keybinding(&binding(&["control", "SHIFT", "alt", "win"], "TAB")).unwrap();
        assert_eq!(
            chord,
            KeyChord::new(
                Key::Tab,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: true,
                    meta: true,
                }
            )
        );
    }

    #[test]
    fn semantic_errors_restore_the_complete_default_table() {
        let cases = [
            raw(vec![("unknown", vec![binding(&["ctrl"], "x")])]),
            raw(vec![("app.new-tab", vec![binding(&["hyper"], "n")])]),
            raw(vec![("app.new-tab", vec![binding(&["ctrl"], "f13")])]),
            raw(vec![("app.new-tab", vec![binding(&["ctrl"], "")])]),
            raw(vec![(
                "app.new-tab",
                vec![binding(&["ctrl", "control"], "n")],
            )]),
            raw(vec![(
                "app.new-tab",
                vec![binding(&["ctrl"], "n"), binding(&["ctrl"], "n")],
            )]),
            raw(vec![("app.new-tab", vec![binding(&["ctrl"], "w")])]),
        ];

        for overrides in cases {
            let resolved = resolve_keybindings(&overrides);
            assert!(resolved.used_defaults);
            assert_eq!(resolved.keybindings, default_bindings(&command_registry()));
            assert!(!resolved.diagnostics.is_empty());
        }
    }

    #[test]
    fn diagnostics_identify_command_and_binding_index() {
        let resolved = resolve_keybindings(&raw(vec![(
            /* duplicate second record */
            "app.new-tab",
            vec![binding(&["ctrl"], "n"), binding(&["ctrl"], "n")],
        )]));
        assert!(
            resolved
                .diagnostics
                .iter()
                .any(|message| message.contains("keybindings.app.new-tab.bindings[1]"))
        );
    }
}
