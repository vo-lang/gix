#[cfg(feature = "native")]
use std::collections::HashMap;
#[cfg(feature = "native")]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "native")]
use std::sync::Mutex;

#[cfg(feature = "wasm-stubs")]
pub mod wasm_stubs;

#[cfg(feature = "native")]
mod native {
    use super::*;
    use vo_ext::prelude::*;
    use vo_runtime::builtins::error_helper::{write_error_to, write_nil_error};

    use gix::bstr::ByteSlice;
    use serde::Serialize;

    lazy_static::lazy_static! {
        static ref REPOS: Mutex<HashMap<u32, gix::ThreadSafeRepository>> = Mutex::new(HashMap::new());
    }

    static NEXT_ID: AtomicU32 = AtomicU32::new(1);

    // ── JSON output types ───────────────────────────────────────────────────

    #[derive(Serialize)]
    struct CommitOut {
        id: String,
        summary: String,
        author_name: String,
        author_email: String,
        time_unix: i64,
    }

    #[derive(Serialize)]
    struct StatusItemOut {
        path: String,
        status: String,
        index_new: bool,
        index_modified: bool,
        index_deleted: bool,
        worktree_new: bool,
        worktree_modified: bool,
        worktree_deleted: bool,
        conflicted: bool,
    }

    #[derive(Serialize)]
    struct BranchOut {
        name: String,
        is_head: bool,
    }

    #[derive(Serialize)]
    struct StatusOut {
        items: Vec<StatusItemOut>,
    }

    #[derive(Serialize)]
    struct BranchesOut {
        branches: Vec<BranchOut>,
    }

    #[derive(Serialize)]
    struct NameOut {
        name: String,
    }

    #[derive(Serialize)]
    struct OidOut {
        oid: String,
    }

    #[derive(Serialize)]
    struct CommitsOut {
        commits: Vec<CommitOut>,
    }

    #[derive(Serialize)]
    struct PathOut {
        path: String,
    }

    #[derive(Serialize)]
    struct WorkdirOut {
        path: String,
        exists: bool,
    }

    fn to_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
        serde_json::to_vec(value).map_err(|e| e.to_string())
    }

    // ── Repo store ──────────────────────────────────────────────────────────

    fn put_repo(repo: gix::Repository) -> Result<u32, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let ts = repo.into_sync();
        let mut map = REPOS.lock().map_err(|_| "gix lock poisoned".to_string())?;
        map.insert(id, ts);
        Ok(id)
    }

    fn with_repo<F, T>(raw_id: u64, f: F) -> Result<T, String>
    where
        F: FnOnce(gix::Repository) -> Result<T, String>,
    {
        let id = u32::try_from(raw_id).map_err(|_| format!("id out of range: {raw_id}"))?;
        let map = REPOS.lock().map_err(|_| "gix lock poisoned".to_string())?;
        let ts = map.get(&id).ok_or_else(|| format!("invalid repo id {}", id))?;
        let repo = ts.to_thread_local();
        f(repo)
    }

    // ── Repo lifecycle ──────────────────────────────────────────────────────

    fn open_impl(path: &str) -> Result<u32, String> {
        let repo = gix::open(path).map_err(|e| e.to_string())?;
        put_repo(repo)
    }

    fn init_impl(path: &str) -> Result<u32, String> {
        let repo = gix::init(path).map_err(|e| e.to_string())?;
        put_repo(repo)
    }

    fn init_bare_impl(path: &str) -> Result<u32, String> {
        let repo = gix::init_bare(path).map_err(|e| e.to_string())?;
        put_repo(repo)
    }

    fn discover_impl(start_path: &str) -> Result<u32, String> {
        let repo = gix::discover(start_path).map_err(|e| e.to_string())?;
        put_repo(repo)
    }

    fn close_impl(raw_id: u64) -> Result<(), String> {
        let id = u32::try_from(raw_id).map_err(|_| format!("id out of range: {raw_id}"))?;
        let mut map = REPOS.lock().map_err(|_| "gix lock poisoned".to_string())?;
        map.remove(&id).ok_or_else(|| format!("invalid repo id {}", id))?;
        Ok(())
    }

    // ── Queries ─────────────────────────────────────────────────────────────

    fn is_bare_impl(raw_id: u64) -> Result<bool, String> {
        with_repo(raw_id, |repo| Ok(repo.is_bare()))
    }

    fn workdir_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let out = match repo.work_dir() {
                Some(path) => WorkdirOut {
                    path: path.to_string_lossy().to_string(),
                    exists: true,
                },
                None => WorkdirOut {
                    path: String::new(),
                    exists: false,
                },
            };
            to_json_bytes(&out)
        })
    }

    fn repo_path_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            to_json_bytes(&PathOut {
                path: repo.git_dir().to_string_lossy().to_string(),
            })
        })
    }

    fn current_branch_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let name = repo
                .head_name()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "HEAD is detached".to_string())?;
            let short = name.shorten().to_string();
            to_json_bytes(&NameOut { name: short })
        })
    }

    fn head_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let commit = repo.head_commit().map_err(|e| e.to_string())?;
            let author = commit.author().map_err(|e| e.to_string())?;
            to_json_bytes(&CommitOut {
                id: commit.id().to_string(),
                summary: commit
                    .message_raw_sloppy()
                    .to_str_lossy()
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
                author_name: author.name.to_string(),
                author_email: author.email.to_string(),
                time_unix: author.time.seconds,
            })
        })
    }

    fn head_oid_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let id = repo.head_id().map_err(|e| e.to_string())?;
            to_json_bytes(&OidOut {
                oid: id.to_string(),
            })
        })
    }

    fn branches_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let head_name = repo
                .head_name()
                .ok()
                .flatten()
                .map(|n| n.shorten().to_string());

            let platform = repo.references().map_err(|e| e.to_string())?;
            let local = platform.local_branches().map_err(|e| e.to_string())?;

            let mut branches = Vec::new();
            for reference in local.flatten() {
                let name = reference.name().shorten().to_string();
                let is_head = head_name.as_ref().map(|h| h == &name).unwrap_or(false);
                branches.push(BranchOut { name, is_head });
            }

            to_json_bytes(&BranchesOut { branches })
        })
    }

    fn log_impl(raw_id: u64, raw_max: i64) -> Result<Vec<u8>, String> {
        let max =
            usize::try_from(raw_max).map_err(|_| format!("max out of range: {raw_max}"))?;
        with_repo(raw_id, |repo| {
            let head = repo.head_id().map_err(|e| e.to_string())?;
            let walk = head.ancestors().all().map_err(|e| e.to_string())?;

            let mut commits = Vec::new();
            for info in walk.take(max.max(1)) {
                let info = info.map_err(|e| e.to_string())?;
                let object = repo.find_object(info.id).map_err(|e| e.to_string())?;
                let commit = object.try_into_commit().map_err(|e| e.to_string())?;
                let author = commit.author().map_err(|e| e.to_string())?;
                commits.push(CommitOut {
                    id: info.id.to_string(),
                    summary: commit
                        .message_raw_sloppy()
                        .to_str_lossy()
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    author_name: author.name.to_string(),
                    author_email: author.email.to_string(),
                    time_unix: author.time.seconds,
                });
            }

            to_json_bytes(&CommitsOut { commits })
        })
    }

    fn status_impl(raw_id: u64) -> Result<Vec<u8>, String> {
        with_repo(raw_id, |repo| {
            let platform = repo
                .status(gix::progress::Discard)
                .map_err(|e| e.to_string())?;

            // index ↔ worktree changes
            let iter = platform
                .into_index_worktree_iter(None)
                .map_err(|e| e.to_string())?;

            let mut items = Vec::new();
            for entry in iter {
                let entry = entry.map_err(|e| e.to_string())?;
                use gix::status::index_worktree::iter::Item as WtItem;
                match entry {
                    WtItem::Modification {
                        rela_path, status, ..
                    } => {
                        let path = rela_path.to_str_lossy().to_string();
                        use gix_status::index_as_worktree::EntryStatus;
                        let (label, wt_new, wt_mod, wt_del, conflicted) = match &status {
                            EntryStatus::Conflict(_) => ("CONFLICTED", false, false, false, true),
                            EntryStatus::NeedsUpdate(_) => continue,
                            EntryStatus::IntentToAdd => ("WT_NEW", true, false, false, false),
                            EntryStatus::Change(change) => {
                                let (l, n, m, d) = classify_worktree_change(change);
                                (l, n, m, d, false)
                            }
                        };
                        items.push(StatusItemOut {
                            path,
                            status: label.to_string(),
                            index_new: false,
                            index_modified: false,
                            index_deleted: false,
                            worktree_new: wt_new,
                            worktree_modified: wt_mod,
                            worktree_deleted: wt_del,
                            conflicted,
                        });
                    }
                    WtItem::DirectoryContents { entry, .. } => {
                        let path = entry.rela_path.to_str_lossy().to_string();
                        items.push(StatusItemOut {
                            path,
                            status: "WT_NEW".to_string(),
                            index_new: false,
                            index_modified: false,
                            index_deleted: false,
                            worktree_new: true,
                            worktree_modified: false,
                            worktree_deleted: false,
                            conflicted: false,
                        });
                    }
                    WtItem::Rewrite {
                        dirwalk_entry,
                        source,
                        ..
                    } => {
                        let dest_path = dirwalk_entry.rela_path.to_str_lossy().to_string();
                        let src_path = source.rela_path().to_str_lossy().to_string();
                        items.push(StatusItemOut {
                            path: format!("{} -> {}", src_path, dest_path),
                            status: "RENAMED".to_string(),
                            index_new: false,
                            index_modified: false,
                            index_deleted: false,
                            worktree_new: true,
                            worktree_modified: true,
                            worktree_deleted: false,
                            conflicted: false,
                        });
                    }
                }
            }

            to_json_bytes(&StatusOut { items })
        })
    }

    /// Classify a worktree entry status into human-readable label + flag tuple.
    fn classify_worktree_change<T, U>(
        change: &gix_status::index_as_worktree::Change<T, U>,
    ) -> (&'static str, bool, bool, bool) {
        use gix_status::index_as_worktree::Change;
        match change {
            Change::Removed => ("WT_DELETED", false, false, true),
            Change::Type => ("WT_MODIFIED", false, true, false),
            Change::Modification { .. } => ("WT_MODIFIED", false, true, false),
            Change::SubmoduleModification(_) => ("WT_MODIFIED", false, true, false),
        }
    }

    // ── RawCall entry point ─────────────────────────────────────────────────

    #[vo_fn("github.com/vo-lang/gix", "RawCall")]
    pub fn raw_call(call: &mut ExternCallContext) -> ExternResult {
        let op = call.arg_str(0).to_string();
        let input = call.arg_str(1).to_string();

        let result = dispatch(&op, &input);

        match result {
            Ok(bytes) => {
                let out_ref = call.alloc_bytes(&bytes);
                call.ret_ref(0, out_ref);
                write_nil_error(call, 1);
            }
            Err(msg) => {
                call.ret_nil(0);
                write_error_to(call, 1, &msg);
            }
        }
        ExternResult::Ok
    }

    fn dispatch(op: &str, input: &str) -> Result<Vec<u8>, String> {
        match op {
            "open" => {
                let v: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let path = v["path"].as_str().ok_or("missing 'path'".to_string())?;
                let id = open_impl(path)?;
                to_json_bytes(&serde_json::json!({ "id": id }))
            }
            "init" => {
                let v: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let path = v["path"].as_str().ok_or("missing 'path'".to_string())?;
                let id = init_impl(path)?;
                to_json_bytes(&serde_json::json!({ "id": id }))
            }
            "discover" => {
                let v: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let start = v["start_path"]
                    .as_str()
                    .ok_or("missing 'start_path'".to_string())?;
                let id = discover_impl(start)?;
                to_json_bytes(&serde_json::json!({ "id": id }))
            }
            "status" => {
                let id = parse_id(input)?;
                status_impl(id)
            }
            "branches" => {
                let id = parse_id(input)?;
                branches_impl(id)
            }
            "current_branch" => {
                let id = parse_id(input)?;
                current_branch_impl(id)
            }
            "head" => {
                let id = parse_id(input)?;
                head_impl(id)
            }
            "head_oid" => {
                let id = parse_id(input)?;
                head_oid_impl(id)
            }
            "log" => {
                let v: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let id = v["id"].as_u64().ok_or("missing 'id'".to_string())?;
                let max = v["max"].as_i64().unwrap_or(50);
                log_impl(id, max)
            }
            "is_bare" => {
                let id = parse_id(input)?;
                is_bare_impl(id).map(|b| serde_json::to_vec(&b).unwrap())
            }
            "workdir" => {
                let id = parse_id(input)?;
                workdir_impl(id)
            }
            "repo_path" => {
                let id = parse_id(input)?;
                repo_path_impl(id)
            }
            "close" => {
                let id = parse_id(input)?;
                close_impl(id).map(|_| b"null".to_vec())
            }
            _ => Err(format!("unknown gix op: {op}")),
        }
    }

    fn parse_id(input: &str) -> Result<u64, String> {
        let v: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
        v["id"].as_u64().ok_or_else(|| "missing 'id'".to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::Value;
        use std::fs;
        use std::path::Path;
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        fn temp_dir(prefix: &str) -> std::path::PathBuf {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("vo_gix_{prefix}_{nanos}"));
            fs::create_dir_all(&dir).expect("failed to create temp dir");
            dir
        }

        fn git(dir: &Path, args: &[&str]) -> String {
            let out = Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_AUTHOR_NAME", "vo-test")
                .env("GIT_AUTHOR_EMAIL", "vo@test.local")
                .env("GIT_COMMITTER_NAME", "vo-test")
                .env("GIT_COMMITTER_EMAIL", "vo@test.local")
                .output()
                .expect("failed to run git");
            assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).to_string()
        }

        #[test]
        fn repo_lifecycle() {
            let dir = temp_dir("repo");
            let repo_path = dir.to_string_lossy().to_string();

            // init via git CLI
            git(&dir, &["init"]);
            git(&dir, &["config", "user.name", "vo-test"]);
            git(&dir, &["config", "user.email", "vo@test.local"]);

            // open via gix
            let repo_id = open_impl(&repo_path).expect("open_impl should succeed");

            // no commits yet → head should fail
            assert!(
                head_impl(repo_id as u64).is_err(),
                "head should fail before first commit"
            );

            // create a commit via CLI
            fs::write(dir.join("a.txt"), "hello\n").expect("write");
            git(&dir, &["add", "."]);
            git(&dir, &["-c", "commit.gpgsign=false", "commit", "-m", "initial"]);

            // branches
            let branch_payload = branches_impl(repo_id as u64).expect("branches should succeed");
            let branch_json: Value =
                serde_json::from_slice(&branch_payload).expect("branches json");
            let branches = branch_json["branches"]
                .as_array()
                .expect("branches array");
            assert!(!branches.is_empty(), "should have at least one branch");

            // current branch
            let cb_payload =
                current_branch_impl(repo_id as u64).expect("current branch should succeed");
            let cb_json: Value = serde_json::from_slice(&cb_payload).expect("current branch json");
            let name = cb_json["name"].as_str().expect("branch name");
            assert!(!name.is_empty(), "branch name should not be empty");

            // is_bare
            let is_bare = is_bare_impl(repo_id as u64).expect("is_bare should succeed");
            assert!(!is_bare, "init repo should not be bare");

            // workdir
            let wd_payload = workdir_impl(repo_id as u64).expect("workdir should succeed");
            let wd_json: Value = serde_json::from_slice(&wd_payload).expect("workdir json");
            assert_eq!(wd_json["exists"], Value::Bool(true));

            // repo_path
            let rp_payload = repo_path_impl(repo_id as u64).expect("repo_path should succeed");
            let rp_json: Value = serde_json::from_slice(&rp_payload).expect("repo_path json");
            let internal = rp_json["path"].as_str().expect("repo path str");
            assert!(internal.contains(".git"), "repo path should point to .git");

            // log
            let log_payload = log_impl(repo_id as u64, 1).expect("log should succeed");
            let log_json: Value = serde_json::from_slice(&log_payload).expect("log json");
            let commits = log_json["commits"].as_array().expect("commits array");
            assert_eq!(commits.len(), 1, "log(1) should return 1 commit");

            // head
            let head_payload = head_impl(repo_id as u64).expect("head should succeed");
            let head_json: Value = serde_json::from_slice(&head_payload).expect("head json");
            assert!(head_json["id"].as_str().is_some(), "head id should be present");

            // status — modify tracked file
            fs::write(dir.join("a.txt"), "changed\n").expect("modify");
            let st_payload = status_impl(repo_id as u64).expect("status should succeed");
            let st_json: Value = serde_json::from_slice(&st_payload).expect("status json");
            let status_items = st_json["items"].as_array().expect("status items array");
            assert!(
                !status_items.is_empty(),
                "status should report modified file"
            );

            // close
            close_impl(repo_id as u64).expect("close should succeed");
            assert!(
                status_impl(repo_id as u64).is_err(),
                "status on closed repo should fail"
            );

            fs::remove_dir_all(&dir).expect("cleanup");
        }

        #[test]
        fn invalid_repo_id_fails() {
            let invalid = 9_999_999u64;
            assert!(status_impl(invalid).is_err());
            assert!(branches_impl(invalid).is_err());
            assert!(close_impl(invalid).is_err());
        }
    }
}

#[cfg(feature = "native")]
vo_ext::export_extensions!();
