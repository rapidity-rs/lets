//! Interactive command picker for a bare `lets` on a terminal.
//!
//! Owning the render loop rather than delegating to a prompt library buys
//! three things that matter here:
//!
//! - **Nothing wraps.** Every line is truncated to the terminal width, so
//!   the number of lines drawn always equals the number of rows. Redraws
//!   that move the cursor up by "lines drawn last frame" are then exact;
//!   an off-by-one leaves stale text on screen and the list appears to
//!   walk up and down as the selection moves.
//! - **Scroll state follows the filter.** The window resets whenever the
//!   query changes, so clearing a search always returns to the top.
//! - **Styling is ours.** Match scoring runs against plain text, so colour
//!   never becomes searchable and highlighting can't split an escape
//!   sequence.

use console::{Key, Term};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::style::{self, DIM, GROUP, NAME, truncate};
use crate::tree::{CommandNode, CommandTree};

/// A command the user can pick.
struct Choice {
    /// Full command path, as it would be typed.
    path: Vec<String>,
    /// Nesting level, for indenting the unfiltered tree.
    depth: usize,
    /// What the argument list looks like, e.g. `<environment>`.
    signature: String,
    description: Option<String>,
    aliases: Vec<String>,
    /// The command path and its arguments, as plain text. Never styled:
    /// colour in the haystack would make escape sequences searchable.
    subject: String,
    /// The description, scored separately so a word buried in prose can't
    /// outrank a name.
    prose: String,
}

/// A line in the list: either a group heading or a pickable command.
enum Row {
    /// A group that holds subcommands but can't be run itself. Shown for
    /// context in the unfiltered tree; never selectable.
    Group {
        depth: usize,
        name: String,
    },
    Choice(usize),
}

/// How the picker ended.
pub enum Outcome {
    /// Run this command path.
    Run(Vec<String>),
    /// The user backed out; they asked for nothing and should get nothing.
    Cancelled,
    /// There was nothing to pick, so the caller should fall back to help.
    Nothing,
}

/// Show the picker and wait for the user to choose or back out.
pub fn pick(tree: &CommandTree) -> Outcome {
    let choices = collect(tree);
    if choices.is_empty() {
        return Outcome::Nothing;
    }
    let term = Term::stderr();
    let mut state = State::new(&choices);
    let picked = state.run(&term);
    // However we leave — picked, cancelled, or a write failing midway —
    // the screen goes back to how we found it.
    let _ = state.erase(&term);
    let _ = term.show_cursor();
    match picked {
        Some(path) => Outcome::Run(path),
        None => Outcome::Cancelled,
    }
}

/// Flatten the tree into pickable commands, skipping hidden ones.
fn collect(tree: &CommandTree) -> Vec<Choice> {
    fn walk(commands: &[CommandNode], prefix: &[String], depth: usize, out: &mut Vec<Choice>) {
        for cmd in commands {
            if cmd.hide {
                continue;
            }
            let mut path = prefix.to_vec();
            path.push(cmd.name.clone());
            if cmd.is_runnable() {
                let signature = crate::listing::signature(cmd);
                let description = cmd.description.clone();
                // Names, aliases, and arguments are what you usually
                // reach for; descriptions make a task findable by what it
                // does when you can't recall the name.
                let subject = [path.join(" "), cmd.aliases.join(" "), signature.clone()].join(" ");
                out.push(Choice {
                    path: path.clone(),
                    depth,
                    signature,
                    prose: description.clone().unwrap_or_default(),
                    description,
                    aliases: cmd.aliases.clone(),
                    subject,
                });
            }
            walk(&cmd.children, &path, depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(&tree.commands, &[], 0, &mut out);
    out
}

/// Lines of chrome around the list: header, two rules, and the key hints.
const CHROME: usize = 4;
/// Below this many rows the chrome costs more than it explains.
const COMPACT_BELOW: usize = 10;

struct State<'a> {
    choices: &'a [Choice],
    matcher: SkimMatcherV2,
    query: String,
    /// Indices into `choices`, best match first, or every choice in tree
    /// order when the query is empty.
    matches: Vec<usize>,
    /// Index into `rows`, always pointing at a selectable row.
    cursor: usize,
    /// First visible row, kept so the cursor stays on screen.
    offset: usize,
    /// Lines drawn last frame, so the next redraw lands exactly on them.
    drawn: usize,
}

impl<'a> State<'a> {
    fn new(choices: &'a [Choice]) -> Self {
        State {
            choices,
            matcher: SkimMatcherV2::default(),
            query: String::new(),
            matches: (0..choices.len()).collect(),
            cursor: 0,
            offset: 0,
            drawn: 0,
        }
    }

    fn run(&mut self, term: &Term) -> Option<Vec<String>> {
        let _ = term.hide_cursor();
        self.refilter();
        loop {
            let rows = self.rows();
            if self.draw(term, &rows).is_err() {
                return None;
            }
            let key = term.read_key().ok()?;
            match key {
                Key::Escape | Key::CtrlC => return None,
                Key::Enter => {
                    return match rows.get(self.cursor) {
                        Some(Row::Choice(i)) => Some(self.choices[*i].path.clone()),
                        _ => None,
                    };
                }
                Key::ArrowDown | Key::Tab => self.step(&rows, 1),
                Key::ArrowUp | Key::BackTab => self.step(&rows, -1),
                Key::Backspace => {
                    self.query.pop();
                    self.refilter();
                }
                Key::Char(c) if !c.is_control() => {
                    self.query.push(c);
                    self.refilter();
                }
                _ => {}
            }
        }
    }

    /// Rescore against the query and put the cursor back at the top.
    ///
    /// Resetting the window here is the whole point: leaving it where it
    /// was means clearing a search returns to a list scrolled halfway down,
    /// which then slides as the selection moves.
    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.matches = (0..self.choices.len()).collect();
        } else {
            let mut scored: Vec<(usize, i64)> = self
                .choices
                .iter()
                .enumerate()
                .filter_map(|(i, c)| self.score(c).map(|score| (i, score)))
                .collect();
            // Best first, and stable on ties so equal matches keep the
            // order they were declared in.
            scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
            self.matches = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.cursor = 0;
        self.offset = 0;
        let rows = self.rows();
        if !matches!(rows.first(), Some(Row::Choice(_))) {
            self.step(&rows, 1);
        }
    }

    /// Score one command against the query, or `None` if it doesn't match.
    ///
    /// A hit on the name always outranks one found only in the description,
    /// so typing `dep` puts `deploy` above every task whose prose happens
    /// to contain those letters.
    fn score(&self, choice: &Choice) -> Option<i64> {
        /// Larger than any score the matcher produces for these strings.
        const NAME_MATCH: i64 = 10_000;
        match self.matcher.fuzzy_match(&choice.subject, &self.query) {
            Some(score) => Some(score + NAME_MATCH),
            None => self.matcher.fuzzy_match(&choice.prose, &self.query),
        }
    }

    /// The list as it should appear: a tree with group headings when
    /// browsing, a flat ranked list when searching.
    fn rows(&self) -> Vec<Row> {
        if !self.query.is_empty() {
            return self.matches.iter().map(|i| Row::Choice(*i)).collect();
        }

        let mut rows = Vec::new();
        let mut open: Vec<String> = Vec::new();
        for &i in &self.matches {
            let choice = &self.choices[i];
            // Announce any ancestor group this command sits under, so the
            // tree reads the way `--list` does.
            let parents = &choice.path[..choice.path.len() - 1];
            while !open.is_empty() && !parents.starts_with(&open) {
                open.pop();
            }
            for (depth, name) in parents.iter().enumerate().skip(open.len()) {
                rows.push(Row::Group {
                    depth,
                    name: name.clone(),
                });
                open.push(name.clone());
            }
            rows.push(Row::Choice(i));
        }
        rows
    }

    /// Move the cursor to the next selectable row in `direction`, wrapping.
    fn step(&mut self, rows: &[Row], direction: isize) {
        if rows.is_empty() {
            return;
        }
        let len = rows.len() as isize;
        let mut at = self.cursor as isize;
        for _ in 0..len {
            at = (at + direction).rem_euclid(len);
            if matches!(rows[at as usize], Row::Choice(_)) {
                self.cursor = at as usize;
                return;
            }
        }
    }

    /// Scroll so the cursor is visible, and report the visible window.
    fn window(&mut self, rows: &[Row], height: usize) -> std::ops::Range<usize> {
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
        // A group heading directly above the first visible row is the only
        // thing naming the command under it; keep it on screen.
        if self.offset > 0
            && matches!(rows.get(self.offset - 1), Some(Row::Group { .. }))
            && self.cursor < self.offset + height - 1
        {
            self.offset -= 1;
        }
        self.offset = self.offset.min(rows.len().saturating_sub(height));
        self.offset..(self.offset + height).min(rows.len())
    }

    fn draw(&mut self, term: &Term, rows: &[Row]) -> std::io::Result<()> {
        let (term_rows, term_cols) = term.size();
        // One column short of the edge: a line filling the last cell makes
        // some terminals wrap, which would break the line accounting.
        let width = (term_cols as usize).saturating_sub(1).max(20);
        let term_rows = term_rows as usize;
        let compact = term_rows < COMPACT_BELOW;
        // Header, two rules, and the hint line; compact keeps only the hint.
        let chrome = if compact { 1 } else { CHROME };
        // One row spare so the shell prompt has somewhere to return to.
        let height = term_rows.saturating_sub(chrome + 1).max(1);

        let visible = self.window(rows, height);
        // Filtering flattens the tree, so rows carry their full path and
        // lose the indent that a group heading would otherwise explain.
        let flat = !self.query.is_empty();
        let label_width = self.label_width(&rows[visible.clone()], flat, width);

        let mut lines = Vec::new();
        if !compact {
            lines.push(self.header(width));
            lines.push(style::err(DIM, "─".repeat(width.min(60))));
        }
        for (index, row) in rows
            .iter()
            .enumerate()
            .take(visible.end)
            .skip(visible.start)
        {
            lines.push(self.row(row, index == self.cursor, flat, label_width, width));
        }
        if rows.is_empty() {
            lines.push(format!("  {}", style::err(DIM, "no matching commands")));
        }
        if !compact {
            lines.push(style::err(DIM, "─".repeat(width.min(60))));
        }
        // Even a cramped pane gets one line of orientation: without it
        // there is nothing to say the list scrolls or that esc gets out.
        lines.push(self.hints(rows, visible, width, compact));

        // Land exactly on last frame's first line, overwrite, then wipe any
        // lines this frame no longer needs.
        if self.drawn > 0 {
            term.move_cursor_up(self.drawn)?;
        }
        for line in &lines {
            term.clear_line()?;
            term.write_line(line)?;
        }
        for _ in lines.len()..self.drawn {
            term.clear_line()?;
            term.write_line("")?;
        }
        if lines.len() < self.drawn {
            term.move_cursor_up(self.drawn - lines.len())?;
        }
        self.drawn = lines.len();
        term.flush()
    }

    fn header(&self, width: usize) -> String {
        let (left, right) = if self.query.is_empty() {
            (
                "Run a command".to_string(),
                format!("{} available", self.choices.len()),
            )
        } else {
            (
                format!("Run: {}▏", self.query),
                format!("{} of {}", self.matches.len(), self.choices.len()),
            )
        };
        let used = left.chars().count() + right.chars().count() + 2;
        let gap = width.min(60).saturating_sub(used).max(1);
        format!(
            "  {}{}{}",
            style::err(NAME, &left),
            " ".repeat(gap),
            style::err(DIM, &right)
        )
    }

    fn hints(
        &self,
        rows: &[Row],
        visible: std::ops::Range<usize>,
        width: usize,
        compact: bool,
    ) -> String {
        let mut hint = if compact {
            "  ↑↓  ⏎ run  esc".to_string()
        } else {
            "  ↑↓ move   ⏎ run   esc cancel".to_string()
        };
        if self.query.is_empty() && !compact {
            hint.push_str("   type to filter");
        }
        // Only claim there is more when there is.
        if visible.end < rows.len() || visible.start > 0 {
            hint.push_str(&format!(
                "   [{}–{} of {}]",
                visible.start + 1,
                visible.end,
                rows.len()
            ));
        }
        style::err(DIM, truncate(&hint, width))
    }

    /// Width of the name column across the visible rows, so descriptions
    /// line up without a long name off-screen dictating the layout.
    fn label_width(&self, rows: &[Row], flat: bool, width: usize) -> usize {
        rows.iter()
            .filter_map(|row| match row {
                Row::Choice(i) => {
                    let c = &self.choices[*i];
                    Some(2 + indent_of(c, flat) + plain_label(c, flat).chars().count())
                }
                Row::Group { .. } => None,
            })
            .max()
            .unwrap_or(0)
            .min(width / 2)
    }

    fn row(
        &self,
        row: &Row,
        selected: bool,
        flat: bool,
        label_width: usize,
        width: usize,
    ) -> String {
        match row {
            Row::Group { depth, name } => format!(
                "  {}{}",
                "  ".repeat(*depth),
                style::err(GROUP, format!("{name} ›"))
            ),
            Row::Choice(i) => {
                let choice = &self.choices[*i];
                let indent = " ".repeat(indent_of(choice, flat));
                let marker = if selected {
                    style::err(NAME, "❯ ")
                } else {
                    "  ".to_string()
                };

                let label = plain_label(choice, flat);
                let mut styled = format!(
                    "{}{}",
                    style::err(NAME, display_name(choice, flat)),
                    if choice.signature.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", style::err(DIM, &choice.signature))
                    }
                );

                let mut note = choice.description.clone().unwrap_or_default();
                if !choice.aliases.is_empty() {
                    if !note.is_empty() {
                        note.push(' ');
                    }
                    note.push_str(&format!("({})", choice.aliases.join(", ")));
                }
                if !note.is_empty() {
                    let used = 2 + indent_of(choice, flat) + label.chars().count();
                    let gap = label_width.saturating_sub(used) + 2;
                    let room = width.saturating_sub(used + gap);
                    styled.push_str(&" ".repeat(gap));
                    styled.push_str(&style::err(DIM, truncate(&note, room)));
                }
                format!("{marker}{indent}{styled}")
            }
        }
    }

    /// Wipe everything drawn, leaving the terminal as it was found.
    fn erase(&mut self, term: &Term) -> std::io::Result<()> {
        if self.drawn == 0 {
            return Ok(());
        }
        term.move_cursor_up(self.drawn)?;
        for _ in 0..self.drawn {
            term.clear_line()?;
            term.write_line("")?;
        }
        term.move_cursor_up(self.drawn)?;
        self.drawn = 0;
        term.flush()
    }
}

/// How the command is named on screen: its own name under a group heading
/// while browsing, its full path once filtering has flattened the tree and
/// removed the heading that gave it context.
fn display_name(choice: &Choice, flat: bool) -> String {
    if flat {
        choice.path.join(" ")
    } else {
        choice.path.last().cloned().unwrap_or_default()
    }
}

fn indent_of(choice: &Choice, flat: bool) -> usize {
    if flat { 0 } else { choice.depth * 2 }
}

/// The unstyled name-and-signature text, for measuring the column.
fn plain_label(choice: &Choice, flat: bool) -> String {
    match choice.signature.is_empty() {
        true => display_name(choice, flat),
        false => format!("{} {}", display_name(choice, flat), choice.signature),
    }
}

/// Whether an interactive picker can be shown: both the keyboard and the
/// display it draws on have to be a terminal, and colour-free terminals
/// still work — only redirection rules it out.
pub fn available() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(path: &[&str], description: &str, aliases: &[&str]) -> Choice {
        let path: Vec<String> = path.iter().map(|s| s.to_string()).collect();
        let aliases: Vec<String> = aliases.iter().map(|s| s.to_string()).collect();
        Choice {
            depth: path.len() - 1,
            subject: [path.join(" "), aliases.join(" ")].join(" "),
            prose: description.to_string(),
            description: (!description.is_empty()).then(|| description.to_string()),
            aliases,
            signature: String::new(),
            path,
        }
    }

    fn sample() -> Vec<Choice> {
        vec![
            choice(&["build"], "Compile everything", &["b"]),
            choice(&["deploy"], "Ship it", &[]),
            choice(&["doc", "api"], "Build API documentation", &[]),
            choice(&["doc", "site"], "Build the docs site", &[]),
            choice(&["test"], "Run the suite", &["t"]),
        ]
    }

    fn picked(state: &State, rows: &[Row]) -> String {
        match rows[state.cursor] {
            Row::Choice(i) => state.choices[i].path.join(" "),
            Row::Group { .. } => panic!("cursor parked on a group heading"),
        }
    }

    /// Clearing a search has to return to the top. Leaving the window where
    /// it was is what makes a list appear to slide as the selection moves.
    #[test]
    fn clearing_the_query_resets_the_window() {
        let choices = sample();
        let mut state = State::new(&choices);

        state.query.push('d');
        state.refilter();
        let rows = state.rows();
        state.step(&rows, 1);
        state.step(&rows, 1);
        state.window(&rows, 2);
        assert!(state.cursor > 0);

        state.query.pop();
        state.refilter();
        assert_eq!(state.cursor, 0, "cursor should return to the top");
        assert_eq!(state.offset, 0, "window should return to the top");
    }

    /// A name match always beats one found only in a description.
    #[test]
    fn names_outrank_prose() {
        let choices = sample();
        let mut state = State::new(&choices);
        state.query.push_str("doc");
        state.refilter();

        let rows = state.rows();
        assert_eq!(picked(&state, &rows), "doc api");
        // `build`'s description contains the letters too, but only in prose.
        let ranked: Vec<String> = state.matches[..2]
            .iter()
            .map(|i| choices[*i].path.join(" "))
            .collect();
        assert_eq!(ranked, vec!["doc api", "doc site"]);
    }

    /// Aliases are how people refer to commands, so they are searchable.
    #[test]
    fn aliases_are_searchable() {
        let choices = sample();
        let mut state = State::new(&choices);
        state.query.push('b');
        state.refilter();
        let rows = state.rows();
        assert_eq!(picked(&state, &rows), "build");
    }

    /// Browsing shows the tree with headings; searching flattens it, so the
    /// rows have to carry their full path instead.
    #[test]
    fn groups_head_the_tree_but_not_the_search() {
        let choices = sample();
        let mut state = State::new(&choices);

        let rows = state.rows();
        assert!(
            rows.iter()
                .any(|r| matches!(r, Row::Group { name, .. } if name == "doc")),
            "browsing should announce the group"
        );
        assert_eq!(display_name(&choices[2], false), "api");

        state.query.push_str("api");
        state.refilter();
        let rows = state.rows();
        assert!(
            rows.iter().all(|r| matches!(r, Row::Choice(_))),
            "searching should not emit group headings"
        );
        assert_eq!(display_name(&choices[2], true), "doc api");
    }

    /// The cursor must never land on a heading, which cannot be run.
    #[test]
    fn stepping_skips_group_headings() {
        let choices = sample();
        let mut state = State::new(&choices);
        let rows = state.rows();

        let mut seen = Vec::new();
        for _ in 0..rows.len() * 2 {
            seen.push(picked(&state, &rows));
            state.step(&rows, 1);
        }
        assert!(seen.contains(&"doc api".to_string()));
    }

    /// Scrolling keeps the cursor on screen in both directions.
    #[test]
    fn the_window_follows_the_cursor() {
        let choices = sample();
        let mut state = State::new(&choices);
        let rows = state.rows();

        for _ in 0..rows.len() + 2 {
            state.step(&rows, 1);
            let visible = state.window(&rows, 3);
            assert!(
                visible.contains(&state.cursor),
                "cursor {} outside {visible:?}",
                state.cursor
            );
        }
    }
}
