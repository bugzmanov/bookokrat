use std::collections::HashMap;

use super::action::Action;
use super::context::KeyContext;
use crossterm::event::KeyCode;

use super::notation::{KeyBinding, KeyInput, format_key_binding};

/// Result of looking up a key sequence in the keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupResult {
    /// The key sequence matched a complete binding.
    Found(Action),
    /// The key sequence is a prefix of one or more bindings; more keys needed.
    Prefix,
    /// The key sequence does not match any binding or prefix.
    NoMatch,
}

#[derive(Debug, Clone)]
enum TrieNode {
    Leaf(Action),
    Branch(HashMap<KeyInput, TrieNode>),
    ActionAndChildren {
        action: Action,
        children: HashMap<KeyInput, TrieNode>,
    },
}

/// Keybindings for a single context.
#[derive(Debug, Clone)]
pub struct ContextKeymap {
    root: HashMap<KeyInput, TrieNode>,
}

impl Default for ContextKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextKeymap {
    pub fn new() -> Self {
        Self {
            root: HashMap::new(),
        }
    }

    /// Insert a binding. Overwrites any existing binding for the same key sequence.
    pub fn bind(&mut self, binding: KeyBinding, action: Action) {
        let keys = binding.0;
        assert!(!keys.is_empty(), "cannot bind empty key sequence");
        Self::insert(&mut self.root, &keys, action);
    }

    /// Remove a binding. Returns true if a binding was actually removed.
    pub fn unbind(&mut self, binding: &KeyBinding) -> bool {
        let keys = &binding.0;
        if keys.is_empty() {
            return false;
        }
        Self::remove(&mut self.root, keys)
    }

    /// Look up a key sequence.
    pub fn lookup(&self, keys: &[KeyInput]) -> LookupResult {
        if keys.is_empty() {
            return LookupResult::NoMatch;
        }

        let mut current_map = &self.root;
        for (i, key) in keys.iter().enumerate() {
            match current_map.get(key) {
                Some(TrieNode::Leaf(action)) => {
                    if i == keys.len() - 1 {
                        return LookupResult::Found(action.clone());
                    }
                    // More keys provided but this is a leaf - no match
                    return LookupResult::NoMatch;
                }
                Some(TrieNode::Branch(children)) => {
                    if i == keys.len() - 1 {
                        return LookupResult::Prefix;
                    }
                    current_map = children;
                }
                Some(TrieNode::ActionAndChildren { action, children }) => {
                    if i == keys.len() - 1 {
                        // This key has an action but also children.
                        // Return the action (dispatch immediately).
                        // If the caller wants prefix behavior, they should
                        // check is_prefix() separately.
                        return LookupResult::Found(action.clone());
                    }
                    current_map = children;
                }
                None => return LookupResult::NoMatch,
            }
        }
        LookupResult::NoMatch
    }

    /// Check if the given key sequence is a strict prefix of any binding
    /// (i.e., there are longer bindings that start with these keys).
    pub fn is_prefix(&self, keys: &[KeyInput]) -> bool {
        if keys.is_empty() {
            return !self.root.is_empty();
        }

        let mut current_map = &self.root;
        for (i, key) in keys.iter().enumerate() {
            match current_map.get(key) {
                Some(TrieNode::Leaf(_)) => return false,
                Some(TrieNode::Branch(children)) => {
                    if i == keys.len() - 1 {
                        return true;
                    }
                    current_map = children;
                }
                Some(TrieNode::ActionAndChildren { children, .. }) => {
                    if i == keys.len() - 1 {
                        return !children.is_empty();
                    }
                    current_map = children;
                }
                None => return false,
            }
        }
        false
    }

    /// Get all bindings as (KeyBinding, Action) pairs.
    pub fn all_bindings(&self) -> Vec<(KeyBinding, Action)> {
        let mut result = Vec::new();
        let mut path = Vec::new();
        Self::collect_bindings(&self.root, &mut path, &mut result);
        result
    }

    /// Find the shortest/simplest binding for a given action (reverse lookup).
    /// Prefers single char keys over special keys, shorter sequences over longer.
    pub fn binding_for_action(&self, action: &Action) -> Option<KeyBinding> {
        let mut matches: Vec<KeyBinding> = self
            .all_bindings()
            .into_iter()
            .filter(|(_, a)| a == action)
            .map(|(binding, _)| binding)
            .collect();
        matches.sort_by_key(|b| {
            let len = b.len();
            let first_is_char = matches!(b.0.first(), Some(ki) if matches!(ki.code, KeyCode::Char(_)) && ki.modifiers.is_empty());
            // Prefer: shorter sequences, then plain chars over special keys
            (len, !first_is_char)
        });
        matches.into_iter().next()
    }

    fn insert(map: &mut HashMap<KeyInput, TrieNode>, keys: &[KeyInput], action: Action) {
        let key = keys[0].clone();
        if keys.len() == 1 {
            match map.get_mut(&key) {
                Some(TrieNode::Branch(children)) => {
                    let children = std::mem::take(children);
                    map.insert(key, TrieNode::ActionAndChildren { action, children });
                }
                Some(TrieNode::ActionAndChildren { action: a, .. }) => {
                    *a = action;
                }
                _ => {
                    map.insert(key, TrieNode::Leaf(action));
                }
            }
        } else {
            let children = match map.get_mut(&key) {
                Some(TrieNode::Leaf(existing_action)) => {
                    let existing_action = existing_action.clone();
                    let children = HashMap::new();
                    map.insert(
                        key.clone(),
                        TrieNode::ActionAndChildren {
                            action: existing_action,
                            children,
                        },
                    );
                    match map.get_mut(&key) {
                        Some(TrieNode::ActionAndChildren { children, .. }) => children,
                        _ => unreachable!(),
                    }
                }
                Some(TrieNode::Branch(children)) => children,
                Some(TrieNode::ActionAndChildren { children, .. }) => children,
                None => {
                    map.insert(key.clone(), TrieNode::Branch(HashMap::new()));
                    match map.get_mut(&key) {
                        Some(TrieNode::Branch(children)) => children,
                        _ => unreachable!(),
                    }
                }
            };
            Self::insert(children, &keys[1..], action);
        }
    }

    fn remove(map: &mut HashMap<KeyInput, TrieNode>, keys: &[KeyInput]) -> bool {
        let key = &keys[0];
        if keys.len() == 1 {
            match map.get_mut(key) {
                Some(TrieNode::Leaf(_)) => {
                    map.remove(key);
                    true
                }
                Some(TrieNode::ActionAndChildren { children, .. }) => {
                    let children = std::mem::take(children);
                    if children.is_empty() {
                        map.remove(key);
                    } else {
                        map.insert(key.clone(), TrieNode::Branch(children));
                    }
                    true
                }
                _ => false,
            }
        } else {
            match map.get_mut(key) {
                Some(TrieNode::Branch(children)) => {
                    let removed = Self::remove(children, &keys[1..]);
                    if children.is_empty() {
                        map.remove(key);
                    }
                    removed
                }
                Some(TrieNode::ActionAndChildren { children, .. }) => {
                    let removed = Self::remove(children, &keys[1..]);
                    if children.is_empty() {
                        let action = match map.get(key) {
                            Some(TrieNode::ActionAndChildren { action, .. }) => action.clone(),
                            _ => unreachable!(),
                        };
                        map.insert(key.clone(), TrieNode::Leaf(action));
                    }
                    removed
                }
                _ => false,
            }
        }
    }

    fn collect_bindings(
        map: &HashMap<KeyInput, TrieNode>,
        path: &mut Vec<KeyInput>,
        result: &mut Vec<(KeyBinding, Action)>,
    ) {
        for (key, node) in map {
            path.push(key.clone());
            match node {
                TrieNode::Leaf(action) => {
                    result.push((KeyBinding(path.clone()), action.clone()));
                }
                TrieNode::Branch(children) => {
                    Self::collect_bindings(children, path, result);
                }
                TrieNode::ActionAndChildren { action, children } => {
                    result.push((KeyBinding(path.clone()), action.clone()));
                    Self::collect_bindings(children, path, result);
                }
            }
            path.pop();
        }
    }
}

/// The complete keymap: all contexts.
#[derive(Debug, Clone)]
pub struct Keymap {
    contexts: HashMap<KeyContext, ContextKeymap>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new()
    }
}

impl Keymap {
    pub fn new() -> Self {
        Self {
            contexts: HashMap::new(),
        }
    }

    /// Get the context keymap, creating it if it doesn't exist.
    pub fn context_mut(&mut self, context: KeyContext) -> &mut ContextKeymap {
        self.contexts.entry(context).or_default()
    }

    /// Get the context keymap (read-only).
    pub fn context(&self, context: KeyContext) -> Option<&ContextKeymap> {
        self.contexts.get(&context)
    }

    /// Look up a key sequence in a specific context.
    pub fn lookup(&self, context: KeyContext, keys: &[KeyInput]) -> LookupResult {
        match self.contexts.get(&context) {
            Some(ctx) => ctx.lookup(keys),
            None => LookupResult::NoMatch,
        }
    }

    /// Check if the key sequence is a prefix in the given context.
    pub fn is_prefix(&self, context: KeyContext, keys: &[KeyInput]) -> bool {
        match self.contexts.get(&context) {
            Some(ctx) => ctx.is_prefix(keys),
            None => false,
        }
    }

    /// Get neovim-notation description of a binding (for config files).
    pub fn describe_binding(&self, context: KeyContext, action: &Action) -> Option<String> {
        self.contexts
            .get(&context)
            .and_then(|ctx| ctx.binding_for_action(action))
            .map(|b| format_key_binding(&b))
    }

    /// Get human-friendly description of a binding (for UI display).
    /// Uses `Ctrl+d`, `Space+a`, etc. instead of neovim notation.
    pub fn describe_binding_display(&self, context: KeyContext, action: &Action) -> Option<String> {
        self.contexts
            .get(&context)
            .and_then(|ctx| ctx.binding_for_action(action))
            .map(|b| super::notation::format_key_binding_display(&b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::notation::parse_key_binding;

    fn bind(ctx: &mut ContextKeymap, notation: &str, action: Action) {
        ctx.bind(parse_key_binding(notation).unwrap(), action);
    }

    fn lookup(ctx: &ContextKeymap, notation: &str) -> LookupResult {
        let binding = parse_key_binding(notation).unwrap();
        ctx.lookup(binding.keys())
    }

    fn is_prefix(ctx: &ContextKeymap, notation: &str) -> bool {
        let binding = parse_key_binding(notation).unwrap();
        ctx.is_prefix(binding.keys())
    }

    // === Single-key lookup ===

    #[test]
    fn bind_and_lookup_single_key() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        assert_eq!(lookup(&ctx, "j"), LookupResult::Found(Action::MoveDown));
    }

    #[test]
    fn lookup_unbound_key() {
        let ctx = ContextKeymap::new();
        assert_eq!(lookup(&ctx, "j"), LookupResult::NoMatch);
    }

    #[test]
    fn bind_then_unbind() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        assert!(ctx.unbind(&parse_key_binding("j").unwrap()));
        assert_eq!(lookup(&ctx, "j"), LookupResult::NoMatch);
    }

    #[test]
    fn unbind_nonexistent_returns_false() {
        let mut ctx = ContextKeymap::new();
        assert!(!ctx.unbind(&parse_key_binding("j").unwrap()));
    }

    #[test]
    fn multiple_bindings_same_context() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        bind(&mut ctx, "k", Action::MoveUp);
        assert_eq!(lookup(&ctx, "j"), LookupResult::Found(Action::MoveDown));
        assert_eq!(lookup(&ctx, "k"), LookupResult::Found(Action::MoveUp));
    }

    #[test]
    fn rebind_overwrites() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        bind(&mut ctx, "j", Action::MoveUp);
        assert_eq!(lookup(&ctx, "j"), LookupResult::Found(Action::MoveUp));
    }

    // === Sequence lookup ===

    #[test]
    fn bind_sequence_prefix_detection() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "gg", Action::GoTop);
        assert!(is_prefix(&ctx, "g"));
        assert_eq!(lookup(&ctx, "g"), LookupResult::Prefix);
    }

    #[test]
    fn bind_sequence_complete_match() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "gg", Action::GoTop);
        assert_eq!(lookup(&ctx, "gg"), LookupResult::Found(Action::GoTop));
    }

    #[test]
    fn bind_sequence_non_matching_continuation() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "gg", Action::GoTop);
        assert_eq!(lookup(&ctx, "gx"), LookupResult::NoMatch);
    }

    #[test]
    fn two_sequences_same_prefix() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "gg", Action::GoTop);
        bind(&mut ctx, "gd", Action::SynctexInverse);
        assert!(is_prefix(&ctx, "g"));
        assert_eq!(lookup(&ctx, "gg"), LookupResult::Found(Action::GoTop));
        assert_eq!(
            lookup(&ctx, "gd"),
            LookupResult::Found(Action::SynctexInverse)
        );
    }

    #[test]
    fn space_prefixed_sequence() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "<Space>f", Action::OpenBookSearch);
        assert!(is_prefix(&ctx, "<Space>"));
        assert_eq!(
            lookup(&ctx, "<Space>f"),
            LookupResult::Found(Action::OpenBookSearch)
        );
    }

    #[test]
    fn unbind_sequence() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "gg", Action::GoTop);
        assert!(ctx.unbind(&parse_key_binding("gg").unwrap()));
        assert_eq!(lookup(&ctx, "gg"), LookupResult::NoMatch);
        // Prefix should also be gone since no sequences remain under 'g'
        assert!(!is_prefix(&ctx, "g"));
    }

    // === Action + children (same prefix) ===

    #[test]
    fn single_key_action_and_sequence() {
        let mut ctx = ContextKeymap::new();
        // First bind "g" as a standalone action
        bind(&mut ctx, "g", Action::MoveDown);
        // Then also bind "gg" as a sequence
        bind(&mut ctx, "gg", Action::GoTop);

        // "g" should return the action (Found), not Prefix
        assert_eq!(lookup(&ctx, "g"), LookupResult::Found(Action::MoveDown));
        // "gg" should still work
        assert_eq!(lookup(&ctx, "gg"), LookupResult::Found(Action::GoTop));
        // "g" should also be a prefix
        assert!(is_prefix(&ctx, "g"));
    }

    #[test]
    fn bind_sequence_then_single_key() {
        let mut ctx = ContextKeymap::new();
        // Bind sequence first
        bind(&mut ctx, "gg", Action::GoTop);
        // Then bind the prefix as a standalone action
        bind(&mut ctx, "g", Action::MoveDown);

        assert_eq!(lookup(&ctx, "g"), LookupResult::Found(Action::MoveDown));
        assert_eq!(lookup(&ctx, "gg"), LookupResult::Found(Action::GoTop));
    }

    #[test]
    fn unbind_single_key_preserves_sequence() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "g", Action::MoveDown);
        bind(&mut ctx, "gg", Action::GoTop);
        ctx.unbind(&parse_key_binding("g").unwrap());

        // "g" action is gone, but prefix should remain
        assert_eq!(lookup(&ctx, "g"), LookupResult::Prefix);
        assert_eq!(lookup(&ctx, "gg"), LookupResult::Found(Action::GoTop));
    }

    #[test]
    fn unbind_sequence_preserves_single_key() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "g", Action::MoveDown);
        bind(&mut ctx, "gg", Action::GoTop);
        ctx.unbind(&parse_key_binding("gg").unwrap());

        assert_eq!(lookup(&ctx, "g"), LookupResult::Found(Action::MoveDown));
        assert_eq!(lookup(&ctx, "gg"), LookupResult::NoMatch);
        assert!(!is_prefix(&ctx, "g"));
    }

    // === Override and independent contexts ===

    #[test]
    fn independent_contexts() {
        let mut keymap = Keymap::new();
        keymap
            .context_mut(KeyContext::Navigation)
            .bind(parse_key_binding("j").unwrap(), Action::MoveDown);
        keymap
            .context_mut(KeyContext::EpubContent)
            .bind(parse_key_binding("j").unwrap(), Action::ScrollDown);

        let nav_keys = parse_key_binding("j").unwrap();
        let epub_keys = parse_key_binding("j").unwrap();

        assert_eq!(
            keymap.lookup(KeyContext::Navigation, nav_keys.keys()),
            LookupResult::Found(Action::MoveDown)
        );
        assert_eq!(
            keymap.lookup(KeyContext::EpubContent, epub_keys.keys()),
            LookupResult::Found(Action::ScrollDown)
        );
    }

    #[test]
    fn lookup_in_empty_context() {
        let keymap = Keymap::new();
        let keys = parse_key_binding("j").unwrap();
        assert_eq!(
            keymap.lookup(KeyContext::Navigation, keys.keys()),
            LookupResult::NoMatch
        );
    }

    // === Reverse lookup ===

    #[test]
    fn binding_for_action_found() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        let result = ctx.binding_for_action(&Action::MoveDown);
        assert!(result.is_some());
        assert_eq!(result.unwrap().keys(), &[KeyInput::char('j')]);
    }

    #[test]
    fn binding_for_action_not_found() {
        let ctx = ContextKeymap::new();
        assert!(ctx.binding_for_action(&Action::MoveDown).is_none());
    }

    // === Edge cases ===

    #[test]
    fn lookup_empty_keys() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        assert_eq!(ctx.lookup(&[]), LookupResult::NoMatch);
    }

    #[test]
    fn modifier_differentiated_keys() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        bind(&mut ctx, "<C-j>", Action::ScrollHalfDown);
        assert_eq!(lookup(&ctx, "j"), LookupResult::Found(Action::MoveDown));
        assert_eq!(
            lookup(&ctx, "<C-j>"),
            LookupResult::Found(Action::ScrollHalfDown)
        );
    }

    #[test]
    fn all_bindings_returns_correct_count() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "j", Action::MoveDown);
        bind(&mut ctx, "k", Action::MoveUp);
        bind(&mut ctx, "gg", Action::GoTop);
        let bindings = ctx.all_bindings();
        assert_eq!(bindings.len(), 3);
    }

    #[test]
    fn all_bindings_includes_sequences() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "<Space>f", Action::OpenBookSearch);
        let bindings = ctx.all_bindings();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].1, Action::OpenBookSearch);
    }

    #[test]
    fn describe_binding_found() {
        let mut keymap = Keymap::new();
        keymap
            .context_mut(KeyContext::Global)
            .bind(parse_key_binding("?").unwrap(), Action::ToggleHelp);
        let desc = keymap.describe_binding(KeyContext::Global, &Action::ToggleHelp);
        assert_eq!(desc, Some("?".to_string()));
    }

    #[test]
    fn describe_binding_not_found() {
        let keymap = Keymap::new();
        let desc = keymap.describe_binding(KeyContext::Global, &Action::ToggleHelp);
        assert_eq!(desc, None);
    }

    #[test]
    fn three_key_sequence() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "abc", Action::Quit);
        assert!(is_prefix(&ctx, "a"));
        assert!(is_prefix(&ctx, "ab"));
        assert_eq!(lookup(&ctx, "abc"), LookupResult::Found(Action::Quit));
    }

    #[test]
    fn ctrl_modified_sequence() {
        let mut ctx = ContextKeymap::new();
        bind(&mut ctx, "<C-w>j", Action::MoveDown);
        let prefix_keys = parse_key_binding("<C-w>").unwrap();
        assert!(ctx.is_prefix(prefix_keys.keys()));
        assert_eq!(
            lookup(&ctx, "<C-w>j"),
            LookupResult::Found(Action::MoveDown)
        );
    }
}
