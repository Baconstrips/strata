// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

const RESULT_LIMIT: usize = 100;
const PUBLISH_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchItem {
    pub path: PathBuf,
    pub name: String,
    pub is_directory: bool,
    search_name: String,
    search_path: String,
}

pub enum SearchEvent {
    Results {
        query: String,
        items: Vec<SearchItem>,
        indexing: bool,
    },
}

enum SearchCommand {
    Query(String),
}

pub struct SearchHandle {
    cancelled: Arc<AtomicBool>,
    commands: Sender<SearchCommand>,
}

impl SearchHandle {
    pub fn query(&self, query: &str) {
        let _sent = self
            .commands
            .send(SearchCommand::Query(query.trim().to_owned()));
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Builds and searches the index entirely off the GTK thread. The UI receives only the best
/// bounded result set, so typing remains responsive even while very large trees are being walked.
pub fn index_tree(root: PathBuf) -> (SearchHandle, Receiver<SearchEvent>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let _worker = std::thread::Builder::new()
        .name("strata-search-index".into())
        .spawn(move || {
            let mut index = Vec::new();
            let mut query = String::new();
            let mut matches = Vec::<(i64, SearchItem)>::new();
            let mut last_publish = Instant::now();
            let walker = ignore::WalkBuilder::new(&root)
                .hidden(true)
                .follow_links(false)
                .standard_filters(true)
                .require_git(false)
                .build();

            for entry in walker
                .filter_map(Result::ok)
                .filter(|entry| entry.depth() > 0)
            {
                if worker_cancelled.load(Ordering::Relaxed) {
                    return;
                }
                apply_pending_queries(
                    &command_receiver,
                    &event_sender,
                    &index,
                    &root,
                    &mut query,
                    &mut matches,
                    true,
                );
                let is_directory = entry.file_type().is_some_and(|kind| kind.is_dir());
                let path = entry.into_path();
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let search_path = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_lowercase();
                let item = SearchItem {
                    search_name: name.to_lowercase(),
                    name,
                    is_directory,
                    path,
                    search_path,
                };
                if let Some(score) = fuzzy_score(&item, &query, &root) {
                    insert_match(&mut matches, score, item.clone());
                }
                index.push(item);

                if !query.is_empty() && last_publish.elapsed() >= PUBLISH_INTERVAL {
                    publish(&event_sender, &query, &matches, true);
                    last_publish = Instant::now();
                }
            }

            publish(&event_sender, &query, &matches, false);
            while !worker_cancelled.load(Ordering::Relaxed) {
                match command_receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(SearchCommand::Query(next)) => {
                        query = command_receiver
                            .try_iter()
                            .map(|SearchCommand::Query(query)| query)
                            .last()
                            .unwrap_or(next);
                        matches = score_index(&index, &query, &root);
                        publish(&event_sender, &query, &matches, false);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
    (
        SearchHandle {
            cancelled,
            commands: command_sender,
        },
        event_receiver,
    )
}

fn apply_pending_queries(
    receiver: &Receiver<SearchCommand>,
    sender: &Sender<SearchEvent>,
    index: &[SearchItem],
    root: &Path,
    query: &mut String,
    matches: &mut Vec<(i64, SearchItem)>,
    indexing: bool,
) {
    let Some(next) = receiver
        .try_iter()
        .map(|SearchCommand::Query(query)| query)
        .last()
    else {
        return;
    };
    *query = next;
    *matches = score_index(index, query, root);
    publish(sender, query, matches, indexing);
}

fn score_index(index: &[SearchItem], query: &str, _root: &Path) -> Vec<(i64, SearchItem)> {
    let mut matches = Vec::with_capacity(RESULT_LIMIT);
    let normalized_query = query.trim().to_lowercase();
    for item in index {
        if let Some(score) = fuzzy_score_normalized(item, &normalized_query) {
            insert_match(&mut matches, score, item.clone());
        }
    }
    matches
}

fn insert_match(matches: &mut Vec<(i64, SearchItem)>, score: i64, item: SearchItem) {
    let position = matches
        .binary_search_by(|candidate| candidate.0.cmp(&score).reverse())
        .unwrap_or_else(|position| position);
    if position < RESULT_LIMIT {
        matches.insert(position, (score, item));
        matches.truncate(RESULT_LIMIT);
    }
}

fn publish(
    sender: &Sender<SearchEvent>,
    query: &str,
    matches: &[(i64, SearchItem)],
    indexing: bool,
) {
    if query.is_empty() {
        return;
    }
    let _sent = sender.send(SearchEvent::Results {
        query: query.to_owned(),
        items: matches.iter().map(|(_, item)| item.clone()).collect(),
        indexing,
    });
}

/// Scores ordered character matches, strongly preferring names, contiguous runs and word/path
/// boundaries. Exact substrings rank ahead of looser fuzzy matches.
pub fn fuzzy_score(item: &SearchItem, query: &str, _root: &Path) -> Option<i64> {
    fuzzy_score_normalized(item, &query.trim().to_lowercase())
}

fn fuzzy_score_normalized(item: &SearchItem, query: &str) -> Option<i64> {
    if query.is_empty() {
        return None;
    }
    let mut score = if let Some(position) = item.search_name.find(query) {
        10_000 - position as i64 * 12 - item.search_name.len() as i64
    } else if let Some(position) = item.search_path.find(query) {
        7_000 - position as i64 * 4 - item.search_path.len() as i64
    } else {
        fuzzy_subsequence_score(&item.search_path, query)?
    };
    if item.search_name == query {
        score += 20_000;
    }
    if item.is_directory {
        score += 20;
    }
    Some(score)
}

fn fuzzy_subsequence_score(haystack: &str, needle: &str) -> Option<i64> {
    let mut chars = haystack.char_indices();
    let mut previous = None;
    let mut score = 1_000i64;
    for wanted in needle.chars() {
        let (position, _) = chars.find(|(_, candidate)| *candidate == wanted)?;
        score -= position as i64;
        if previous.is_some_and(|previous| previous + wanted.len_utf8() == position) {
            score += 80;
        }
        if position == 0
            || haystack[..position]
                .chars()
                .next_back()
                .is_some_and(|character| matches!(character, '/' | '-' | '_' | ' ' | '.'))
        {
            score += 45;
        }
        previous = Some(position);
    }
    Some(score)
}

#[cfg(test)]
mod tests;
