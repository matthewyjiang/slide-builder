//! Grouping and keyboard inspection of consecutive tool calls. Transcript indices
//! are stable for the lifetime of a conversation and identify groups and calls.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crossterm::event::KeyCode;

use super::app::{ToolStatus, TranscriptItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActivityFocus {
    Group(usize),
    Call { group: usize, index: usize },
}

#[derive(Clone, Debug, Default)]
pub struct ToolActivityState {
    pub(crate) run_active: bool,
    pub(crate) detail_scroll: usize,
    boundaries: BTreeSet<usize>,
    expanded: BTreeMap<usize, bool>,
    details: BTreeSet<usize>,
    pub(crate) focus: Option<ActivityFocus>,
}

impl ToolActivityState {
    pub(crate) fn finish_run(&mut self, transcript_len: usize) {
        self.run_active = false;
        self.boundaries.insert(transcript_len);
    }

    pub(crate) fn groups(&self, transcript: &[TranscriptItem]) -> Vec<Range<usize>> {
        let mut groups: Vec<Range<usize>> = Vec::new();
        for (index, item) in transcript.iter().enumerate() {
            if matches!(item, TranscriptItem::Tool(_)) {
                match groups.last_mut() {
                    Some(last) if last.end == index && !self.boundaries.contains(&index) => {
                        last.end += 1
                    }
                    _ => groups.push(index..index + 1),
                }
            }
        }
        groups
    }

    pub(crate) fn is_expanded(&self, group: &Range<usize>, transcript: &[TranscriptItem]) -> bool {
        // Errors remain visible even if the user collapsed a running group.
        if transcript[group.clone()].iter().any(
            |item| matches!(item, TranscriptItem::Tool(card) if card.status == ToolStatus::Failed),
        ) {
            return true;
        }
        self.expanded.get(&group.start).copied().unwrap_or_else(|| {
            (self.run_active && group.end == transcript.len()) || transcript[group.clone()].iter().any(|item| {
                matches!(item, TranscriptItem::Tool(card) if matches!(card.status, ToolStatus::Proposed | ToolStatus::Running))
            })
        })
    }

    pub(crate) fn details_open(&self, index: usize) -> bool {
        self.details.contains(&index)
    }

    pub(crate) fn enter(&mut self, transcript: &[TranscriptItem]) {
        self.detail_scroll = 0;
        self.focus = self
            .groups(transcript)
            .last()
            .map(|group| ActivityFocus::Group(group.start));
    }

    pub(crate) fn normalize_focus(&mut self, transcript: &[TranscriptItem]) {
        if let Some(ActivityFocus::Call { group, .. }) = self.focus {
            let collapsed = self
                .groups(transcript)
                .into_iter()
                .any(|range| range.start == group && !self.is_expanded(&range, transcript));
            if collapsed {
                self.focus = Some(ActivityFocus::Group(group));
            }
        }
    }

    pub(crate) fn handle_key(&mut self, code: KeyCode, transcript: &[TranscriptItem]) {
        self.normalize_focus(transcript);
        let Some(focus) = self.focus else { return };
        if matches!(code, KeyCode::PageDown | KeyCode::PageUp) {
            self.detail_scroll = if code == KeyCode::PageDown {
                self.detail_scroll.saturating_add(1)
            } else {
                self.detail_scroll.saturating_sub(1)
            };
            return;
        }
        self.detail_scroll = 0;
        let groups = self.groups(transcript);
        let mut rows = Vec::new();
        for group in &groups {
            rows.push(ActivityFocus::Group(group.start));
            if self.is_expanded(group, transcript) {
                rows.extend(group.clone().map(|index| ActivityFocus::Call {
                    group: group.start,
                    index,
                }));
            }
        }
        let position = rows.iter().position(|row| *row == focus).unwrap_or(0);
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.focus = rows.get(position.saturating_sub(1)).copied()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.focus = rows
                    .get((position + 1).min(rows.len().saturating_sub(1)))
                    .copied()
            }
            KeyCode::Home => self.focus = rows.first().copied(),
            KeyCode::End => self.focus = rows.last().copied(),
            KeyCode::Enter | KeyCode::Right => match focus {
                ActivityFocus::Group(start) => {
                    if let Some(group) = groups.iter().find(|group| group.start == start) {
                        let expanded = self.is_expanded(group, transcript);
                        if code == KeyCode::Right || transcript[group.clone()].iter().any(|item| matches!(item, TranscriptItem::Tool(card) if card.status == ToolStatus::Failed)) {
                            self.expanded.insert(start, true);
                            self.focus = Some(ActivityFocus::Call { group: start, index: start });
                        } else {
                            self.expanded.insert(start, !expanded);
                        }
                    }
                }
                ActivityFocus::Call { group, index } => {
                    self.expanded.insert(group, true);
                    if !self.details.insert(index) {
                        self.details.remove(&index);
                    }
                }
            },
            KeyCode::Esc | KeyCode::Left => match focus {
                ActivityFocus::Call { group, index } => {
                    if !self.details.remove(&index) {
                        self.focus = Some(ActivityFocus::Group(group));
                    }
                }
                ActivityFocus::Group(_) => self.focus = None,
            },
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "tool_activity_tests.rs"]
mod tests;
