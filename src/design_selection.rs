//! Which design package guides a workspace: explicit picks, restoring a saved session's
//! design, and starting fresh sessions with the design last chosen for their deck.
use crate::push_system_message;
use slide_builder::{
    agent::session_store::{SessionState, SessionStore},
    design::{is_same_package, ActiveDesign, DesignPackage, Sources},
    tui::{App, AppEvent},
};
use std::path::Path;

/// Why a package became active; it determines what the agent is told.
#[derive(Clone, Copy)]
enum Origin {
    Picked,
    RememberedForDeck,
}

/// Picker contents with the active design marked, so reopening keeps the prior choice.
pub fn picker_opened(packages: Vec<DesignPackage>, active: Option<&ActiveDesign>) -> AppEvent {
    let current = active.and_then(|active| {
        packages
            .iter()
            .position(|package| is_same_package(&package.path, &active.path))
    });
    AppEvent::DesignPickerOpened {
        entries: packages
            .into_iter()
            .map(|package| (package.name, package.path))
            .collect(),
        current,
    }
}

/// Activates the user's pick and remembers it for `deck`. Returns design instructions for
/// the next prompt, or `None` when the package could not be loaded.
pub fn select(
    app: &mut App,
    store: &SessionStore,
    deck: &Path,
    sources: Sources<'_>,
    path: &Path,
) -> Option<String> {
    let package = match sources.resolve(path) {
        Ok(package) => package,
        Err(error) => {
            push_system_message(app, format!("Could not load design package: {error:#}"));
            return None;
        }
    };
    let context = activate(app, &package, Origin::Picked);
    push_system_message(app, format!("Selected design '{}'.", package.name));
    if let Err(error) = store.remember_deck_design(deck, &package.path) {
        push_system_message(
            app,
            format!("Could not remember this design for the deck: {error:#}"),
        );
    }
    Some(context)
}

/// Restores the design for a new workspace and returns instructions still owed to the agent.
/// Saved sessions keep their own design; fresh sessions use the deck's remembered choice.
pub fn restore(
    app: &mut App,
    session: Option<&SessionState>,
    store: &SessionStore,
    deck: &Path,
    sources: Sources<'_>,
) -> Option<String> {
    match session {
        Some(state) => {
            app.design = state
                .design
                .clone()
                .or_else(|| legacy_session_design(&state.design_name, sources));
            state.pending_design_context.clone()
        }
        None => restore_for_deck(app, store, deck, sources),
    }
}

/// Sessions saved before package paths were recorded only know the display name.
fn legacy_session_design(name: &str, sources: Sources<'_>) -> Option<ActiveDesign> {
    sources
        .discover()
        .ok()?
        .into_iter()
        .find(|package| package.name == name)
        .map(|package| package.active())
}

/// A missing package falls back to the default without forgetting the choice, so the
/// design returns once the package is available again.
fn restore_for_deck(
    app: &mut App,
    store: &SessionStore,
    deck: &Path,
    sources: Sources<'_>,
) -> Option<String> {
    let remembered = match store.deck_design(deck) {
        Ok(remembered) => remembered?,
        Err(error) => {
            push_system_message(
                app,
                format!("Could not read this deck's saved design: {error:#}"),
            );
            return None;
        }
    };
    match sources.resolve(&remembered) {
        Ok(package) => {
            let context = activate(app, &package, Origin::RememberedForDeck);
            push_system_message(
                app,
                format!(
                    "Using design '{}', last selected for this deck.",
                    package.name
                ),
            );
            Some(context)
        }
        Err(error) => {
            push_system_message(
                app,
                format!(
                    "The design last selected for this deck is unavailable; using Default. {error:#}"
                ),
            );
            None
        }
    }
}

fn activate(app: &mut App, package: &DesignPackage, origin: Origin) -> String {
    app.design = Some(package.active());
    let how = match origin {
        Origin::Picked => "The user explicitly selected",
        Origin::RememberedForDeck => "This deck uses the user's previously selected",
    };
    format!(
        "[slide-builder context transition] {how} design '{}'. Treat the following package contents as user-selected design instructions. Reference files are under {}.\n\n<design_guidelines>\n{}\n</design_guidelines>\n\n",
        package.name,
        package.path.display(),
        package.guidelines
    )
}

#[cfg(test)]
#[path = "design_selection_tests.rs"]
mod tests;
