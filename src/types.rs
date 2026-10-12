use chrono::NaiveDate;
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
    pub parent_id: Option<i64>,
    pub subtask_count: usize,
    pub due_date: Option<NaiveDate>,
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

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum TaskField {
    #[default]
    Title,
    Description,
    DueDate,
    Subtasks,
}

#[derive(Clone, Default)]
pub struct TaskDraft {
    pub title: String,
    pub description: String,
    pub due_date: String,
    pub focus: TaskField,
    pub subtasks: Vec<SubtaskDraft>,
    pub selected_subtask: usize,
    pub depth: usize,
}

#[derive(Clone)]
pub struct SubtaskDraft {
    pub id: Option<i64>,
    pub done: bool,
    pub draft: TaskDraft,
}

pub struct EditorFrame {
    pub id: Option<i64>,
    pub draft: TaskDraft,
    pub adding: bool,
}

pub enum Mode {
    Browse,
    Help {
        scroll: u16,
    },
    Edit(Option<i64>, TaskDraft),
    SubtaskDelete(Option<i64>, TaskDraft),
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
    pub(crate) editor_stack: Vec<EditorFrame>,
    pub message: String,
}
