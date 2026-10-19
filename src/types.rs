use chrono::NaiveDate;
use ratatui::layout::Rect;
use rusqlite::Connection;
use std::{error::Error, ops::Range, path::PathBuf, sync::mpsc::Receiver};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, PartialEq, Eq)]
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
    ResetHistory,
    Quit,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum TaskField {
    #[default]
    Title,
    Description,
    DueDate,
}

#[derive(Clone)]
pub struct DatePhrase {
    pub range: Range<usize>,
    pub date: NaiveDate,
}

#[derive(Clone)]
pub struct AutoDate {
    pub previous_due_date: String,
    pub field: TaskField,
    pub phrase: DatePhrase,
}

#[derive(Clone, Default)]
pub struct TaskDraft {
    pub cursors: Box<[crate::editor::TextCursor; 3]>,
    pub title: String,
    pub description: String,
    pub due_date: String,
    pub auto_date: Option<AutoDate>,
    pub ignored_dates: Vec<(TaskField, String)>,
    pub focus: TaskField,
    pub subtasks: Vec<SubtaskDraft>,
    pub depth: usize,
}

#[derive(Clone)]
pub struct SubtaskDraft {
    pub id: Option<i64>,
    pub done: bool,
    pub draft: TaskDraft,
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
    ResetHistory {
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

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum TaskPane {
    #[default]
    Details,
    Subtasks,
}

pub struct TaskView {
    pub focus: TaskPane,
    pub task: Task,
    pub children: Vec<Task>,
    pub selected_child: usize,
    pub scroll: u16,
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
    pub task_view: Option<TaskView>,
    pub(crate) task_path: Vec<i64>,
    pub(crate) new_task_parent: Option<i64>,
    pub(crate) viewport: Rect,
    pub(crate) input_cursor: crate::editor::TextCursor,
    pub navigation_focused: bool,
    pub selected_filter: usize,
    pub mode: Mode,
    pub message: String,
}
