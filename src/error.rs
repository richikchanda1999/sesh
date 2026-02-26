use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SeshError {
    #[error("sesh.toml not found at {0}")]
    ConfigNotFound(PathBuf),

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("no git repos found in {0}")]
    NoReposFound(PathBuf),

    #[error("session '{0}' already exists")]
    SessionExists(String),

    #[error("session '{0}' not found")]
    SessionNotFound(String),

    #[error("branch '{branch}' already exists in repo '{repo}'")]
    BranchExists { repo: String, branch: String },

    #[error("base branch '{branch}' not found in repo '{repo}'")]
    BaseBranchNotFound { repo: String, branch: String },

    #[error("git command failed in {repo}: {message}")]
    GitError { repo: String, message: String },

    #[error("worktree creation failed for {repo}: {message}")]
    WorktreeError { repo: String, message: String },

    #[error("prerequisite not found: {0}")]
    PrerequisiteNotFound(String),

    #[error("invalid branch name: {0}")]
    InvalidBranchName(String),

    #[error("script failed: {0}")]
    ScriptError(String),

    #[error("no repos selected")]
    NoReposSelected,
}
