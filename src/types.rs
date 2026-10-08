use rusqlite::Connection;
use std::{error::Error, path::PathBuf, sync::mpsc::Receiver};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, PartialEq, Eq)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub done: bool,
    pub project_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Copy)]
pub enum Filter {
    All,
    Pending,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Add,
    Edit,
    ToggleCompletion,
    Delete,
    Move,
    ShowFilter(usize),
    OpenTask(i64),
    Quit,
}

#[derive(Default)]
pub struct TaskDraft {
    pub title: String,
    pub description: String,
    pub description_focused: bool,
}

pub enum Mode {
    Browse,
    Help {
        scroll: u16,
    },
    Edit(Option<i64>, TaskDraft),
    Delete(i64),
    Nuke {
        tasks: usize,
        projects: usize,
        commits: usize,
    },
    Palette {
        query: String,
        selected: usize,
    },
    ProjectEdit(Option<i64>, String),
    ProjectDelete(i64),
    CommitEdit(String),
    NoChanges,
    RemoteEdit(String),
    Move {
        task: i64,
        selected: usize,
    },
}

pub struct Commit {
    pub hash: String,
    pub subject: String,
}

pub struct PullPlan {
    pub(crate) previous_head: Option<String>,
    pub(crate) hash: String,
    pub(crate) branch: String,
    pub(crate) snapshot: Vec<u8>,
}

pub enum SyncResult {
    Message(String),
    Pull(PullPlan),
}

#[derive(Clone)]
pub struct History {
    pub directory: PathBuf,
}

pub struct App {
    pub(crate) db: Connection,
    pub(crate) palette_tasks: Vec<Task>,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub selected_project: usize,
    pub current_project: Option<i64>,
    pub projects_focused: bool,
    pub commits_focused: bool,
    pub uncommitted_changes: bool,
    pub remote_url: String,
    pub(crate) sync_receiver: Option<Receiver<std::result::Result<SyncResult, String>>>,
    pub(crate) history: Option<History>,
    pub commits: Vec<Commit>,
    pub selected_commit: usize,
    pub commit_details: String,
    pub detail_scroll: u16,
    pub(crate) help_page_size: u16,
    pub selected_task: usize,
    pub navigation_focused: bool,
    pub selected_filter: usize,
    pub mode: Mode,
    pub message: String,
}
