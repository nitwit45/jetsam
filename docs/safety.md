# Safety review

Reviewed 10 Nov 2026 against commit `548fa4b`.

This is a maintainer review, not an independent audit.

Each row is one claim from the README, the code that enforces it, and the test that checks it. Paths are relative to `src-tauri/src/`. Run the tests with `cargo test` from `src-tauri/`. "No test" means the claim holds in the code today but nothing fails if it stops holding; those rows are the next tests to write.

## Deleting files

| Claim | Code | Test |
|---|---|---|
| A clean acts only on items the same app process produced in its latest scan, or its latest Aged listing. | `cleaner::ScanStore::select` | `cleaner::store_keeps_aged_results_beside_the_latest_scan` |
| In the app, every item is listed with its path and size before it can be removed, and a permanent delete needs `delete` typed. **Move to Trash** and the Trash button on an Aged Files row act at once. Headless and scheduled runs show nothing first. | `src/pages/Clean.tsx`, `src/pages/Aged.tsx`, `src/components/clean/CleanModals.tsx` (UI), `lib::run_headless` | No test |
| Items flagged `report-only`, `dirty`, `unmerged`, `git-tracked` or `recent` cannot be selected and are refused at delete time. | `types::Item::selectable`, `cleaner::verify` | `types::selectable_excludes_every_blocking_flag`, `cleaner::verify_refuses_symlink_kind_mismatch_flags` |
| Each path is checked again right before removal: must exist, be absolute and normalised, not be a symlink, keep its type, keep its modified time, and (for a file) keep its size. | `cleaner::verify` | `cleaner::verify_refuses_drift`, `cleaner::verify_refuses_symlink_kind_mismatch_flags`, `cleaner::verify_refuses_unnormalised_paths`, `cleaner::verify_accepts_unchanged_file_and_dir` |
| Deny-listed paths are refused in every mode, including permanent deletes. | `rules::deny_roots`, `rules::is_denied`, `cleaner::verify` | `rules::deny_list_roots`, `rules::deny_list_patterns`, `cleaner::verify_refuses_denied_and_protected` |
| Protected roots (home, Library, Desktop, Documents and the rest) can never be a target themselves. | `rules::protected_roots`, `cleaner::verify` | `rules::protected_roots_block_targets`, `cleaner::verify_refuses_denied_and_protected` |
| A folder with a `.git` file or folder directly inside it is never removed, except a worktree item, which `worktrees::verify_removable` checks instead. Only the top level is checked: a `.git` nested deeper goes with its parent. | `rules::contains_git_entry`, `rules::is_artifact_dir`, `cleaner::verify` | `rules::a_candidate_holding_git_is_never_an_artifact`, `cleaner::verify_refuses_a_directory_holding_git` |
| Generic build-folder names count only next to a project file or inside a repo; `target` needs `Cargo.toml`, `bin` and `obj` need a .NET project file. | `rules::artifact_rule`, `rules::is_artifact_dir` | `rules::artifact_sibling_rules`, `rules::always_names_need_a_repo_or_a_manifest`, `categories::deep_walk_skips_repos_and_loose_folders_named_like_artifacts` |
| Git-tracked build folders are flagged, and a git failure counts as tracked. | `categories::git_tracked` | `categories::recent_and_tracked_artifacts_are_listed_but_flagged`, `categories::git_failure_fails_closed` |
| Build folders in a repo active within the recent window are flagged `recent` below Full; unknown activity counts as recent; Full has no window. | `categories::is_recent`, `categories::Ctx::new` | `categories::recent_and_tracked_artifacts_are_listed_but_flagged`, `categories::git_failure_fails_closed` |
| Installers are `.dmg` or `.pkg` only and older than 14 days; archives are never included. | `rules::is_installer` | `rules::installer_age` |
| AI session logs start at Deep and are marked `review`, never preselected. Claude Code memory is never listed. | `categories::ai_sessions`, `rules::is_memory_dir` | `categories::ai_sessions_need_review_and_start_at_deep`, `rules::deny_list_patterns` |
| A path is listed in one category only: App caches skips the pip, Playwright and VS Code updater caches that Package manager caches lists. | `categories::app_caches`, `rules::pkg_cache_paths` | `categories::no_path_is_listed_in_two_categories` |
| Symlinks are never followed or listed, and walks do not cross volumes. | `scanner::walk`, `scanner::children`, `scanner::size_of` | `scanner::size_skips_symlinks_and_counts_blocks`, `scanner::walk_prunes` |

## Trash, undo and the log

| Claim | Code | Test |
|---|---|---|
| Files go to the Trash through NSFileManager, and Jetsam records where each one landed. | `cleaner::trash_context`, `cleaner::move_to_trash`, `cleaner::locate_in_trash` | `cleaner::nsfilemanager_trash_records_where_the_file_landed` (ignored by default: it uses the real `~/.Trash`; run with `cargo test -- --ignored`) |
| If the Trash refuses an item it stays on disk with reason `trash-refused`; nothing falls back to a permanent delete. | `cleaner::move_to_trash`, `cleaner::TRASH_REFUSED` | `cleaner::items_already_in_the_trash_are_refused_not_removed` |
| A permanent delete of refused items is offered only behind typing `delete`. | `src/components/clean/CleanModals.tsx` (UI) | No test |
| Undo acts on the most recent batch in the ops log, a scheduled run's included. It restores only the exact Trash entry recorded at delete time, never over a path that exists again. Items trashed on another volume are not located, so they cannot be restored. | `cleaner::restore`, `cleaner::undo_last`, `cleaner::read_last_batch`, `cleaner::locate_in_trash` | `cleaner::undo_restores_the_recorded_trash_entry_and_skips_conflicts` |
| Every clean appends one JSON line per item to `~/Library/Application Support/Jetsam/ops.log`. | `cleaner::append_ops`, `cleaner::ops_log_path` | `cleaner::clean_permanent_deletes_and_logs` |
| Headless and scheduled runs send files to the Trash and skip `review` items. They also run the Homebrew and Docker command items, which are permanent. | `lib::run_headless` | No test |
| Removing a worktree runs `git worktree prune` in its repository, which permanently deletes that worktree's entry under `.git/worktrees/` and any other stale entry there. Undo restores the folder but does not register it as a worktree again. | `cleaner::clean`, `cleaner::undo_last` | No test |
| Headless cleaning runs in Light and Standard only; Deep and Full need `--dry-run`. The schedule refuses Deep and Full. | `main::parse_args`, `system::schedule` | `main::headless_mode_messages`, `system::schedule_refuses_deep_modes` |
| Any argument the binary does not know, a misspelled `--dry-run` included, is refused with exit status 2 before anything runs. `--mode=X` and `--mode X` are the same. | `main::parse_args` | `main::unknown_argument_is_refused`, `main::misspelled_dry_run_is_refused`, `main::mode_equals_form_is_parsed` |

## Worktrees

| Claim | Code | Test |
|---|---|---|
| Only worktrees under `.claude/worktrees/` are candidates, never the main worktree. | `worktrees::is_candidate`, `worktrees::scan` | `worktrees::porcelain_and_candidates` |
| Ignored build folders and `.DS_Store` are allowed; any other uncommitted or ignored file makes it dirty. An ignored path counts as build output only when a folder in it passes the same build-folder test as the Deep scan, with the worktree as the repository, so an ignored `bin/.env` with no .NET project beside `bin` makes it dirty. | `worktrees::status_is_dirty`, `worktrees::is_disposable`, `rules::is_artifact_dir` | `worktrees::ignored_artifacts_are_clean_but_ignored_data_is_not`, `worktrees::ignored_folders_need_the_same_context_as_the_deep_scan` |
| A git failure counts as dirty, unmerged and active. | `worktrees::is_dirty`, `worktrees::is_unmerged`, `worktrees::is_recently_active` | `worktrees::git_failure_counts_as_dirty_unmerged_and_active` |
| Git activity in the last 24 hours, or any process working inside it, marks it in use; an unreadable process list means every worktree is in use. | `worktrees::is_recently_active`, `worktrees::ProcessCwds` | `worktrees::process_cwds_match_by_path_prefix`, `worktrees::live_worktree_checks` |
| Processes, dirty files and unmerged commits are checked again at delete time. | `worktrees::verify_removable`, `cleaner::verify` | `worktrees::live_worktree_checks`, `cleaner::verify_rechecks_worktrees_live` |

## Commands and Docker

| Claim | Code | Test |
|---|---|---|
| Command items may run only `brew` or `docker`. | `cleaner::COMMAND_BINARIES`, `cleaner::verify_command` | `cleaner::verify_refuses_volume_commands_and_unknown_binaries` |
| Docker commands must name a context and may only be `image prune` or `builder prune`; any argument containing `volume` refuses the item. | `cleaner::verify_docker_argv`, `cleaner::verify_command` | `cleaner::verify_refuses_volume_commands_and_unknown_binaries` |
| Docker runs only when the active context's endpoint is a local `unix://` socket. | `categories::is_local_docker_endpoint`, `categories::local_docker_context` | `categories::docker_acts_only_on_a_local_socket` |
| The Docker size counts images and build cache only, never containers or volumes. | `categories::parse_docker_df` | `categories::docker_df_counts_only_what_is_pruned` |
| Command lines never come from the UI. | `types::Item::apply` is `#[serde(skip)]` | No test |

## Memory

| Claim | Code | Test |
|---|---|---|
| A running Claude Code agent, its descendants and every process hosting it are protected. | `ram::is_claude`, `ram::agents_of`, `ram::tag` | `ram::claude_descendants_and_ancestors_are_protected`, `ram::npm_claude_code_is_detected` |
| A process an agent started stays protected while the agent runs, and for 120 seconds after the agent exits. | `ram::Sampler::record`, `ram::Sampler::adopt`, `ram::CLAUDE_GRACE_SEC` | `ram::background_server_of_live_agent_outlives_grace`, `ram::reparented_claude_child_stays_protected_within_grace`, `ram::young_process_in_agent_group_or_session_is_adopted` |
| Jetsam itself, its ancestors, executables under `/System/`, `/usr/libexec/`, `/usr/sbin/`, `/usr/bin/`, `/sbin/`, `/bin/` or `/Library/Apple/`, running `com.apple.` launchd jobs, and `WindowServer`, `loginwindow`, `kernel_task` and `launchd` are protected. | `ram::tag`, `ram::SYSTEM_PREFIXES`, `ram::is_system_job` | `ram::protected_basics`, `ram::system_executables_and_apple_jobs_are_protected` |
| The frontmost app and its bundle helpers are protected. While Jetsam is in front, the last other app a 10-second sample saw in front is protected instead: its whole bundle, and its pid only while the same process holds it. | `ram::Frontmost::owns`, `ram::Sampler::note_frontmost`, `ram::Sampler::frontmost_to_protect`, `ram::rows` | `ram::frontmost_bundle_helpers_are_protected`, `ram::last_non_jetsam_frontmost_is_protected_while_jetsam_is_in_front` |
| An orphan must have been seen with a real parent, must not be a launchd job, and a reused pid starts over. | `ram::was_orphaned`, `ram::Sampler::update_lineage` | `ram::reparented_node_with_dead_parent_is_orphan`, `ram::daemonised_at_birth_node_is_not_orphan`, `ram::reused_pid_forgets_its_parent` |
| A headless browser is protected while its driver lives; a user's own Chrome is not headless. | `ram::is_headless_root`, `ram::headless_groups` | `ram::headless_root_detection`, `ram::headless_trees_group_and_tag` |
| Virtual machines are labelled and never ended. | `ram::is_vm`, `ram::reclaim` | `ram::vm_is_labelled_never_actioned` checks the label; the refusal in `reclaim` has no test |
| A ticked protected process is refused at reclaim time, not only hidden in the UI. | `ram::reclaim` | No test |
| Ending is SIGTERM, 3 seconds, then SIGKILL. | `ram::end_processes` | No test |
| VS Code and Cursor are not restarted while an agent runs inside them. | `ram::restart_editor`, `ram::agents_in` | `ram::claude_descendants_and_ancestors_are_protected` checks the count and `ram::restart_refusal_reads_naturally` the message; the refusal itself has no test |
