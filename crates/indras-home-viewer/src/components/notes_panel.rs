//! Notes panel component showing note cards.

use dioxus::prelude::*;

use crate::state::{short_id, AppState, Note};

/// Notes panel showing the list of notes as cards.
#[component]
pub fn NotesPanel(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        section {
            class: "notes-panel",

            div {
                class: "panel-header",
                h2 {
                    class: "panel-title",
                    "Notes"
                }
                span {
                    class: "panel-count",
                    "{state_read.notes.active_count()} active"
                }
            }

            div {
                class: "notes-grid",

                if state_read.notes.notes.is_empty() {
                    div {
                        class: "notes-empty",
                        p { "No notes created yet." }
                        p { class: "notes-empty-hint", "Notes will appear here as you create them." }
                    }
                } else {
                    for note in state_read.notes.notes_by_recency().iter().take(12) {
                        NoteCard {
                            key: "{note.id}",
                            note: (*note).clone(),
                            is_selected: state_read.selected_note_id.as_ref() == Some(&note.id),
                        }
                    }
                }
            }
        }
    }
}

/// A single note card.
#[component]
fn NoteCard(note: Note, is_selected: bool) -> Element {
    let card_class = if note.deleted {
        "note-card note-card-deleted"
    } else if is_selected {
        "note-card note-card-selected"
    } else {
        "note-card"
    };

    rsx! {
        div {
            class: "{card_class}",

            // Note title
            h3 {
                class: "note-card-title",
                if note.deleted {
                    del { "{note.title}" }
                } else {
                    "{note.title}"
                }
            }

            // Tags indicator
            if note.tag_count > 0 {
                div {
                    class: "note-card-tags",
                    for i in 0..note.tag_count.min(3) {
                        span {
                            key: "{i}",
                            class: "tag-pill",
                            "tag"
                        }
                    }
                    if note.tag_count > 3 {
                        span {
                            class: "tag-more",
                            "+{note.tag_count - 3}"
                        }
                    }
                }
            }

            // Metadata footer
            div {
                class: "note-card-footer",

                span {
                    class: "note-card-id",
                    "{short_id(&note.id)}"
                }

                span {
                    class: "note-card-tick",
                    "tick {note.created_tick}"
                }

                if let Some(updated) = note.updated_tick {
                    span {
                        class: "note-card-updated",
                        "edited {updated}"
                    }
                }
            }
        }
    }
}
